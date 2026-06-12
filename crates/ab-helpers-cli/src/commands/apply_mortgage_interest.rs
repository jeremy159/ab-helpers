use std::process::ExitCode;
use std::sync::Arc;

use ab_helpers_domain::InterestOutcome;
use ab_helpers_server::config::Settings;
use ab_helpers_server::services::actual::{InterestDryRun, InterestService};
use clap::Args;

#[derive(Args, Debug)]
pub struct ApplyMortgageInterestArgs {
    /// Print what would be done without writing anything to Actual.
    #[arg(long)]
    pub dry_run: bool,
}

pub async fn run(settings: Settings, args: ApplyMortgageInterestArgs) -> anyhow::Result<ExitCode> {
    tracing::info!(dry_run = args.dry_run, "apply-mortgage-interest started");

    let client = Arc::new(settings.actual.client());
    let config = settings.actual.mortgage.interest_config();
    let service = InterestService::new(client, config);

    if args.dry_run {
        tracing::debug!("previewing mortgage interest (dry-run)");
        return match service.preview().await {
            Ok(InterestDryRun::AccountClosed) => {
                tracing::info!("mortgage account is closed — dry-run would skip");
                Ok(ExitCode::SUCCESS)
            }
            Ok(InterestDryRun::NoInterest { balance, cutoff }) => {
                tracing::info!(balance, %cutoff, "no mortgage interest would be applied (dry-run)");
                Ok(ExitCode::SUCCESS)
            }
            Ok(InterestDryRun::WouldApply {
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
                    "mortgage interest dry-run: would apply\n  Last transaction: {last_tx_date}\n  Cutoff date:      {cutoff}\n  Balance:          {balance} cents\n  Interest (dry):   {interest} cents\n  New balance:      {new_balance} cents\n  Notes:            {notes}"
                );
                Ok(ExitCode::SUCCESS)
            }
            Err(err) => {
                tracing::error!(?err, "mortgage interest preview failed");
                Ok(ExitCode::from(3))
            }
        };
    }

    tracing::debug!("applying mortgage interest");
    match service.apply().await {
        Ok(InterestOutcome::AccountClosed) => {
            tracing::info!("mortgage account is closed — skipping");
            Ok(ExitCode::SUCCESS)
        }
        Ok(InterestOutcome::NoInterest { balance }) => {
            tracing::info!(balance, "no mortgage interest to apply");
            Ok(ExitCode::SUCCESS)
        }
        Ok(InterestOutcome::Applied {
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
                "mortgage interest applied\n  Balance:      {balance} cents\n  Interest:     {interest} cents\n  New balance:  {new_balance} cents\n  Transaction:  {transaction_id}"
            );
            Ok(ExitCode::SUCCESS)
        }
        Err(err) => {
            tracing::error!(?err, "mortgage interest application failed");
            Ok(ExitCode::from(3))
        }
    }
}
