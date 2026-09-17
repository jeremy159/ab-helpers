//! Tests for the messages printed to the user when an account lookup fails -
//! `map_app_error` (user-typed account names, e.g. `set-balance`) and
//! `map_app_error_labelled` (config-driven lookups, e.g. the interest
//! commands).

use crate::commands::error::{CliError, map_app_error, map_app_error_labelled};
use ab_helpers_server::error::AppError;

fn message(err: CliError) -> String {
    match err {
        CliError::NotFound(message) => message,
        CliError::Failure(e) => panic!("expected CliError::NotFound, got Failure: {e:?}"),
    }
}

#[test]
fn not_found_by_name_lists_available_accounts() {
    let err = AppError::ActualAccountNotFound {
        name: Some("Chekcing".to_string()),
        available: vec!["Checking".to_string(), "Savings".to_string()],
    };
    assert_eq!(
        message(map_app_error(err)),
        "Account \"Chekcing\" not found.\nAvailable accounts:\n  Checking\n  Savings"
    );
}

#[test]
fn not_found_by_name_with_no_available_accounts() {
    let err = AppError::ActualAccountNotFound {
        name: Some("Checking".to_string()),
        available: vec![],
    };
    assert_eq!(message(map_app_error(err)), "Account \"Checking\" not found.");
}

#[test]
fn ambiguous_lists_all_candidates() {
    let err = AppError::ActualAccountAmbiguous {
        name: "Checking".to_string(),
        matches: vec!["Checking (a-1)".to_string(), "checking (a-2)".to_string()],
    };
    assert_eq!(
        message(map_app_error(err)),
        "\"Checking\" matches more than one account:\n  Checking (a-1)\n  checking (a-2)"
    );
}

#[test]
fn other_errors_pass_through_as_failure() {
    let err = AppError::Unexpected(anyhow::anyhow!("boom"));
    assert!(matches!(map_app_error(err), CliError::Failure(_)));
}

#[test]
fn labelled_not_found_without_a_name_points_at_the_config_key() {
    // Interest commands resolve an account by a configured id, not a name a
    // user typed - `name` is `None` in that case, and the message must name
    // the config key instead of the meaningless raw id.
    let err = AppError::ActualAccountNotFound {
        name: None,
        available: vec!["Checking".to_string()],
    };
    assert_eq!(
        message(map_app_error_labelled(err, "kia")),
        "Configured `kia` account (`actual.kia.account_id`) not found.\nAvailable accounts:\n  Checking"
    );
}

#[test]
fn labelled_not_found_with_a_name_still_uses_the_name() {
    let err = AppError::ActualAccountNotFound {
        name: Some("Checking".to_string()),
        available: vec![],
    };
    assert_eq!(
        message(map_app_error_labelled(err, "kia")),
        "Account \"Checking\" not found."
    );
}

#[test]
fn labelled_ambiguous_includes_the_kind_in_the_log_but_not_the_message() {
    let err = AppError::ActualAccountAmbiguous {
        name: "Checking".to_string(),
        matches: vec!["Checking (a-1)".to_string(), "Checking (a-2)".to_string()],
    };
    assert_eq!(
        message(map_app_error_labelled(err, "mortgage")),
        "\"Checking\" matches more than one account:\n  Checking (a-1)\n  Checking (a-2)"
    );
}
