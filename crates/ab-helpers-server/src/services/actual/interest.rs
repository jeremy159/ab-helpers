use crate::config::InterestConfig;
use crate::error::{ABHelpersResult, AppError};
use crate::execution::{Live, PlanExecute, PlanOutcome, RunMode};
use ab_helpers_domain::{InterestPlan, InterestSkip, LiveOutcome, Money};
use std::sync::Arc;

use super::{ActualClient, ActualReadClient, ActualWriteClient};

/// Live apply: writes the interest transaction to Actual via the write client.
#[async_trait::async_trait]
impl<W: ActualWriteClient + 'static> RunMode<InterestSkip, InterestPlan, W> for Live {
    type Outcome = LiveOutcome;

    fn on_skip(reason: InterestSkip) -> LiveOutcome {
        LiveOutcome::Skip(reason)
    }

    async fn apply(writer: &W, plan: InterestPlan) -> ABHelpersResult<LiveOutcome> {
        let payee_id = writer.ensure_payee(&plan.payee_name).await?;

        let import_tx = actual::ImportTransaction {
            account_id: plan.account_id.clone(),
            date: plan.last_tx_date,
            payee_id,
            amount: plan.interest.cents(),
            notes: Some(plan.notes.clone()),
            cleared: Some(true),
        };

        let tx_id = writer.import_transaction(import_tx).await?;

        Ok(LiveOutcome::Applied {
            balance: plan.balance,
            interest: plan.interest,
            new_balance: plan.new_balance,
            transaction_id: tx_id,
        })
    }
}

/// Read-only context for the plan phase. Contains only what `plan()` needs —
/// the write client is absent, so writes are structurally unreachable from `plan()`.
pub struct InterestPlanCtx<R> {
    pub reader: Arc<R>,
    pub config: InterestConfig,
}

/// Service that computes and optionally applies interest for one Actual account.
///
/// `R` is the read client (used in `plan()`); `W` is the write client (used in
/// `Live::apply`). In production both are the same concrete `Client` — use
/// `InterestService::new` which takes a single `Arc<C: ActualClient>`.
pub struct InterestService<R, W = R> {
    plan_ctx: InterestPlanCtx<R>,
    writer: Arc<W>,
}

impl<C: ActualClient + 'static> InterestService<C, C> {
    pub fn new(client: Arc<C>, config: InterestConfig) -> Self {
        Self {
            plan_ctx: InterestPlanCtx {
                reader: Arc::clone(&client),
                config,
            },
            writer: client,
        }
    }
}

#[async_trait::async_trait]
impl<R: ActualReadClient + 'static, W: ActualWriteClient + 'static> PlanExecute
    for InterestService<R, W>
{
    type Skip = InterestSkip;
    type Plan = InterestPlan;
    type PlanCtx = InterestPlanCtx<R>;
    type Writer = W;

    fn plan_ctx(&self) -> &InterestPlanCtx<R> {
        &self.plan_ctx
    }

    fn writer(&self) -> &W {
        self.writer.as_ref()
    }

    async fn plan(ctx: &InterestPlanCtx<R>) -> ABHelpersResult<PlanOutcome<InterestSkip, InterestPlan>> {
        use ab_helpers_domain::apply_bank_payment;

        let accounts = ctx.reader.list_accounts().await?;
        let account = accounts
            .iter()
            .find(|a| a.id == ctx.config.account_id)
            .ok_or_else(|| AppError::ActualAccountNotFound(ctx.config.account_id.clone()))?;

        if account.closed {
            return Ok(PlanOutcome::Skip(InterestSkip::AccountClosed));
        }

        let last_tx = ctx.reader.get_last_transaction(&account.id).await?;

        let cutoff = ctx.config.period.cutoff_for(last_tx.date);

        let balance_cents = ctx.reader.get_balance_at(&account.id, cutoff).await?;
        let balance = Money::from_cents(balance_cents);

        let payment = Money::from_cents(last_tx.amount);
        let result = apply_bank_payment(balance, payment, ctx.config.rate, ctx.config.round);

        if result.interest.is_zero() {
            return Ok(PlanOutcome::Skip(InterestSkip::NoInterest {
                balance,
                cutoff,
            }));
        }

        let notes = format!(
            "Intérêt pour 1 {} à {:.2}%",
            ctx.config.period.notes_label(),
            ctx.config.rate * 100.0
        );

        Ok(PlanOutcome::Ready(InterestPlan {
            account_id: account.id.clone(),
            last_tx_date: last_tx.date,
            cutoff,
            balance,
            interest: result.interest,
            new_balance: result.new_balance,
            notes,
            payee_name: ctx.config.payee_name.clone(),
        }))
    }
}
