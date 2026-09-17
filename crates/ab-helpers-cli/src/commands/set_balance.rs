use ab_helpers_domain::{Money, ReconcileOutcome, ReconcileSkip};
use anyhow::Context as _;
use std::io::IsTerminal;

use super::error::{CliError, map_app_error};
use ab_helpers_server::config::Settings;
use ab_helpers_server::error::AppError;
use ab_helpers_server::execution::{DryRun, Live, PlanExecute, Preview};
use ab_helpers_server::services::actual::{
    AccountMatch, ReconcileOptions, ReconcileService, available_account_names, match_account,
};
use actual::{Account, ActualReadRequests};
use clap::Args;

#[derive(Args, Debug)]
pub struct SetBalanceArgs {
    /// Name of the account in Actual (case-insensitive).
    pub account: String,

    /// Target balance, e.g. `1234.56` or `-50`.
    pub amount: Money,

    /// Date in `YYYY-MM-DD`. Defaults to today (resolved by the bridge).
    #[arg(long)]
    pub date: Option<String>,

    /// Override the payee name on the adjustment transaction. Defaults to
    /// `actual.set_balance.payee_name` from config.
    #[arg(long)]
    pub payee_name: Option<String>,

    /// Notes attached to the adjustment transaction.
    #[arg(long)]
    pub notes: Option<String>,

    /// Compute the diff and print what would be done; do not write anything.
    #[arg(long)]
    pub dry_run: bool,
}

pub async fn run(settings: Settings, args: SetBalanceArgs) -> Result<(), CliError> {
    tracing::info!(account = %args.account, amount = %args.amount, dry_run = args.dry_run, "set-balance started");

    let payee_name = args
        .payee_name
        .clone()
        .unwrap_or_else(|| settings.actual.set_balance.payee_name.clone());

    let opts = ReconcileOptions {
        date: args
            .date
            .as_deref()
            .map(|s| {
                chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
                    .context("--date must be in YYYY-MM-DD format")
            })
            .transpose()?,
        payee_name: Some(payee_name),
        notes: args.notes,
    };

    settings
        .actual
        .with_session(move |client| async move {
            let accounts = client
                .list_accounts()
                .await
                .map_err(anyhow::Error::from)?;

            let account_name = resolve_account_name(&accounts, &args.account)?;

            let service = ReconcileService::new(client, account_name.clone(), args.amount, opts);

            if args.dry_run {
                tracing::debug!(account = %account_name, "previewing reconcile (dry-run)");
                match service.run::<DryRun>().await.map_err(map_reconcile_error)? {
                    Preview::Skip(ReconcileSkip::AlreadyAtTarget { balance }) => {
                        tracing::info!(account = %account_name, %balance, "account already at target (dry-run)");
                        println!(
                            "No adjustment needed\n  Account:         {}\n  Current balance: {balance}\n  Target balance:  {}\n  No transaction would be created.",
                            account_name, args.amount
                        );
                    }
                    Preview::WouldApply(plan) => {
                        tracing::info!(account = %plan.account_name, diff = %plan.diff, "dry-run: would adjust balance");
                        println!(
                            "Would adjust balance\n  Account:         {}\n  Current balance: {}\n  Target balance:  {}\n  Adjustment (dry): {}",
                            plan.account_name,
                            plan.current,
                            plan.target,
                            plan.diff.signed_str()
                        );
                    }
                }
                return Ok(());
            }

            tracing::debug!(account = %account_name, "reconciling account balance");
            match service.run::<Live>().await.map_err(map_reconcile_error)? {
                ReconcileOutcome::AlreadyAtTarget { balance } => {
                    print_already_at_target(&account_name, balance);
                }
                ReconcileOutcome::Adjusted {
                    previous,
                    target,
                    adjustment,
                    transaction_id,
                } => {
                    print_adjusted(&account_name, previous, target, adjustment, &transaction_id);
                }
            }
            Ok(())
        })
        .await
}

/// Resolves `query` against `accounts`, falling back to an interactive picker
/// when connected to a terminal if it doesn't match exactly one open
/// account.
fn resolve_account_name(accounts: &[Account], query: &str) -> Result<String, CliError> {
    match match_account(accounts, query) {
        AccountMatch::Found(account) => {
            tracing::debug!(account = %account.name, "account resolved");
            Ok(account.name)
        }
        AccountMatch::NotFound => {
            tracing::warn!(query, "account not found; offering interactive picker if possible");

            let opened_accounts: Vec<Account> =
                accounts.iter().filter(|a| !a.closed).cloned().collect();

            let fallback = AppError::ActualAccountNotFound {
                name: Some(query.to_string()),
                available: available_account_names(&opened_accounts),
            };

            pick_account_interactively(opened_accounts, fallback)
        }
        AccountMatch::Ambiguous(candidates) => {
            tracing::warn!(
                query,
                candidates = candidates.len(),
                "account name is ambiguous; offering interactive picker if possible"
            );

            let fallback = AppError::ActualAccountAmbiguous {
                name: query.to_string(),
                matches: available_account_names(&candidates),
            };

            pick_account_interactively(candidates, fallback)
        }
    }
}

fn pick_account_interactively(
    options: Vec<Account>,
    fallback: AppError,
) -> Result<String, CliError> {
    if options.is_empty() || !std::io::stdout().is_terminal() {
        tracing::debug!(
            options = options.len(),
            interactive = std::io::stdout().is_terminal(),
            "skipping interactive picker"
        );
        return Err(map_app_error(fallback));
    }

    let labels: Vec<String> = options.iter().map(|a| &a.name).cloned().collect();

    tracing::debug!(options = labels.len(), "prompting interactive account picker");

    match inquire::Select::new("Select account:", labels).prompt() {
        Ok(label) => {
            let account = options
                .into_iter()
                .find(|a| a.name == label)
                .expect("selected label must be one of the presented options");
            tracing::info!(account = %account.name, "account selected interactively");
            Ok(account.name)
        }
        Err(err) => {
            tracing::warn!(error = %err, "interactive account selection cancelled or failed");
            Err(map_app_error(fallback))
        }
    }
}

fn map_reconcile_error(err: ab_helpers_server::error::AppError) -> CliError {
    match map_app_error(err) {
        CliError::Failure(e) => CliError::Failure(e.context("reconciliation failed")),
        other => other,
    }
}

fn print_already_at_target(account_name: &str, balance: Money) {
    tracing::info!(account = %account_name, %balance, "account already at target");
    println!(
        "Account already at target\n  Account: {account_name}\n  Balance: {balance}\n  No transaction created."
    );
}

fn print_adjusted(
    account_name: &str,
    previous: Money,
    target: Money,
    adjustment: Money,
    transaction_id: &str,
) {
    tracing::info!(account = %account_name, %adjustment, %transaction_id, "balance adjusted");
    println!(
        "Balance adjusted\n  Account:          {account_name}\n  Previous balance: {previous}\n  Target balance:   {target}\n  Adjustment:       {}\n  Transaction:      {transaction_id}",
        adjustment.signed_str()
    );
}
