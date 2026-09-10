use super::super::*;
use crate::config::InterestConfig;
use crate::execution::{DryRun, Live, PlanExecute, Preview};
use ab_helpers_domain::InterestPeriod;
use ab_helpers_domain::{InterestSkip, LiveOutcome, Money};
use actual::{
    Account, ActualResult, AddTransactionResponse, ImportTransaction, LastTransaction,
    SaveTransaction,
};
use async_trait::async_trait;
use chrono::NaiveDate;
use std::sync::Arc;

struct FakeClient {
    accounts: Vec<Account>,
    note: Option<String>,
    last_tx: LastTransaction,
    balance: i64,
    payee_id: String,
    imported_tx: std::sync::Mutex<Option<ImportTransaction>>,
}

#[async_trait]
impl actual::ActualReadRequests for FakeClient {
    async fn list_accounts(&self) -> ActualResult<Vec<Account>> {
        Ok(self.accounts.clone())
    }
    async fn get_account_balance(&self, _id: &str) -> ActualResult<i64> {
        Ok(self.balance)
    }
    async fn get_last_transaction(&self, _id: &str) -> ActualResult<LastTransaction> {
        Ok(self.last_tx.clone())
    }
    async fn get_balance_at(&self, _id: &str, _date: NaiveDate) -> ActualResult<i64> {
        Ok(self.balance)
    }
    async fn get_account_note(&self, _id: &str) -> ActualResult<Option<String>> {
        Ok(self.note.clone())
    }
}

#[async_trait]
impl actual::ActualWriteRequests for FakeClient {
    async fn ensure_payee(&self, _name: &str) -> ActualResult<String> {
        Ok(self.payee_id.clone())
    }
    async fn add_transaction(&self, _tx: SaveTransaction) -> ActualResult<AddTransactionResponse> {
        Ok(AddTransactionResponse {
            id: "ignored".into(),
        })
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
        note: Some("Kia Carnival 2026\ninterestRate:0.0699".into()),
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
    let notes = tx.notes.as_deref().unwrap_or("");
    assert!(notes.contains("semaine"));
    // The displayed rate comes from the account note's `interestRate:` token
    // (6.99%), not from the internal weekly rate used for the math (0.13%).
    assert!(notes.contains("6.99%"));
}

#[tokio::test]
async fn falls_back_to_config_rate_when_note_has_no_interest_rate() {
    let client = Arc::new(FakeClient {
        accounts: vec![make_account("acc-1", false)],
        note: Some("just a regular note".into()),
        last_tx: LastTransaction {
            date: NaiveDate::from_ymd_opt(2024, 5, 18).unwrap(),
            amount: 10000,
        },
        balance: -50000,
        payee_id: "payee-1".into(),
        imported_tx: Default::default(),
    });
    let svc = InterestService::new(client.clone(), kia_config());
    let outcome = svc.run::<Live>().await.unwrap();
    assert!(matches!(outcome, LiveOutcome::Applied { .. }));

    let tx = client
        .imported_tx
        .lock()
        .unwrap()
        .clone()
        .expect("tx imported");
    // No interestRate: token in the note - falls back to the internal
    // weekly rate (0.13%) instead of skipping the run.
    assert!(tx.notes.as_deref().unwrap_or("").contains("0.13%"));
}

#[tokio::test]
async fn falls_back_to_config_rate_when_account_has_no_note_at_all() {
    let client = Arc::new(FakeClient {
        accounts: vec![make_account("acc-1", false)],
        note: None,
        last_tx: LastTransaction {
            date: NaiveDate::from_ymd_opt(2024, 5, 18).unwrap(),
            amount: 10000,
        },
        balance: -50000,
        payee_id: "payee-1".into(),
        imported_tx: Default::default(),
    });
    let svc = InterestService::new(client.clone(), kia_config());
    let outcome = svc.run::<Live>().await.unwrap();
    assert!(matches!(outcome, LiveOutcome::Applied { .. }));

    let tx = client
        .imported_tx
        .lock()
        .unwrap()
        .clone()
        .expect("tx imported");
    assert!(tx.notes.as_deref().unwrap_or("").contains("0.13%"));
}

#[tokio::test]
async fn returns_no_interest_when_zero() {
    let client = Arc::new(FakeClient {
        accounts: vec![make_account("acc-1", false)],
        note: Some("interestRate:0.0699".into()),
        last_tx: LastTransaction {
            date: NaiveDate::from_ymd_opt(2024, 5, 18).unwrap(),
            amount: 0,
        },
        balance: 0,
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
async fn dry_run_returns_would_apply_and_writes_nothing() {
    let client = make_client(false);
    let svc = InterestService::new(Arc::clone(&client), kia_config());
    let outcome = svc.run::<DryRun>().await.unwrap();
    match outcome {
        Preview::WouldApply(plan) => {
            assert_eq!(plan.interest, Money::from_cents(-66));
            assert!(plan.notes.contains("semaine"));
        }
        other => panic!("unexpected: {other:?}"),
    }
    assert!(
        client.imported_tx.lock().unwrap().is_none(),
        "dry-run must not write"
    );
}
