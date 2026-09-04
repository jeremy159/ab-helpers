use std::process::ExitCode;

use ab_helpers_server::config::Settings;
use anyhow::Context;
use clap::{Parser, Subcommand};
use commands::error::CliError;
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
    ApplyKiaInterest(commands::interest::InterestArgs),
    /// Apply monthly mortgage interest.
    ApplyMortgageInterest(commands::interest::InterestArgs),
    /// Run the daemon scheduler (production entry point).
    Daemon,
}

#[tokio::main]
async fn main() -> ExitCode {
    let args = Cli::parse();
    init_tracing(default_filter_for(&args.command));
    match run(args).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(CliError::NotFound) => ExitCode::from(1),
        Err(CliError::Failure(err)) => {
            tracing::error!("{:#}", err);
            ExitCode::from(3)
        }
    }
}

/// The daemon is unattended (no TTY, this is what Docker runs) and relies on
/// tracing as its only output, so it keeps the informative default. One-shot
/// commands already report their outcome via `println!`, so tracing only
/// needs to surface anomalies/errors by default.
fn default_filter_for(command: &Commands) -> &'static str {
    match command {
        Commands::WithSettings(SettingsCommand::Daemon) => "abh=info,actual=info",
        _ => "abh=warn,actual=warn",
    }
}

fn init_tracing(default: &str) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default));
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();
}

async fn run(args: Cli) -> Result<(), CliError> {
    tracing::info!(command = ?args.command, "abh CLI started");

    match args.command {
        Commands::Init(a) => commands::init::run(a),
        Commands::WithSettings(cmd) => {
            let settings = Settings::build().context("failed to load configuration")?;

            tracing::debug!("configuration loaded");

            match cmd {
                SettingsCommand::SetBalance(a) => commands::set_balance::run(settings, a).await,
                SettingsCommand::ApplyKiaInterest(a) => {
                    commands::interest::run(settings, a, commands::interest::InterestKind::Kia)
                        .await
                }
                SettingsCommand::ApplyMortgageInterest(a) => {
                    commands::interest::run(settings, a, commands::interest::InterestKind::Mortgage)
                        .await
                }
                SettingsCommand::Daemon => commands::daemon::run(settings).await,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Cli, SettingsCommand, Commands, default_filter_for};
    use clap::Parser;

    #[test]
    fn daemon_defaults_to_info() {
        let command = Commands::WithSettings(SettingsCommand::Daemon);
        assert_eq!(default_filter_for(&command), "abh=info,actual=info");
    }

    #[test]
    fn one_shot_commands_default_to_warn() {
        let init = Cli::parse_from(["abh", "init"]).command;
        assert_eq!(default_filter_for(&init), "abh=warn,actual=warn");

        let interest = Cli::parse_from(["abh", "apply-kia-interest"]).command;
        assert_eq!(default_filter_for(&interest), "abh=warn,actual=warn");
    }
}
