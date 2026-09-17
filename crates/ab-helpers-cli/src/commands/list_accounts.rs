use super::error::CliError;
use ab_helpers_server::config::Settings;
use ab_helpers_server::services::actual::format_account_groups;
use actual::ActualReadRequests;
use clap::Args;

#[derive(Args, Debug)]
pub struct ListAccountsArgs {
    /// Include closed accounts.
    #[arg(long)]
    pub all: bool,
}

pub async fn run(settings: Settings, args: ListAccountsArgs) -> Result<(), CliError> {
    tracing::info!(include_closed = args.all, "list-accounts started");

    settings
        .actual
        .with_session(move |client| async move {
            tracing::debug!("fetching accounts from Actual");
            let mut accounts = client.list_accounts().await.map_err(anyhow::Error::from)?;

            if !args.all {
                accounts.retain(|a| !a.closed);
            }

            accounts.sort_by(|a, b| a.name.cmp(&b.name));

            tracing::info!(count = accounts.len(), "accounts listed");

            if accounts.is_empty() {
                tracing::warn!("no accounts found");
                println!("No accounts found.");
                return Ok(());
            }

            println!(
                "{}",
                format_account_groups(&accounts, |a| {
                    let closed = if a.closed { " [closed]" } else { "" };
                    format!("{}{closed}", a.name)
                })
            );

            Ok(())
        })
        .await
}
