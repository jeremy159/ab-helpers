use std::process::ExitCode;

use ab_helpers_server::config::Settings;
use anyhow::Context;
use clap::{Parser, Subcommand};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod commands;

/// abh — Actual Budget Helpers CLI.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Reconcile an account balance to a target value.
    SetBalance(commands::set_balance::SetBalanceArgs),
    /// Apply weekly Kia loan interest.
    ApplyKiaInterest,
    /// Apply monthly mortgage interest.
    ApplyMortgageInterest,
    /// Run the daemon scheduler (production entry point).
    Daemon,
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
        Commands::SetBalance(a) => commands::set_balance::run(settings, a).await,
        Commands::ApplyKiaInterest => commands::apply_kia_interest::run(settings).await,
        Commands::ApplyMortgageInterest => commands::apply_mortgage_interest::run(settings).await,
        Commands::Daemon => commands::daemon::run(settings).await,
    }
}
