use super::super::*;
use crate::config::InterestConfig;
use crate::execution::{DryRun, Live, PlanExecute};
use ab_helpers_domain::InterestPeriod;
use ab_helpers_domain::{DryRunOutcome, InterestSkip, LiveOutcome, Money};
use actual::{
    Account, ActualResult, AddTransactionResponse, ImportTransaction, LastTransaction,
    SaveTransaction,
};
use async_trait::async_trait;
use chrono::NaiveDate;
use std::sync::Arc;

struct FakeClient {
    accounts: Vec<Account>,
    last_tx: LastTransaction,
    balance: i64,
    payee_id: String,
    imported_tx: std::sync::Mutex<Option<ImportTransaction>>,
}

#[async_trait]
impl actual::AccountRequests for FakeClient {
    async fn list_accounts(&self) -> ActualResult<Vec<Account>> {
        Ok(self.accounts.clone())
    }
    async fn get_account_balance(&self, _id: &str) -> ActualResult<i64> {
        Ok(self.balance)
    }
    async fn get_last_transaction(&self, _id: &str) -> ActualResult<LastTransaction> {
        Ok(self.last_tx.clone())
    }
    async fn ensure_payee(&self, _name: &str) -> ActualResult<String> {
        Ok(self.payee_id.clone())
    }
}

#[async_trait]
impl actual::TransactionRequests for FakeClient {
    async fn add_transaction(&self, _tx: SaveTransaction) -> ActualResult<AddTransactionResponse> {
        Ok(AddTransactionResponse {
            id: "ignored".into(),
        })
    }
    async fn get_balance_at(&self, _id: &str, _date: NaiveDate) -> ActualResult<i64> {
        Ok(self.balance)
    }
    async fn import_transaction(&self, tx: ImportTransaction) -> ActualResult<String> {
        *self.imported_tx.lock().unwrap() = Some(tx);
        Ok("tx-interest-1".into())
    }
}

fn make_account(id: &str, closed: bool) -> Account {
    Account {
        id: id.into(),
        name: "Test Loan".into(),
        offbudget: false,
        closed,
    }
}

fn make_client(closed: bool) -> Arc<FakeClient> {
    Arc::new(FakeClient {
        accounts: vec![make_account("acc-1", closed)],
        last_tx: LastTransaction {
            date: NaiveDate::from_ymd_opt(2024, 5, 18).unwrap(),
            amount: 10000,
        },
        balance: -50000,
        payee_id: "payee-1".into(),
        imported_tx: Default::default(),
    })
}

fn kia_config() -> InterestConfig {
    InterestConfig {
        account_id: "acc-1".into(),
        rate: 0.00133978648017598,
        payee_name: "Loan Interest".into(),
        round: false,
        period: InterestPeriod::Weekly,
    }
}

#[tokio::test]
async fn returns_account_closed_when_closed() {
    let svc = InterestService::new(make_client(true), kia_config());
    let outcome = svc.run::<Live>().await.unwrap();
    assert!(matches!(
        outcome,
        LiveOutcome::Skip(InterestSkip::AccountClosed)
    ));
}

#[tokio::test]
async fn applies_interest_and_imports_transaction() {
    let client = make_client(false);
    let svc = InterestService::new(client.clone(), kia_config());
    let outcome = svc.run::<Live>().await.unwrap();

    match outcome {
        LiveOutcome::Applied {
            interest,
            transaction_id,
            ..
        } => {
            assert_eq!(interest, Money::from_cents(-66)); // floor(50000 * 0.00133978...) = 66, signed negative
            assert_eq!(transaction_id, "tx-interest-1");
        }
        other => panic!("unexpected: {other:?}"),
    }

    let tx = client
        .imported_tx
        .lock()
        .unwrap()
        .clone()
        .expect("tx imported");
    assert_eq!(tx.account_id, "acc-1");
    assert_eq!(tx.payee_id, "payee-1");
    assert_eq!(tx.cleared, Some(true));
    assert!(tx.notes.as_deref().unwrap_or("").contains("semaine"));
}

#[tokio::test]
async fn returns_no_interest_when_zero() {
    let client = Arc::new(FakeClient {
        accounts: vec![make_account("acc-1", false)],
        last_tx: LastTransaction {
            date: NaiveDate::from_ymd_opt(2024, 5, 18).unwrap(),
            amount: 0,
        },
        balance: 0, // zero balance → zero interest
        payee_id: "p".into(),
        imported_tx: Default::default(),
    });
    let svc = InterestService::new(client, kia_config());
    let outcome = svc.run::<Live>().await.unwrap();
    assert!(matches!(
        outcome,
        LiveOutcome::Skip(InterestSkip::NoInterest { .. })
    ));
}

#[tokio::test]
async fn dry_run_returns_would_apply() {
    let client = make_client(false);
    let svc = InterestService::new(client, kia_config());
    let outcome = svc.run::<DryRun>().await.unwrap();
    match outcome {
        DryRunOutcome::WouldApply {
            interest, notes, ..
        } => {
            assert_eq!(interest, Money::from_cents(-66));
            assert!(notes.contains("semaine"));
        }
        other => panic!("unexpected: {other:?}"),
    }
}
