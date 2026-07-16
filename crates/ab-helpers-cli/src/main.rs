use std::process::ExitCode;

use ab_helpers_server::config::Settings;
use anyhow::Context;
use clap::{Parser, Subcommand};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

mod commands;

/// abh: Actual Budget Helpers CLI.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

/// Commands that run without loading configuration first.
#[derive(Subcommand, Debug)]
enum Commands {
    /// Seed ~/.config/ab-helpers with base.toml + a starter config.toml.
    Init(commands::init::InitArgs),
    #[command(flatten)]
    WithSettings(SettingsCommand),
}

/// Commands that require configuration to be loaded first.
#[derive(Subcommand, Debug)]
enum SettingsCommand {
    /// Reconcile an account balance to a target value.
    SetBalance(commands::set_balance::SetBalanceArgs),
    /// Apply weekly Kia loan interest.
    ApplyKiaInterest(commands::apply_kia_interest::ApplyKiaInterestArgs),
    /// Apply monthly mortgage interest.
    ApplyMortgageInterest(commands::apply_mortgage_interest::ApplyMortgageInterestArgs),
    /// Run the daemon scheduler (production entry point).
    Daemon,
}

#[tokio::main]
async fn main() -> ExitCode {
    init_tracing();
    match run().await {
        Ok(code) => code,
        Err(err) => {
            tracing::error!("{:#}", err);
            ExitCode::from(3)
        }
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("abh=info,actual=info"));
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();
}

async fn run() -> anyhow::Result<ExitCode> {
    let args = Cli::parse();

    tracing::info!(command = ?args.command, "abh CLI started");

    match args.command {
        Commands::Init(a) => commands::init::run(a),
        Commands::WithSettings(cmd) => {
            let settings = Settings::build().context("failed to load configuration")?;

            tracing::debug!("configuration loaded");

            match cmd {
                SettingsCommand::SetBalance(a) => commands::set_balance::run(settings, a).await,
                SettingsCommand::ApplyKiaInterest(a) => {
                    commands::apply_kia_interest::run(settings, a).await
                }
                SettingsCommand::ApplyMortgageInterest(a) => {
                    commands::apply_mortgage_interest::run(settings, a).await
                }
                SettingsCommand::Daemon => commands::daemon::run(settings).await,
            }
        }
    }
}
