use std::process::ExitCode;
use std::sync::Arc;

use ab_helpers_domain::{Money, ReconcileOutcome};
use ab_helpers_server::config::Settings;
use ab_helpers_server::error::AppError;
use ab_helpers_server::services::actual::{ReconcileOptions, ReconcileService, ReconcileServiceExt};
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

pub async fn run(settings: Settings, args: SetBalanceArgs) -> anyhow::Result<ExitCode> {
    tracing::info!(account = %args.account, amount = %args.amount, dry_run = args.dry_run, "set-balance started");

    let client = settings.actual.client();
    let service = ReconcileService::new(Arc::new(client));

    if args.dry_run {
        return run_dry_run(&settings, &args).await;
    }

    let opts = ReconcileOptions {
        date: args.date,
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
            Ok(ExitCode::SUCCESS)
        }
        Err(AppError::ActualAccountNotFound(name)) => {
            tracing::warn!(account = %name, "account not found in Actual");
            Ok(ExitCode::from(1))
        }
        Err(AppError::ActualAccountAmbiguous { name, matches }) => {
            tracing::warn!(account = %name, matches = %matches, "account name is ambiguous");
            Ok(ExitCode::from(1))
        }
        Err(AppError::Actual(err)) => {
            tracing::error!(?err, "Actual API error during reconciliation");
            Ok(ExitCode::from(3))
        }
        Err(err) => {
            tracing::error!(?err, "reconciliation failed");
            Ok(ExitCode::from(3))
        }
    }
}

async fn run_dry_run(settings: &Settings, args: &SetBalanceArgs) -> anyhow::Result<ExitCode> {
    use actual::{AccountRequests, Client};

    tracing::debug!(account = %args.account, "fetching accounts for dry-run");
    let client = Client::new(settings.actual.bridge_config());
    let accounts = client.list_accounts().await?;
    tracing::trace!(count = accounts.len(), "accounts fetched");

    let matches: Vec<&actual::Account> = accounts
        .iter()
        .filter(|a| !a.closed && a.name == args.account)
        .collect();
    let account = match matches.as_slice() {
        [] => {
            tracing::warn!(account = %args.account, "account not found in Actual (dry-run)");
            return Ok(ExitCode::from(1));
        }
        [only] => *only,
        many => {
            let names = many.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ");
            tracing::warn!(account = %args.account, matches = %names, "account name is ambiguous (dry-run)");
            return Ok(ExitCode::from(1));
        }
    };
    let current = Money::from_cents(client.get_account_balance(&account.id).await?);
    let diff = args.amount - current;
    tracing::debug!(account_id = %account.id, current = %current, target = %args.amount, diff = %diff, "dry-run balance computed");

    if diff.is_zero() {
        tracing::info!(
            account = %account.name,
            "dry-run: account already at target\n  Account:         {}\n  Current balance: ${current}\n  Target balance:  ${}\n  No transaction would be created.",
            account.name,
            args.amount
        );
    } else {
        let sign = if diff.cents() > 0 { "+" } else { "" };
        tracing::info!(
            account = %account.name,
            diff = %diff,
            "dry-run: would adjust balance\n  Account:         {}\n  Current balance: ${current}\n  Target balance:  ${}\n  Adjustment (dry): {sign}${diff}",
            account.name,
            args.amount
        );
    }
    Ok(ExitCode::SUCCESS)
}

fn print_outcome(account_name: &str, outcome: &ReconcileOutcome) {
    match outcome {
        ReconcileOutcome::AlreadyAtTarget { balance } => {
            tracing::info!(
                account = %account_name,
                balance = %balance,
                "account already at target\n  Account: {account_name}\n  Balance: ${balance}\n  No transaction created."
            );
        }
        ReconcileOutcome::Adjusted {
            previous,
            target,
            adjustment,
            transaction_id,
        } => {
            let sign = if adjustment.cents() > 0 { "+" } else { "" };
            tracing::info!(
                account = %account_name,
                adjustment = %adjustment,
                transaction_id = %transaction_id,
                "balance adjusted\n  Account:          {account_name}\n  Previous balance: ${previous}\n  Target balance:   ${target}\n  Adjustment:       {sign}${adjustment}\n  Transaction:      {transaction_id}"
            );
        }
    }
}
