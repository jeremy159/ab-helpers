use std::sync::Arc;

use ab_helpers_domain::{Money, ReconcileOutcome};
use anyhow::Context as _;

use super::error::CliError;
use ab_helpers_server::config::Settings;
use ab_helpers_server::error::AppError;
use ab_helpers_server::services::actual::{Reconcile, ReconcileOptions, ReconcileService};
use clap::Args;

#[derive(Args, Debug)]
pub struct SetBalanceArgs {
    /// Exact name of the account in Actual.
    pub account: String,

    /// Target balance, e.g. `1234.56` or `-50`.
    pub amount: Money,

    /// Date in `YYYY-MM-DD`. Defaults to today (resolved by the bridge).
    #[arg(long)]
    pub date: Option<String>,

    /// Override the payee name on the adjustment transaction.
    #[arg(long, default_value = "Balance Adjustment")]
    pub payee_name: String,

    /// Notes attached to the adjustment transaction.
    #[arg(long)]
    pub notes: Option<String>,

    /// Compute the diff and print what would be done; do not write anything.
    #[arg(long)]
    pub dry_run: bool,
}

pub async fn run(settings: Settings, args: SetBalanceArgs) -> Result<(), CliError> {
    tracing::info!(account = %args.account, amount = %args.amount, dry_run = args.dry_run, "set-balance started");

    let client = settings.actual.client();
    let service = ReconcileService::new(Arc::new(client));

    if args.dry_run {
        return run_dry_run(&settings, &args).await;
    }

    let opts = ReconcileOptions {
        date: args
            .date
            .as_deref()
            .map(|s| {
                chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
                    .context("--date must be in YYYY-MM-DD format")
            })
            .transpose()?,
        payee_name: Some(args.payee_name),
        notes: args.notes,
    };

    tracing::debug!(account = %args.account, "reconciling account balance");
    match service
        .reconcile_account_to(&args.account, args.amount, opts)
        .await
    {
        Ok(outcome) => {
            print_outcome(&args.account, &outcome);
            Ok(())
        }
        Err(AppError::ActualAccountNotFound(name)) => {
            tracing::warn!(account = %name, "account not found in Actual");
            Err(CliError::NotFound)
        }
        Err(AppError::ActualAccountAmbiguous { name, matches }) => {
            tracing::warn!(account = %name, matches = %matches.join(", "), "account name is ambiguous");
            Err(CliError::NotFound)
        }
        Err(err) => Err(CliError::Failure(
            anyhow::Error::from(err).context("reconciliation failed"),
        )),
    }
}

async fn run_dry_run(settings: &Settings, args: &SetBalanceArgs) -> Result<(), CliError> {
    use actual::{AccountRequests, Client};

    tracing::debug!(account = %args.account, "fetching accounts for dry-run");
    let client = Client::new(settings.actual.bridge_config());
    let accounts = client
        .list_accounts()
        .await
        .map_err(|e| CliError::Failure(e.into()))?;
    tracing::trace!(count = accounts.len(), "accounts fetched");

    let matches: Vec<&actual::Account> = accounts
        .iter()
        .filter(|a| !a.closed && a.name == args.account)
        .collect();
    let account = match matches.as_slice() {
        [] => {
            tracing::warn!(account = %args.account, "account not found in Actual (dry-run)");
            return Err(CliError::NotFound);
        }
        [only] => *only,
        many => {
            let names = many
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            tracing::warn!(account = %args.account, matches = %names, "account name is ambiguous (dry-run)");
            return Err(CliError::NotFound);
        }
    };
    let current = Money::from_cents(
        client
            .get_account_balance(&account.id)
            .await
            .map_err(|e| CliError::Failure(e.into()))?,
    );
    let diff = args.amount - current;
    tracing::debug!(account_id = %account.id, current = %current, target = %args.amount, diff = %diff, "dry-run balance computed");

    if diff.is_zero() {
        tracing::info!(account = %account.name, "dry-run: account already at target");
        println!(
            "No adjustment needed\n  Account:         {}\n  Current balance: ${current}\n  Target balance:  ${}\n  No transaction would be created.",
            account.name, args.amount
        );
    } else {
        tracing::info!(account = %account.name, diff = %diff, "dry-run: would adjust balance");
        println!(
            "Would adjust balance\n  Account:         {}\n  Current balance: ${current}\n  Target balance:  ${}\n  Adjustment (dry): {}",
            account.name,
            args.amount,
            diff.signed_str()
        );
    }
    Ok(())
}

fn print_outcome(account_name: &str, outcome: &ReconcileOutcome) {
    match outcome {
        ReconcileOutcome::AlreadyAtTarget { balance } => {
            tracing::info!(
                account = %account_name,
                balance = %balance,
                "account already at target"
            );
            println!(
                "Account already at target\n  Account: {account_name}\n  Balance: ${balance}\n  No transaction created."
            );
        }
        ReconcileOutcome::Adjusted {
            previous,
            target,
            adjustment,
            transaction_id,
        } => {
            tracing::info!(
                account = %account_name,
                adjustment = %adjustment,
                transaction_id = %transaction_id,
                "balance adjusted"
            );
            println!(
                "Balance adjusted\n  Account:          {account_name}\n  Previous balance: ${previous}\n  Target balance:   ${target}\n  Adjustment:       {}\n  Transaction:      {transaction_id}",
                adjustment.signed_str()
            );
        }
    }
}
