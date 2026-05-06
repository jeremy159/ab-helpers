use std::process::ExitCode;
use std::sync::Arc;

use anyhow::Context;
use ab_helpers_domain::{Money, ReconcileOutcome};
use ab_helpers_server::config::Settings;
use ab_helpers_server::error::AppError;
use ab_helpers_server::services::actual::{
    ReconcileOptions, ReconcileService, ReconcileServiceExt,
};
use clap::{Parser, Subcommand};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Budgetize companion CLI for Actual Budget.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Reconcile an account so its balance matches the given target.
    ///
    /// Looks up the named account, reads its current balance, and posts a
    /// single adjustment transaction equal to `target - current`.
    SetBalance(SetBalanceArgs),
}

#[derive(clap::Args, Debug)]
struct SetBalanceArgs {
    /// Exact name of the account in Actual.
    account: String,

    /// Target balance, e.g. `1234.56` or `-50`.
    amount: Money,

    /// Date in `YYYY-MM-DD`. Defaults to today (resolved by the bridge).
    #[arg(long)]
    date: Option<String>,

    /// Override the payee name on the adjustment transaction.
    #[arg(long, default_value = "Balance Adjustment")]
    payee_name: String,

    /// Notes attached to the adjustment transaction.
    #[arg(long)]
    notes: Option<String>,

    /// Compute the diff and print what would be done; do not write anything.
    #[arg(long)]
    dry_run: bool,
}

#[tokio::main]
async fn main() -> ExitCode {
    init_tracing();

    match run().await {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {err:?}");
            ExitCode::from(3)
        }
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("ab_helpers_cli=info,actual=info"));
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();
}

async fn run() -> anyhow::Result<ExitCode> {
    let args = Cli::parse();
    let settings = Settings::build().context("failed to load configuration")?;

    match args.command {
        Commands::SetBalance(a) => set_balance(settings, a).await,
    }
}

async fn set_balance(settings: Settings, args: SetBalanceArgs) -> anyhow::Result<ExitCode> {
    let client = settings.actual.client();
    let service = ReconcileService::new(Arc::new(client));

    if args.dry_run {
        // For dry-run we still hit the bridge to discover the current balance
        // and account, but we tell the service nothing — instead we re-read
        // the data ourselves to avoid calling add_transaction. This keeps the
        // service trait single-purpose.
        return run_dry_run(&settings, &args).await;
    }

    let opts = ReconcileOptions {
        date: args.date,
        payee_name: Some(args.payee_name),
        notes: args.notes,
    };

    match service
        .reconcile_account_to(&args.account, args.amount, opts)
        .await
    {
        Ok(outcome) => {
            print_outcome(&args.account, &outcome);
            Ok(ExitCode::SUCCESS)
        }
        Err(AppError::ActualAccountNotFound(name)) => {
            eprintln!("Account `{name}` not found in Actual.");
            Ok(ExitCode::from(1))
        }
        Err(AppError::ActualAccountAmbiguous { name, matches }) => {
            eprintln!("Account `{name}` is ambiguous; matches: {matches}");
            Ok(ExitCode::from(1))
        }
        Err(AppError::Actual(err)) => {
            eprintln!("Actual error: {err}");
            Ok(ExitCode::from(3))
        }
        Err(err) => {
            eprintln!("error: {err:?}");
            Ok(ExitCode::from(3))
        }
    }
}

async fn run_dry_run(settings: &Settings, args: &SetBalanceArgs) -> anyhow::Result<ExitCode> {
    use actual::{AccountRequests, Client};

    let client = Client::new(settings.actual.bridge_config());
    let accounts = client.list_accounts().await?;
    let matches: Vec<&actual::Account> = accounts
        .iter()
        .filter(|a| !a.closed && a.name == args.account)
        .collect();
    let account = match matches.as_slice() {
        [] => {
            eprintln!("Account `{}` not found in Actual.", args.account);
            return Ok(ExitCode::from(1));
        }
        [only] => *only,
        many => {
            let names = many.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ");
            eprintln!(
                "Account `{}` is ambiguous; matches: {names}",
                args.account
            );
            return Ok(ExitCode::from(1));
        }
    };
    let current = Money::from_cents(client.get_account_balance(&account.id).await?);
    let diff = args.amount - current;

    println!("Account:           {}", account.name);
    println!("Current balance:   ${current}");
    println!("Target balance:    ${}", args.amount);
    if diff.is_zero() {
        println!("Already at target. No transaction would be created.");
    } else {
        let sign = if diff.cents() > 0 { "+" } else { "" };
        println!("Adjustment (dry):  {sign}${diff}");
    }
    Ok(ExitCode::SUCCESS)
}

fn print_outcome(account_name: &str, outcome: &ReconcileOutcome) {
    match outcome {
        ReconcileOutcome::AlreadyAtTarget { balance } => {
            println!(
                "Account `{account_name}` already at ${balance}. No transaction created."
            );
        }
        ReconcileOutcome::Adjusted {
            previous,
            target,
            adjustment,
            transaction_id,
        } => {
            let sign = if adjustment.cents() > 0 { "+" } else { "" };
            println!("Account:           {account_name}");
            println!("Previous balance:  ${previous}");
            println!("Target balance:    ${target}");
            println!("Adjustment:        {sign}${adjustment}");
            println!("Transaction:       {transaction_id}");
        }
    }
}
