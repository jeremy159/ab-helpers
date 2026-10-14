use crate::{Cli, Commands, SettingsCommand, default_filter_for};
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

#[test]
fn budget_details_parses_with_defaults() {
    let command = Cli::parse_from(["abh", "budget-details"]).command;
    match command {
        Commands::WithSettings(SettingsCommand::BudgetDetails(args)) => {
            assert_eq!(args.month, None);
        }
        other => panic!("expected BudgetDetails, got {other:?}"),
    }
}

#[test]
fn budget_details_parses_month_and_budget_mode() {
    use crate::commands::budget_details::BudgetDetailsArgs;

    let command = Cli::parse_from([
        "abh",
        "budget-details",
        "--month",
        "2026-03",
        "--budget-mode",
        "actual",
    ])
    .command;
    match command {
        Commands::WithSettings(SettingsCommand::BudgetDetails(BudgetDetailsArgs {
            month,
            budget_mode,
        })) => {
            assert_eq!(month.as_deref(), Some("2026-03"));
            assert!(matches!(
                budget_mode,
                crate::commands::budget_summary::BudgetModeArg::Actual
            ));
        }
        other => panic!("expected BudgetDetails, got {other:?}"),
    }
}
