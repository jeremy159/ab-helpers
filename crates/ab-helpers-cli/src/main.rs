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
    ApplyKiaInterest(commands::apply_kia_interest::ApplyKiaInterestArgs),
    /// Apply monthly mortgage interest.
    ApplyMortgageInterest(commands::apply_mortgage_interest::ApplyMortgageInterestArgs),
    /// Run the daemon scheduler (production entry point).
    Daemon,
    /// Seed ~/.config/ab-helpers with base.toml + a starter config.toml.
    Init(commands::init::InitArgs),
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

    // `init` creates the config, so it must not require it to already exist;
    // every other command loads settings first.
    match args.command {
        Commands::Init(a) => commands::init::run(a),
        command => {
            let settings = Settings::build().context("failed to load configuration")?;
            match command {
                Commands::SetBalance(a) => commands::set_balance::run(settings, a).await,
                Commands::ApplyKiaInterest(a) => {
                    commands::apply_kia_interest::run(settings, a).await
                }
                Commands::ApplyMortgageInterest(a) => {
                    commands::apply_mortgage_interest::run(settings, a).await
                }
                Commands::Daemon => commands::daemon::run(settings).await,
                Commands::Init(_) => unreachable!("handled above"),
            }
        }
    }
}
