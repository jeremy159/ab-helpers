use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;

use async_trait::async_trait;
use serde_json::Value;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use crate::error::{ApiError, Error};
use crate::ActualResult;

/// All the configuration the bridge needs to talk to an Actual server.
///
/// Everything except `node_bin` and `bridge_script` is forwarded as env vars
/// so secrets don't appear on the command line.
#[derive(Debug, Clone)]
pub struct BridgeConfig {
    pub node_bin: PathBuf,
    pub bridge_script: PathBuf,
    pub server_url: String,
    pub password: String,
    pub sync_id: String,
    pub e2e_password: Option<String>,
    pub data_dir: PathBuf,
}

/// Object-safe trait: the wire types are JSON values so this can live behind
/// `Arc<dyn BridgeInvoker>` and be swapped out in tests.
#[async_trait]
pub trait BridgeInvoker: Send + Sync {
    async fn invoke(&self, subcommand: &str, args: Value) -> ActualResult<Value>;
}

#[async_trait]
impl BridgeInvoker for BridgeConfig {
    async fn invoke(&self, subcommand: &str, args: Value) -> ActualResult<Value> {
        let args_json = serde_json::to_string(&args)?;

        let mut env: HashMap<&str, String> = HashMap::new();
        env.insert("ACTUAL_SERVER_URL", self.server_url.clone());
        env.insert("ACTUAL_PASSWORD", self.password.clone());
        env.insert("ACTUAL_SYNC_ID", self.sync_id.clone());
        env.insert(
            "ACTUAL_DATA_DIR",
            self.data_dir.to_string_lossy().into_owned(),
        );
        if let Some(p) = self.e2e_password.as_ref() {
            env.insert("ACTUAL_E2E_PASSWORD", p.clone());
        }

        tracing::debug!(?subcommand, "invoking actual bridge");

        let mut cmd = Command::new(&self.node_bin);
        cmd.arg(&self.bridge_script)
            .arg(subcommand)
            .arg("--json")
            .arg(&args_json)
            .envs(env)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            Error::Bridge(format!(
                "failed to spawn `{} {}`: {e}",
                self.node_bin.display(),
                self.bridge_script.display()
            ))
        })?;

        let mut stdout = String::new();
        if let Some(mut s) = child.stdout.take() {
            s.read_to_string(&mut stdout).await?;
        }
        let mut stderr = String::new();
        if let Some(mut s) = child.stderr.take() {
            s.read_to_string(&mut stderr).await?;
        }
        let _status = child.wait().await?;

        let value: Value = serde_json::from_str(stdout.trim()).map_err(|e| {
            Error::BridgeProtocol(format!(
                "could not parse bridge stdout as JSON: {e}\nstdout: {}\nstderr: {}",
                stdout.trim(),
                stderr.trim()
            ))
        })?;

        if let Some(err_obj) = value.get("error") {
            let api_err: ApiError = serde_json::from_value(err_obj.clone())?;
            return Err(Error::Api(api_err));
        }
        Ok(value)
    }
}
