use std::sync::Arc;

use super::error::CliError;
use ab_helpers_domain::{DryRunOutcome, InterestSkip, LiveOutcome};
use ab_helpers_server::{
    config::Settings,
    error::AppError,
    execution::{DryRun, Live, PlanExecute},
    services::actual::InterestService,
};
use clap::Args;

#[derive(Args, Debug)]
pub struct InterestArgs {
    /// Print what would be done without writing anything to Actual.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Clone, Copy, Debug)]
pub enum InterestKind {
    Kia,
    Mortgage,
}

impl InterestKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Kia => "kia",
            Self::Mortgage => "mortgage",
        }
    }

    pub fn config(self, settings: &Settings) -> ab_helpers_server::config::InterestConfig {
        match self {
            Self::Kia => settings.actual.kia.interest_config(),
            Self::Mortgage => settings.actual.mortgage.interest_config(),
        }
    }
}

pub async fn run(
    settings: Settings,
    args: InterestArgs,
    kind: InterestKind,
) -> Result<(), CliError> {
    let label = kind.label();
    tracing::info!(
        dry_run = args.dry_run,
        kind = label,
        "apply-interest started"
    );

    let client = Arc::new(settings.actual.client());
    let config = kind.config(&settings);
    let service = InterestService::new(client, config);

    if args.dry_run {
        tracing::debug!(kind = label, "previewing interest (dry-run)");
        return match service.run::<DryRun>().await {
            Ok(DryRunOutcome::Skip(InterestSkip::AccountClosed)) => {
                tracing::info!(
                    kind = label,
                    "{label} account is closed - skipping - (DRY-RUN)"
                );
                Ok(())
            }
            Ok(DryRunOutcome::Skip(InterestSkip::NoInterest { balance, cutoff })) => {
                tracing::info!(balance = %balance, %cutoff, kind = label, "no {label} interest to apply - (DRY-RUN)");
                Ok(())
            }
            Ok(DryRunOutcome::WouldApply {
                last_tx_date,
                cutoff,
                balance,
                interest,
                new_balance,
                notes,
            }) => {
                tracing::info!(
                    balance = %balance,
                    interest = %interest,
                    new_balance = %new_balance,
                    %last_tx_date,
                    %cutoff,
                    %notes,
                    kind = label,
                    "{label} interest would apply (DRY-RUN)"
                );
                println!(
                    "{label} interest (DRY-RUN)\n  Last transaction: {last_tx_date}\n  Cutoff date:      {cutoff}\n  Balance:          {balance}\n  Interest:         {interest}\n  New balance:      {new_balance}\n  Notes:            {notes}"
                );
                Ok(())
            }
            Err(AppError::ActualAccountNotFound(name)) => {
                tracing::warn!(account = %name, kind = label, "account not found");
                Err(CliError::NotFound)
            }
            Err(err) => {
                tracing::error!(?err, kind = label, "interest preview failed");
                Err(CliError::Failure(err.into()))
            }
        };
    }

    tracing::debug!(kind = label, "applying interest");
    match service.run::<Live>().await {
        Ok(LiveOutcome::Skip(InterestSkip::AccountClosed)) => {
            tracing::info!(kind = label, "{label} account is closed - skipping");
            Ok(())
        }
        Ok(LiveOutcome::Skip(InterestSkip::NoInterest { balance, .. })) => {
            tracing::info!(balance = %balance, kind = label, "no {label} interest to apply");
            Ok(())
        }
        Ok(LiveOutcome::Applied {
            balance,
            interest,
            new_balance,
            transaction_id,
        }) => {
            tracing::info!(
                balance = %balance,
                interest = %interest,
                new_balance = %new_balance,
                %transaction_id,
                kind = label,
                "{label} interest applied"
            );
            println!(
                "{label} interest applied\n  Balance:      {balance}\n  Interest:     {interest}\n  New balance:  {new_balance}\n  Transaction:  {transaction_id}"
            );
            Ok(())
        }
        Err(AppError::ActualAccountNotFound(name)) => {
            tracing::warn!(account = %name, kind = label, "account not found");
            Err(CliError::NotFound)
        }
        Err(err) => {
            tracing::error!(?err, kind = label, "interest application failed");
            Err(CliError::Failure(err.into()))
        }
    }
}
