use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;

use crate::bridge::{BridgeConfig, BridgeInvoker};
use crate::error::ActualResult;
use crate::types::{
    Account, AddTransactionResponse, BalanceResponse, ListAccountsResponse, SaveTransaction,
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
}

#[async_trait]
pub trait TransactionRequests: Send + Sync {
    async fn add_transaction(
        &self,
        tx: SaveTransaction,
    ) -> ActualResult<AddTransactionResponse>;
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
        async fn add_transaction(
            &self,
            tx: SaveTransaction,
        ) -> ActualResult<AddTransactionResponse>;
    }
}
