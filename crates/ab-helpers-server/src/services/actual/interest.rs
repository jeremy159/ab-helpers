use crate::config::InterestConfig;
use crate::error::{ABHelpersResult, AppError};
use crate::execution::{DryRun, Live, PlanExecute, PlanOutcome, RunMode};
use ab_helpers_domain::{DryRunOutcome, InterestPlan, InterestSkip, LiveOutcome, Money};
use std::sync::Arc;

use super::ActualClient;

/// Dry-run apply: ignores the client and projects the plan into a preview outcome.
#[async_trait::async_trait]
impl<W: Send + Sync> RunMode<InterestSkip, InterestPlan, W> for DryRun {
    type Outcome = DryRunOutcome;

    fn on_skip(reason: InterestSkip) -> DryRunOutcome {
        DryRunOutcome::Skip(reason)
    }

    async fn apply(_writer: &W, plan: InterestPlan) -> ABHelpersResult<DryRunOutcome> {
        Ok(DryRunOutcome::WouldApply {
            last_tx_date: plan.last_tx_date,
            cutoff: plan.cutoff,
            balance: plan.balance,
            interest: plan.interest,
            new_balance: plan.new_balance,
            notes: plan.notes,
        })
    }
}

/// Live apply: writes the interest transaction to Actual via the client.
#[async_trait::async_trait]
impl<W: ActualClient + 'static> RunMode<InterestSkip, InterestPlan, W> for Live {
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

pub struct InterestService<C> {
    client: Arc<C>,
    config: InterestConfig,
}

impl<C: ActualClient + 'static> InterestService<C> {
    pub fn new(client: Arc<C>, config: InterestConfig) -> Self {
        Self { client, config }
    }
}

#[async_trait::async_trait]
impl<C: ActualClient + 'static> PlanExecute for InterestService<C> {
    type Skip = InterestSkip;
    type Plan = InterestPlan;
    type Writer = C;

    fn writer(&self) -> &C {
        self.client.as_ref()
    }

    async fn plan(&self) -> ABHelpersResult<PlanOutcome<InterestSkip, InterestPlan>> {
        use ab_helpers_domain::apply_bank_payment;

        let accounts = self.client.list_accounts().await?;
        let account = accounts
            .iter()
            .find(|a| a.id == self.config.account_id)
            .ok_or_else(|| AppError::ActualAccountNotFound(self.config.account_id.clone()))?;

        if account.closed {
            return Ok(PlanOutcome::Skip(InterestSkip::AccountClosed));
        }

        let last_tx = self.client.get_last_transaction(&account.id).await?;

        let cutoff = self.config.period.cutoff_for(last_tx.date);

        let balance_cents = self.client.get_balance_at(&account.id, cutoff).await?;
        let balance = Money::from_cents(balance_cents);

        let payment = Money::from_cents(last_tx.amount);
        let result = apply_bank_payment(balance, payment, self.config.rate, self.config.round);

        if result.interest.is_zero() {
            return Ok(PlanOutcome::Skip(InterestSkip::NoInterest {
                balance,
                cutoff,
            }));
        }

        let notes = format!(
            "Intérêt pour 1 {} à {:.2}%",
            self.config.period.notes_label(),
            self.config.rate * 100.0
        );

        Ok(PlanOutcome::Ready(InterestPlan {
            account_id: account.id.clone(),
            last_tx_date: last_tx.date,
            cutoff,
            balance,
            interest: result.interest,
            new_balance: result.new_balance,
            notes,
            payee_name: self.config.payee_name.clone(),
        }))
    }
}
