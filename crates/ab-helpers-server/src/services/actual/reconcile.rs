use std::sync::Arc;

use ab_helpers_domain::{Money, ReconcileOutcome};
use async_trait::async_trait;
use chrono::NaiveDate;

use crate::error::{ABHelpersResult, AppError};

use super::ActualClient;

#[async_trait]
pub trait Reconcile: Send + Sync {
    async fn reconcile_account_to(
        &self,
        account_name: &str,
        target: Money,
        opts: ReconcileOptions,
    ) -> ABHelpersResult<ReconcileOutcome>;
}

#[derive(Debug, Clone, Default)]
pub struct ReconcileOptions {
    /// Date for the adjustment transaction. `None` → today (resolved by the bridge).
    pub date: Option<NaiveDate>,
    /// Override the payee name on the adjustment transaction.
    pub payee_name: Option<String>,
    /// Free-form notes attached to the adjustment transaction.
    pub notes: Option<String>,
}

pub struct ReconcileService<C> {
    client: Arc<C>,
}

impl<C> ReconcileService<C> {
    pub fn new(client: Arc<C>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl<C> Reconcile for ReconcileService<C>
where
    C: ActualClient + 'static,
{
    async fn reconcile_account_to(
        &self,
        account_name: &str,
        target: Money,
        opts: ReconcileOptions,
    ) -> ABHelpersResult<ReconcileOutcome> {
        let accounts = self.client.list_accounts().await?;

        let matches: Vec<&actual::Account> = accounts
            .iter()
            .filter(|a| !a.closed && a.name == account_name)
            .collect();

        let account = match matches.as_slice() {
            [] => return Err(AppError::ActualAccountNotFound(account_name.to_string())),
            [only] => *only,
            many => {
                let matches = many
                    .iter()
                    .map(|a| format!("{} ({})", a.name, a.id))
                    .collect::<Vec<_>>();
                return Err(AppError::ActualAccountAmbiguous {
                    name: account_name.to_string(),
                    matches,
                });
            }
        };

        let current_cents = self.client.get_account_balance(&account.id).await?;
        let current = Money::from_cents(current_cents);
        let diff = target - current;

        if diff.is_zero() {
            return Ok(ReconcileOutcome::AlreadyAtTarget { balance: current });
        }

        let payee_name = opts
            .payee_name
            .unwrap_or_else(|| "Balance Adjustment".to_string());
        let tx = actual::SaveTransaction {
            account_id: account.id.clone(),
            amount: diff.cents(),
            payee_name: Some(payee_name),
            notes: opts.notes,
            date: opts.date.map(|d| d.to_string()),
        };

        let resp = self.client.add_transaction(tx).await?;

        Ok(ReconcileOutcome::Adjusted {
            previous: current,
            target,
            adjustment: diff,
            transaction_id: resp.id,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use ab_helpers_domain::{Money, ReconcileOutcome};
    use async_trait::async_trait;

    use super::*;

    /// Hand-written fake that satisfies both client traits, since combining
    /// two mockall mocks into one type isn't trivial.
    struct FakeClient {
        accounts: Vec<actual::Account>,
        balance_cents: i64,
        last_tx: std::sync::Mutex<Option<actual::SaveTransaction>>,
    }

    #[async_trait]
    impl actual::AccountRequests for FakeClient {
        async fn list_accounts(&self) -> actual::ActualResult<Vec<actual::Account>> {
            Ok(self.accounts.clone())
        }
        async fn get_account_balance(&self, _id: &str) -> actual::ActualResult<i64> {
            Ok(self.balance_cents)
        }
        async fn get_last_transaction(
            &self,
            _id: &str,
        ) -> actual::ActualResult<actual::LastTransaction> {
            unimplemented!("not needed for reconcile tests")
        }
        async fn ensure_payee(&self, _name: &str) -> actual::ActualResult<String> {
            unimplemented!("not needed for reconcile tests")
        }
    }

    #[async_trait]
    impl actual::TransactionRequests for FakeClient {
        async fn add_transaction(
            &self,
            tx: actual::SaveTransaction,
        ) -> actual::ActualResult<actual::AddTransactionResponse> {
            *self.last_tx.lock().unwrap() = Some(tx);
            Ok(actual::AddTransactionResponse {
                id: "tx-123".into(),
            })
        }
        async fn get_balance_at(
            &self,
            _id: &str,
            _date: chrono::NaiveDate,
        ) -> actual::ActualResult<i64> {
            unimplemented!("not needed for reconcile tests")
        }
        async fn import_transaction(
            &self,
            _tx: actual::ImportTransaction,
        ) -> actual::ActualResult<String> {
            unimplemented!("not needed for reconcile tests")
        }
    }

    fn account(id: &str, name: &str) -> actual::Account {
        actual::Account {
            id: id.into(),
            name: name.into(),
            offbudget: false,
            closed: false,
        }
    }

    #[tokio::test]
    async fn reports_already_at_target_when_diff_is_zero() {
        let client = Arc::new(FakeClient {
            accounts: vec![account("a-1", "Checking")],
            balance_cents: 5000,
            last_tx: Default::default(),
        });
        let svc = ReconcileService::new(client.clone());

        let outcome = svc
            .reconcile_account_to("Checking", Money::from_cents(5000), Default::default())
            .await
            .unwrap();

        assert_eq!(
            outcome,
            ReconcileOutcome::AlreadyAtTarget {
                balance: Money::from_cents(5000)
            }
        );
        assert!(client.last_tx.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn posts_diff_when_balance_below_target() {
        let client = Arc::new(FakeClient {
            accounts: vec![account("a-1", "Checking")],
            balance_cents: 110_000,
            last_tx: Default::default(),
        });
        let svc = ReconcileService::new(client.clone());

        let outcome = svc
            .reconcile_account_to("Checking", Money::from_cents(123_456), Default::default())
            .await
            .unwrap();

        match outcome {
            ReconcileOutcome::Adjusted {
                previous,
                target,
                adjustment,
                transaction_id,
            } => {
                assert_eq!(previous, Money::from_cents(110_000));
                assert_eq!(target, Money::from_cents(123_456));
                assert_eq!(adjustment, Money::from_cents(13_456));
                assert_eq!(transaction_id, "tx-123");
            }
            other => panic!("unexpected outcome: {other:?}"),
        }

        let tx = client.last_tx.lock().unwrap().clone().expect("tx posted");
        assert_eq!(tx.account_id, "a-1");
        assert_eq!(tx.amount, 13_456);
        assert_eq!(tx.payee_name.as_deref(), Some("Balance Adjustment"));
    }

    #[tokio::test]
    async fn posts_negative_diff_when_balance_above_target() {
        let client = Arc::new(FakeClient {
            accounts: vec![account("a-1", "Checking")],
            balance_cents: 200_000,
            last_tx: Default::default(),
        });
        let svc = ReconcileService::new(client.clone());

        svc.reconcile_account_to("Checking", Money::from_cents(150_000), Default::default())
            .await
            .unwrap();

        let tx = client.last_tx.lock().unwrap().clone().unwrap();
        assert_eq!(tx.amount, -50_000);
    }

    #[tokio::test]
    async fn errors_when_account_not_found() {
        let client = Arc::new(FakeClient {
            accounts: vec![account("a-1", "Savings")],
            balance_cents: 0,
            last_tx: Default::default(),
        });
        let svc = ReconcileService::new(client);

        let err = svc
            .reconcile_account_to("Checking", Money::from_cents(0), Default::default())
            .await
            .unwrap_err();

        assert!(matches!(err, AppError::ActualAccountNotFound(_)));
    }

    #[tokio::test]
    async fn errors_when_account_name_is_ambiguous() {
        let client = Arc::new(FakeClient {
            accounts: vec![account("a-1", "Checking"), account("a-2", "Checking")],
            balance_cents: 0,
            last_tx: Default::default(),
        });
        let svc = ReconcileService::new(client);

        let err = svc
            .reconcile_account_to("Checking", Money::from_cents(0), Default::default())
            .await
            .unwrap_err();

        assert!(matches!(err, AppError::ActualAccountAmbiguous { .. }));
    }
}
