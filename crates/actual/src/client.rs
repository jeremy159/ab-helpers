use std::sync::Arc;

use async_trait::async_trait;

use crate::bridge::{BridgeConfig, BridgeInvoker, BridgeRequest};
use crate::error::ActualResult;
use crate::types::{
    Account, AccountNoteResponse, AddTransactionResponse, BalanceResponse, EnsurePayeeResponse,
    ImportTransaction, ImportTransactionRequest, LastTransaction, LastTransactionResponse,
    ListAccountsResponse, SaveTransaction,
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

/// Read-only operations: account queries and balance reads.
#[async_trait]
pub trait ActualReadRequests: Send + Sync {
    async fn list_accounts(&self) -> ActualResult<Vec<Account>>;
    async fn get_account_balance(&self, account_id: &str) -> ActualResult<i64>;
    async fn get_last_transaction(&self, account_id: &str) -> ActualResult<LastTransaction>;
    async fn get_balance_at(&self, account_id: &str, date: chrono::NaiveDate) -> ActualResult<i64>;
    async fn get_account_note(&self, account_id: &str) -> ActualResult<Option<String>>;
}

/// Write operations: creating payees and posting transactions.
#[async_trait]
pub trait ActualWriteRequests: Send + Sync {
    async fn ensure_payee(&self, name: &str) -> ActualResult<String>;
    async fn add_transaction(&self, tx: SaveTransaction) -> ActualResult<AddTransactionResponse>;
    async fn import_transaction(&self, tx: ImportTransaction) -> ActualResult<String>;
}

#[async_trait]
impl ActualReadRequests for Client {
    async fn list_accounts(&self) -> ActualResult<Vec<Account>> {
        let value = self.invoker.invoke(BridgeRequest::ListAccounts).await?;
        let resp: ListAccountsResponse = serde_json::from_value(value)?;
        Ok(resp.accounts)
    }

    async fn get_account_balance(&self, account_id: &str) -> ActualResult<i64> {
        let value = self
            .invoker
            .invoke(BridgeRequest::GetBalance {
                account_id: account_id.to_string(),
            })
            .await?;
        let resp: BalanceResponse = serde_json::from_value(value)?;
        Ok(resp.balance)
    }

    async fn get_last_transaction(&self, account_id: &str) -> ActualResult<LastTransaction> {
        let value = self
            .invoker
            .invoke(BridgeRequest::GetLastTransaction {
                account_id: account_id.to_string(),
            })
            .await?;
        let resp: LastTransactionResponse = serde_json::from_value(value)?;
        Ok(LastTransaction {
            date: resp.date,
            amount: resp.amount,
        })
    }

    async fn get_balance_at(&self, account_id: &str, date: chrono::NaiveDate) -> ActualResult<i64> {
        let value = self
            .invoker
            .invoke(BridgeRequest::GetBalanceAt {
                account_id: account_id.to_string(),
                date: date.to_string(),
            })
            .await?;
        let resp: BalanceResponse = serde_json::from_value(value)?;
        Ok(resp.balance)
    }

    async fn get_account_note(&self, account_id: &str) -> ActualResult<Option<String>> {
        let value = self
            .invoker
            .invoke(BridgeRequest::GetAccountNote {
                account_id: account_id.to_string(),
            })
            .await?;
        let resp: AccountNoteResponse = serde_json::from_value(value)?;
        Ok(resp.note)
    }
}

#[async_trait]
impl ActualWriteRequests for Client {
    async fn ensure_payee(&self, name: &str) -> ActualResult<String> {
        let value = self
            .invoker
            .invoke(BridgeRequest::EnsurePayee {
                name: name.to_string(),
            })
            .await?;
        let resp: EnsurePayeeResponse = serde_json::from_value(value)?;
        Ok(resp.id)
    }

    async fn add_transaction(&self, tx: SaveTransaction) -> ActualResult<AddTransactionResponse> {
        let value = self
            .invoker
            .invoke(BridgeRequest::AddTransaction(tx))
            .await?;
        let resp: AddTransactionResponse = serde_json::from_value(value)?;
        Ok(resp)
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
            .invoke(BridgeRequest::ImportTransaction(req))
            .await?;
        let resp: AddTransactionResponse = serde_json::from_value(value)?;
        Ok(resp.id)
    }
}

#[cfg(feature = "testutils")]
mockall::mock! {
    pub ActualReadRequestsImpl {}

    impl Clone for ActualReadRequestsImpl {
        fn clone(&self) -> Self;
    }

    #[async_trait]
    impl ActualReadRequests for ActualReadRequestsImpl {
        async fn list_accounts(&self) -> ActualResult<Vec<Account>>;
        async fn get_account_balance(&self, account_id: &str) -> ActualResult<i64>;
        async fn get_last_transaction(&self, account_id: &str) -> ActualResult<LastTransaction>;
        async fn get_balance_at(&self, account_id: &str, date: chrono::NaiveDate) -> ActualResult<i64>;
        async fn get_account_note(&self, account_id: &str) -> ActualResult<Option<String>>;
    }
}

#[cfg(feature = "testutils")]
mockall::mock! {
    pub ActualWriteRequestsImpl {}

    impl Clone for ActualWriteRequestsImpl {
        fn clone(&self) -> Self;
    }

    #[async_trait]
    impl ActualWriteRequests for ActualWriteRequestsImpl {
        async fn ensure_payee(&self, name: &str) -> ActualResult<String>;
        async fn add_transaction(&self, tx: SaveTransaction) -> ActualResult<AddTransactionResponse>;
        async fn import_transaction(&self, tx: ImportTransaction) -> ActualResult<String>;
    }
}
