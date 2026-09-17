//! Tests for the printed/logged text of `AppError`'s account-lookup
//! variants - these messages are user-facing (surfaced by the CLI) so their
//! exact wording is worth locking down.

use crate::error::AppError;
use actual::Account;

fn account(id: &str, name: &str) -> Account {
    Account {
        id: id.into(),
        name: name.into(),
        offbudget: false,
        closed: false,
    }
}

#[test]
fn account_not_found_with_name_and_no_available_accounts() {
    let err = AppError::ActualAccountNotFound {
        name: Some("Chekcing".to_string()),
        available: vec![],
    };
    assert_eq!(err.to_string(), "Actual account `Chekcing` not found.");
}

#[test]
fn account_not_found_with_name_and_available_accounts() {
    let err = AppError::ActualAccountNotFound {
        name: Some("Chekcing".to_string()),
        available: vec![account("a-1", "Checking"), account("a-2", "Savings")],
    };
    assert_eq!(
        err.to_string(),
        "Actual account `Chekcing` not found. Available accounts: Checking, Savings"
    );
}

#[test]
fn account_not_found_without_a_name_is_still_readable() {
    // `name` is `None` for config-driven lookups (e.g. `actual.kia.account_id`
    // pointing at a nonexistent account) - there's no user-typed name to
    // echo back, so the message must still read sensibly without one.
    let err = AppError::ActualAccountNotFound {
        name: None,
        available: vec![account("a-1", "Checking")],
    };
    assert_eq!(
        err.to_string(),
        "Actual account not found. Available accounts: Checking"
    );
}

#[test]
fn account_not_found_without_name_or_available_accounts() {
    let err = AppError::ActualAccountNotFound {
        name: None,
        available: vec![],
    };
    assert_eq!(err.to_string(), "Actual account not found.");
}

#[test]
fn account_ambiguous_lists_all_matches() {
    let err = AppError::ActualAccountAmbiguous {
        name: "Checking".to_string(),
        matches: vec![account("a-1", "Checking"), account("a-2", "checking")],
    };
    assert_eq!(
        err.to_string(),
        "Multiple Actual accounts match `Checking`: Checking (a-1), checking (a-2)"
    );
}
