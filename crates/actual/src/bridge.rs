use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use secrecy::{ExposeSecret, Secret};
use serde::Serialize;
use serde_json::Value;

use crate::ActualResult;
use crate::types::{ImportTransactionRequest, SaveTransaction};

/// Timeouts governing a [`crate::session::BridgeSession`].
#[derive(Debug, Clone, Copy)]
pub struct BridgeTimeouts {
    /// `open` (api.init + downloadBudget of a potentially large budget file).
    pub open: Duration,
    /// Any single non-open, non-close operation.
    pub operation: Duration,
    /// `close` (final sync + shutdown).
    pub close: Duration,
    /// How long to wait for another process to release the data-dir lock.
    pub lock: Duration,
}

impl Default for BridgeTimeouts {
    fn default() -> Self {
        Self {
            open: Duration::from_secs(300),
            operation: Duration::from_secs(120),
            close: Duration::from_secs(90),
            lock: Duration::from_secs(120),
        }
    }
}

/// All the configuration the bridge needs to talk to an Actual server.
///
/// Everything except `node_bin` and `bridge_script` is forwarded as env vars
/// so secrets don't appear on the command line.
#[derive(Debug, Clone)]
pub struct BridgeConfig {
    pub node_bin: PathBuf,
    pub bridge_script: PathBuf,
    pub server_url: String,
    pub password: Secret<String>,
    pub sync_id: Secret<String>,
    pub cache_dir: PathBuf,
    pub timeouts: BridgeTimeouts,
}

impl BridgeConfig {
    /// Env vars forwarded to the bridge process by [`crate::session::BridgeSession::open`].
    pub(crate) fn env(&self) -> HashMap<&'static str, String> {
        let mut env: HashMap<&'static str, String> = HashMap::new();
        env.insert("ACTUAL_SERVER_URL", self.server_url.clone());
        env.insert("ACTUAL_PASSWORD", self.password.expose_secret().clone());
        env.insert("ACTUAL_SYNC_ID", self.sync_id.expose_secret().clone());
        env.insert(
            "ACTUAL_DATA_DIR",
            self.cache_dir.to_string_lossy().into_owned(),
        );
        env
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "operation", content = "args")]
pub enum BridgeRequest {
    #[serde(rename = "open")]
    Open,
    #[serde(rename = "close")]
    Close,
    #[serde(rename = "list-accounts")]
    ListAccounts,
    #[serde(rename = "get-balance", rename_all = "camelCase")]
    GetBalance { account_id: String },
    #[serde(rename = "get-balance-at", rename_all = "camelCase")]
    GetBalanceAt { account_id: String, date: String },
    #[serde(rename = "get-last-transaction", rename_all = "camelCase")]
    GetLastTransaction { account_id: String },
    #[serde(rename = "ensure-payee")]
    EnsurePayee { name: String },
    #[serde(rename = "get-account-note", rename_all = "camelCase")]
    GetAccountNote { account_id: String },
    #[serde(rename = "find-transaction", rename_all = "camelCase")]
    FindTransaction {
        account_id: String,
        date: String,
        payee_name: String,
    },
    #[serde(rename = "add-transaction")]
    AddTransaction(SaveTransaction),
    #[serde(rename = "import-transaction")]
    ImportTransaction(ImportTransactionRequest),
}

impl BridgeRequest {
    /// Splits into the wire operation name and its JSON `args` payload
    pub(crate) fn wire_parts(&self) -> ActualResult<(String, Value)> {
        let value = serde_json::to_value(self)?;
        let name = value
            .get("operation")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let args = value
            .get("args")
            .cloned()
            .unwrap_or_else(|| Value::Object(Default::default()));
        Ok((name, args))
    }
}

#[async_trait]
pub trait BridgeInvoker: Send + Sync {
    async fn invoke(&self, request: BridgeRequest) -> ActualResult<Value>;
}
