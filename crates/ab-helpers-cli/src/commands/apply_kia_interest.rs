use std::process::ExitCode;
use std::sync::Arc;

use ab_helpers_domain::{DryRunOutcome, InterestSkip, LiveOutcome};
use ab_helpers_server::{
    config::Settings,
    execution::{DryRun, Live, PlanExecute},
    services::actual::InterestService,
};
use clap::Args;

#[derive(Args, Debug)]
pub struct ApplyKiaInterestArgs {
    /// Print what would be done without writing anything to Actual.
    #[arg(long)]
    pub dry_run: bool,
}

pub async fn run(settings: Settings, args: ApplyKiaInterestArgs) -> anyhow::Result<ExitCode> {
    tracing::info!(dry_run = args.dry_run, "apply-kia-interest started");

    let client = Arc::new(settings.actual.client());
    let config = settings.actual.kia.interest_config();
    let service = InterestService::new(client, config);

    if args.dry_run {
        tracing::debug!("previewing kia interest (dry-run)");
        return match service.run::<DryRun>().await {
            Ok(DryRunOutcome::Skip(InterestSkip::AccountClosed)) => {
                tracing::info!("kia account is closed - skipping - (DRY-RUN)");
                Ok(ExitCode::SUCCESS)
            }
            Ok(DryRunOutcome::Skip(InterestSkip::NoInterest { balance, cutoff })) => {
                tracing::info!(balance, %cutoff, "no kia interest to apply - (DRY-RUN)");
                Ok(ExitCode::SUCCESS)
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
                    balance,
                    interest,
                    new_balance,
                    %last_tx_date,
                    %cutoff,
                    %notes,
                    "kia interest applied - (DRY-RUN)\n  Last transaction: {last_tx_date}\n  Cutoff date:      {cutoff}\n  Balance:          {balance} cents\n  Interest:         {interest} cents\n  New balance:      {new_balance} cents\n  Notes:            {notes}"
                );
                Ok(ExitCode::SUCCESS)
            }
            Err(err) => {
                tracing::error!(?err, "kia interest preview failed");
                Ok(ExitCode::from(3))
            }
        };
    }

    tracing::debug!("applying kia interest");
    match service.run::<Live>().await {
        Ok(LiveOutcome::Skip(InterestSkip::AccountClosed)) => {
            tracing::info!("kia account is closed - skipping");
            Ok(ExitCode::SUCCESS)
        }
        Ok(LiveOutcome::Skip(InterestSkip::NoInterest { balance, .. })) => {
            tracing::info!(balance, "no kia interest to apply");
            Ok(ExitCode::SUCCESS)
        }
        Ok(LiveOutcome::Applied {
            balance,
            interest,
            new_balance,
            transaction_id,
        }) => {
            tracing::info!(
                balance,
                interest,
                new_balance,
                %transaction_id,
                "kia interest applied\n  Balance:      {balance} cents\n  Interest:     {interest} cents\n  New balance:  {new_balance} cents\n  Transaction:  {transaction_id}"
            );
            Ok(ExitCode::SUCCESS)
        }
        Err(err) => {
            tracing::error!(?err, "kia interest application failed");
            Ok(ExitCode::from(3))
        }
    }
}
