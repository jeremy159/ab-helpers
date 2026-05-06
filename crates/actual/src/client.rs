use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;

use crate::bridge::{BridgeConfig, BridgeInvoker};
use crate::error::ActualResult;
use crate::types::{
    Account, AddTransactionResponse, BalanceResponse, EnsurePayeeResponse, ImportTransaction,
    ImportTransactionRequest, LastTransaction, LastTransactionResponse, ListAccountsResponse,
    SaveTransaction,
};

/// High-level Rust client.
///
/// Holds an `Arc<dyn BridgeInvoker>` so production code uses the real
/// `BridgeConfig` and tests can swap in a fake invoker.
#[derive(Clone)]
pub struct Client {
    invoker: Arc<dyn BridgeInvoker>,
}

impl Client {
    pub fn new(config: BridgeConfig) -> Self {
        Self {
            invoker: Arc::new(config),
        }
    }

    pub fn with_invoker(invoker: Arc<dyn BridgeInvoker>) -> Self {
        Self { invoker }
    }
}

#[async_trait]
pub trait AccountRequests: Send + Sync {
    async fn list_accounts(&self) -> ActualResult<Vec<Account>>;
    async fn get_account_balance(&self, account_id: &str) -> ActualResult<i64>;
    async fn get_last_transaction(&self, account_id: &str) -> ActualResult<LastTransaction>;
    async fn ensure_payee(&self, name: &str) -> ActualResult<String>;
}

#[async_trait]
pub trait TransactionRequests: Send + Sync {
    async fn add_transaction(&self, tx: SaveTransaction) -> ActualResult<AddTransactionResponse>;
    async fn get_balance_at(&self, account_id: &str, date: chrono::NaiveDate) -> ActualResult<i64>;
    async fn import_transaction(&self, tx: ImportTransaction) -> ActualResult<String>;
}

#[async_trait]
impl AccountRequests for Client {
    async fn list_accounts(&self) -> ActualResult<Vec<Account>> {
        let value = self.invoker.invoke("list-accounts", json!({})).await?;
        let resp: ListAccountsResponse = serde_json::from_value(value)?;
        Ok(resp.accounts)
    }

    async fn get_account_balance(&self, account_id: &str) -> ActualResult<i64> {
        let value = self
            .invoker
            .invoke("get-balance", json!({ "accountId": account_id }))
            .await?;
        let resp: BalanceResponse = serde_json::from_value(value)?;
        Ok(resp.balance)
    }

    async fn get_last_transaction(&self, account_id: &str) -> ActualResult<LastTransaction> {
        let value = self
            .invoker
            .invoke("get-last-transaction", json!({ "accountId": account_id }))
            .await?;
        let resp: LastTransactionResponse = serde_json::from_value(value)?;
        let date = chrono::NaiveDate::parse_from_str(&resp.date, "%Y-%m-%d")
            .map_err(|e| crate::error::Error::BridgeProtocol(format!("invalid date from bridge: {e}")))?;
        Ok(LastTransaction { date, amount: resp.amount })
    }

    async fn ensure_payee(&self, name: &str) -> ActualResult<String> {
        let value = self
            .invoker
            .invoke("ensure-payee", json!({ "name": name }))
            .await?;
        let resp: EnsurePayeeResponse = serde_json::from_value(value)?;
        Ok(resp.id)
    }
}

#[async_trait]
impl TransactionRequests for Client {
    async fn add_transaction(
        &self,
        tx: SaveTransaction,
    ) -> ActualResult<AddTransactionResponse> {
        let value = self
            .invoker
            .invoke("add-transaction", serde_json::to_value(&tx)?)
            .await?;
        let resp: AddTransactionResponse = serde_json::from_value(value)?;
        Ok(resp)
    }

    async fn get_balance_at(&self, account_id: &str, date: chrono::NaiveDate) -> ActualResult<i64> {
        let value = self
            .invoker
            .invoke(
                "get-balance-at",
                json!({ "accountId": account_id, "date": date.to_string() }),
            )
            .await?;
        let resp: BalanceResponse = serde_json::from_value(value)?;
        Ok(resp.balance)
    }

    async fn import_transaction(&self, tx: ImportTransaction) -> ActualResult<String> {
        let req = ImportTransactionRequest {
            account_id: tx.account_id,
            date: tx.date.to_string(),
            payee_id: tx.payee_id,
            amount: tx.amount,
            notes: tx.notes,
            cleared: tx.cleared,
        };
        let value = self
            .invoker
            .invoke("import-transaction", serde_json::to_value(&req)?)
            .await?;
        let resp: AddTransactionResponse = serde_json::from_value(value)?;
        Ok(resp.id)
    }
}

#[cfg(feature = "testutils")]
mockall::mock! {
    pub AccountRequestsImpl {}

    impl Clone for AccountRequestsImpl {
        fn clone(&self) -> Self;
    }

    #[async_trait]
    impl AccountRequests for AccountRequestsImpl {
        async fn list_accounts(&self) -> ActualResult<Vec<Account>>;
        async fn get_account_balance(&self, account_id: &str) -> ActualResult<i64>;
        async fn get_last_transaction(&self, account_id: &str) -> ActualResult<LastTransaction>;
        async fn ensure_payee(&self, name: &str) -> ActualResult<String>;
    }
}

#[cfg(feature = "testutils")]
mockall::mock! {
    pub TransactionRequestsImpl {}

    impl Clone for TransactionRequestsImpl {
        fn clone(&self) -> Self;
    }

    #[async_trait]
    impl TransactionRequests for TransactionRequestsImpl {
        async fn add_transaction(&self, tx: SaveTransaction) -> ActualResult<AddTransactionResponse>;
        async fn get_balance_at(&self, account_id: &str, date: chrono::NaiveDate) -> ActualResult<i64>;
        async fn import_transaction(&self, tx: ImportTransaction) -> ActualResult<String>;
    }
}
