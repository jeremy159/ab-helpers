use std::process::ExitCode;
use std::sync::Arc;

use ab_helpers_domain::InterestOutcome;
use ab_helpers_server::config::Settings;
use ab_helpers_server::services::actual::InterestService;

pub async fn run(settings: Settings) -> anyhow::Result<ExitCode> {
    let client = Arc::new(settings.actual.client());
    let config = settings.actual.kia.interest_config();
    let service = InterestService::new(client, config);

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
