use std::sync::Arc;

use ab_helpers_domain::{Money, ReconcileOutcome, ReconcileSkip};
use async_trait::async_trait;
use chrono::NaiveDate;

use crate::error::{ABHelpersResult, AppError};
use crate::execution::{Live, PlanExecute, PlanOutcome, RunMode};

use super::{ActualClient, ActualReadClient, ActualWriteClient};

#[derive(Debug, Clone, Default)]
pub struct ReconcileOptions {
    /// Date for the adjustment transaction. `None` → today (resolved by the bridge).
    pub date: Option<NaiveDate>,
    /// Override the payee name on the adjustment transaction.
    pub payee_name: Option<String>,
    /// Free-form notes attached to the adjustment transaction.
    pub notes: Option<String>,
}

/// Intermediate state produced by the plan phase.
///
/// All fields are fully resolved — `payee_name` has the default applied,
/// so this is a faithful description of what `Live::apply` will write.
#[derive(Debug)]
pub struct ReconcilePlan {
    pub account_id: String,
    pub account_name: String,
    pub current: Money,
    pub target: Money,
    pub diff: Money,
    pub payee_name: String,
    pub date: Option<chrono::NaiveDate>,
    pub notes: Option<String>,
}

/// Live apply: posts the adjustment transaction.
#[async_trait]
impl<W: ActualWriteClient + 'static> RunMode<ReconcileSkip, ReconcilePlan, W> for Live {
    type Outcome = ReconcileOutcome;

    fn on_skip(reason: ReconcileSkip) -> ReconcileOutcome {
        match reason {
            ReconcileSkip::AlreadyAtTarget { balance } => {
                ReconcileOutcome::AlreadyAtTarget { balance }
            }
        }
    }

    async fn apply(writer: &W, plan: ReconcilePlan) -> ABHelpersResult<ReconcileOutcome> {
        let tx = actual::SaveTransaction {
            account_id: plan.account_id,
            amount: plan.diff.cents(),
            payee_name: Some(plan.payee_name),
            notes: plan.notes,
            date: plan.date.map(|d| d.to_string()),
        };

        let resp = writer.add_transaction(tx).await?;

        Ok(ReconcileOutcome::Adjusted {
            previous: plan.current,
            target: plan.target,
            adjustment: plan.diff,
            transaction_id: resp.id,
        })
    }
}

/// Read-only context for the plan phase. Contains only what `plan()` needs —
/// the write client is absent, so writes are structurally unreachable from `plan()`.
pub struct ReconcilePlanCtx<R> {
    pub reader: Arc<R>,
    pub account_name: String,
    pub target: Money,
    pub opts: ReconcileOptions,
}

/// Service that reconciles one Actual account to a target balance.
///
/// `R` is the read client (used in `plan()`); `W` is the write client (used in
/// `Live::apply`). In production both are the same concrete `Client` — use
/// `ReconcileService::new` which takes a single `Arc<C: ActualClient>`.
pub struct ReconcileService<R, W = R> {
    plan_ctx: ReconcilePlanCtx<R>,
    writer: Arc<W>,
}

impl<C: ActualClient + 'static> ReconcileService<C, C> {
    pub fn new(
        client: Arc<C>,
        account_name: String,
        target: Money,
        opts: ReconcileOptions,
    ) -> Self {
        Self {
            plan_ctx: ReconcilePlanCtx {
                reader: Arc::clone(&client),
                account_name,
                target,
                opts,
            },
            writer: client,
        }
    }
}

#[async_trait]
impl<R: ActualReadClient + 'static, W: ActualWriteClient + 'static> PlanExecute
    for ReconcileService<R, W>
{
    type Skip = ReconcileSkip;
    type Plan = ReconcilePlan;
    type PlanCtx = ReconcilePlanCtx<R>;
    type Writer = W;

    fn plan_ctx(&self) -> &ReconcilePlanCtx<R> {
        &self.plan_ctx
    }

    fn writer(&self) -> &W {
        self.writer.as_ref()
    }

    async fn plan(ctx: &ReconcilePlanCtx<R>) -> ABHelpersResult<PlanOutcome<ReconcileSkip, ReconcilePlan>> {
        let accounts = ctx.reader.list_accounts().await?;

        let matches: Vec<&actual::Account> = accounts
            .iter()
            .filter(|a| !a.closed && a.name == ctx.account_name)
            .collect();

        let account = match matches.as_slice() {
            [] => {
                return Err(AppError::ActualAccountNotFound(ctx.account_name.clone()))
            }
            [only] => *only,
            many => {
                let matches = many
                    .iter()
                    .map(|a| format!("{} ({})", a.name, a.id))
                    .collect::<Vec<_>>();
                return Err(AppError::ActualAccountAmbiguous {
                    name: ctx.account_name.clone(),
                    matches,
                });
            }
        };

        let current_cents = ctx.reader.get_account_balance(&account.id).await?;
        let current = Money::from_cents(current_cents);
        let diff = ctx.target - current;

        if diff.is_zero() {
            return Ok(PlanOutcome::Skip(ReconcileSkip::AlreadyAtTarget {
                balance: current,
            }));
        }

        let payee_name = ctx
            .opts
            .payee_name
            .clone()
            .unwrap_or_else(|| "Balance Adjustment".to_string());

        Ok(PlanOutcome::Ready(ReconcilePlan {
            account_id: account.id.clone(),
            account_name: account.name.clone(),
            current,
            target: ctx.target,
            diff,
            payee_name,
            date: ctx.opts.date,
            notes: ctx.opts.notes.clone(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use ab_helpers_domain::{Money, ReconcileOutcome};
    use async_trait::async_trait;

    use super::*;
    use crate::execution::{DryRun, Preview};

    struct FakeClient {
        accounts: Vec<actual::Account>,
        balance_cents: i64,
        last_tx: std::sync::Mutex<Option<actual::SaveTransaction>>,
    }

    #[async_trait]
    impl actual::ActualReadRequests for FakeClient {
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
        async fn get_balance_at(
            &self,
            _id: &str,
            _date: chrono::NaiveDate,
        ) -> actual::ActualResult<i64> {
            unimplemented!("not needed for reconcile tests")
        }
        async fn get_account_note(&self, _id: &str) -> actual::ActualResult<Option<String>> {
            unimplemented!("not needed for reconcile tests")
        }
        async fn find_transaction(
            &self,
            _account_id: &str,
            _date: chrono::NaiveDate,
            _payee_name: &str,
        ) -> actual::ActualResult<Option<actual::ExistingTransaction>> {
            unimplemented!("not needed for reconcile tests")
        }
    }

    #[async_trait]
    impl actual::ActualWriteRequests for FakeClient {
        async fn ensure_payee(&self, _name: &str) -> actual::ActualResult<String> {
            unimplemented!("not needed for reconcile tests")
        }
        async fn add_transaction(
            &self,
            tx: actual::SaveTransaction,
        ) -> actual::ActualResult<actual::AddTransactionResponse> {
            *self.last_tx.lock().unwrap() = Some(tx);
            Ok(actual::AddTransactionResponse {
                id: "tx-123".into(),
            })
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

    fn make_client(balance_cents: i64) -> Arc<FakeClient> {
        Arc::new(FakeClient {
            accounts: vec![account("a-1", "Checking")],
            balance_cents,
            last_tx: Default::default(),
        })
    }

    fn svc_with_client(
        client: Arc<FakeClient>,
        target_cents: i64,
    ) -> ReconcileService<FakeClient> {
        ReconcileService::new(
            client,
            "Checking".to_string(),
            Money::from_cents(target_cents),
            Default::default(),
        )
    }

    #[tokio::test]
    async fn reports_already_at_target_when_diff_is_zero() {
        let client = make_client(5000);
        let svc = svc_with_client(Arc::clone(&client), 5000);

        let outcome = svc.run::<Live>().await.unwrap();
        assert_eq!(
            outcome,
            ReconcileOutcome::AlreadyAtTarget {
                balance: Money::from_cents(5000)
            }
        );
        assert!(client.last_tx.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn dry_run_reports_already_at_target_and_writes_nothing() {
        let client = make_client(5000);
        let svc = svc_with_client(Arc::clone(&client), 5000);

        let outcome = svc.run::<DryRun>().await.unwrap();
        assert!(matches!(
            outcome,
            Preview::Skip(ReconcileSkip::AlreadyAtTarget { .. })
        ));
        assert!(client.last_tx.lock().unwrap().is_none(), "dry-run must not write");
    }

    #[tokio::test]
    async fn posts_diff_when_balance_below_target() {
        let client = make_client(110_000);
        let svc = svc_with_client(Arc::clone(&client), 123_456);

        let outcome = svc.run::<Live>().await.unwrap();
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
    async fn dry_run_returns_would_apply_with_plan_and_writes_nothing() {
        let client = make_client(110_000);
        let svc = svc_with_client(Arc::clone(&client), 123_456);
        let outcome = svc.run::<DryRun>().await.unwrap();
        match outcome {
            Preview::WouldApply(plan) => {
                assert_eq!(plan.current, Money::from_cents(110_000));
                assert_eq!(plan.target, Money::from_cents(123_456));
                assert_eq!(plan.diff, Money::from_cents(13_456));
                assert_eq!(plan.payee_name, "Balance Adjustment");
            }
            other => panic!("unexpected: {other:?}"),
        }
        assert!(client.last_tx.lock().unwrap().is_none(), "dry-run must not write");
    }

    #[tokio::test]
    async fn plan_resolves_opts_into_transaction() {
        let client = make_client(110_000);
        let opts = ReconcileOptions {
            date: Some(chrono::NaiveDate::from_ymd_opt(2025, 3, 15).unwrap()),
            payee_name: Some("Custom Payee".to_string()),
            notes: Some("my note".to_string()),
        };
        let svc = ReconcileService::new(
            Arc::clone(&client),
            "Checking".to_string(),
            Money::from_cents(123_456),
            opts,
        );
        svc.run::<Live>().await.unwrap();

        let tx = client.last_tx.lock().unwrap().clone().expect("tx posted");
        assert_eq!(tx.payee_name.as_deref(), Some("Custom Payee"));
        assert_eq!(tx.notes.as_deref(), Some("my note"));
        assert_eq!(tx.date.as_deref(), Some("2025-03-15"));
    }

    #[tokio::test]
    async fn posts_negative_diff_when_balance_above_target() {
        let client = Arc::new(FakeClient {
            accounts: vec![account("a-1", "Checking")],
            balance_cents: 200_000,
            last_tx: Default::default(),
        });
        let svc = svc_with_client(Arc::clone(&client), 150_000);
        svc.run::<Live>().await.unwrap();
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
        let svc = ReconcileService::new(
            client,
            "Checking".to_string(),
            Money::from_cents(0),
            Default::default(),
        );
        let err = svc.run::<Live>().await.unwrap_err();
        assert!(matches!(err, AppError::ActualAccountNotFound(_)));
    }

    #[tokio::test]
    async fn errors_when_account_name_is_ambiguous() {
        let client = Arc::new(FakeClient {
            accounts: vec![account("a-1", "Checking"), account("a-2", "Checking")],
            balance_cents: 0,
            last_tx: Default::default(),
        });
        let svc = ReconcileService::new(
            client,
            "Checking".to_string(),
            Money::from_cents(0),
            Default::default(),
        );
        let err = svc.run::<Live>().await.unwrap_err();
        assert!(matches!(err, AppError::ActualAccountAmbiguous { .. }));
    }
}
