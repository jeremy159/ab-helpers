use std::process::ExitCode;
use std::sync::Arc;

use ab_helpers_domain::InterestOutcome;
use ab_helpers_server::config::Settings;
use ab_helpers_server::services::actual::{InterestDryRun, InterestService};
use clap::Args;

#[derive(Args, Debug)]
pub struct ApplyKiaInterestArgs {
    /// Print what would be done without writing anything to Actual.
    #[arg(long)]
    pub dry_run: bool,
}

pub async fn run(settings: Settings, args: ApplyKiaInterestArgs) -> anyhow::Result<ExitCode> {
    let client = Arc::new(settings.actual.client());
    let config = settings.actual.kia.interest_config();
    let service = InterestService::new(client, config);

    if args.dry_run {
        return match service.preview().await {
            Ok(InterestDryRun::AccountClosed) => {
                println!("Account is closed — would skip.");
                Ok(ExitCode::SUCCESS)
            }
            Ok(InterestDryRun::NoInterest { balance, cutoff }) => {
                println!("Balance at cutoff {cutoff}: {} cents — no interest would be applied.", balance);
                Ok(ExitCode::SUCCESS)
            }
            Ok(InterestDryRun::WouldApply { last_tx_date, cutoff, balance, interest, new_balance, notes }) => {
                println!("Last transaction: {last_tx_date}");
                println!("Cutoff date:      {cutoff}");
                println!("Balance:          {} cents", balance);
                println!("Interest (dry):   {} cents", interest);
                println!("New balance:      {} cents", new_balance);
                println!("Notes:            {notes}");
                Ok(ExitCode::SUCCESS)
            }
            Err(err) => {
                eprintln!("error: {err:?}");
                Ok(ExitCode::from(3))
            }
        };
    }

    match service.apply().await {
        Ok(InterestOutcome::AccountClosed) => {
            println!("Account is closed — skipping.");
            Ok(ExitCode::SUCCESS)
        }
        Ok(InterestOutcome::NoInterest { balance }) => {
            println!("No interest to apply. Balance: {} cents", balance);
            Ok(ExitCode::SUCCESS)
        }
        Ok(InterestOutcome::Applied { balance, interest, new_balance, transaction_id }) => {
            println!("Balance:      {} cents", balance);
            println!("Interest:     {} cents", interest);
            println!("New balance:  {} cents", new_balance);
            println!("Transaction:  {}", transaction_id);
            Ok(ExitCode::SUCCESS)
        }
        Err(err) => {
            eprintln!("error: {err:?}");
            Ok(ExitCode::from(3))
        }
    }
}
