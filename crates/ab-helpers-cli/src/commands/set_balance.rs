use std::sync::Arc;

use ab_helpers_domain::{Money, ReconcileOutcome, ReconcileSkip};
use anyhow::Context as _;

use super::error::{map_app_error, CliError};
use ab_helpers_server::config::Settings;
use ab_helpers_server::execution::{DryRun, Live, PlanExecute, Preview};
use ab_helpers_server::services::actual::{ReconcileOptions, ReconcileService};
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

    let client = Arc::new(settings.actual.client());

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

    let service = ReconcileService::new(client, args.account.clone(), args.amount, opts);

    if args.dry_run {
        tracing::debug!(account = %args.account, "previewing reconcile (dry-run)");
        match service.run::<DryRun>().await.map_err(map_reconcile_error)? {
            Preview::Skip(ReconcileSkip::AlreadyAtTarget { balance }) => {
                tracing::info!(account = %args.account, %balance, "account already at target (dry-run)");
                println!(
                    "No adjustment needed\n  Account:         {}\n  Current balance: ${balance}\n  Target balance:  ${}\n  No transaction would be created.",
                    args.account, args.amount
                );
            }
            Preview::WouldApply(plan) => {
                tracing::info!(account = %plan.account_name, diff = %plan.diff, "dry-run: would adjust balance");
                println!(
                    "Would adjust balance\n  Account:         {}\n  Current balance: ${}\n  Target balance:  ${}\n  Adjustment (dry): {}",
                    plan.account_name,
                    plan.current,
                    plan.target,
                    plan.diff.signed_str()
                );
            }
        }
        return Ok(());
    }

    tracing::debug!(account = %args.account, "reconciling account balance");
    match service.run::<Live>().await.map_err(map_reconcile_error)? {
        ReconcileOutcome::AlreadyAtTarget { balance } => {
            print_already_at_target(&args.account, balance);
        }
        ReconcileOutcome::Adjusted {
            previous,
            target,
            adjustment,
            transaction_id,
        } => {
            print_adjusted(&args.account, previous, target, adjustment, &transaction_id);
        }
    }
    Ok(())
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
        "Account already at target\n  Account: {account_name}\n  Balance: ${balance}\n  No transaction created."
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
        "Balance adjusted\n  Account:          {account_name}\n  Previous balance: ${previous}\n  Target balance:   ${target}\n  Adjustment:       {}\n  Transaction:      {transaction_id}",
        adjustment.signed_str()
    );
}
