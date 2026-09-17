use super::super::*;
use actual::Account;

fn account(id: &str, name: &str, closed: bool) -> Account {
    Account {
        id: id.into(),
        name: name.into(),
        offbudget: false,
        closed,
    }
}

#[test]
fn matches_case_insensitively_and_trims_whitespace() {
    let accounts = vec![account("a-1", "Checking", false)];
    assert!(matches!(
        match_account(&accounts, "  checking  "),
        AccountMatch::Found(a) if a.id == "a-1"
    ));
}

#[test]
fn found_account_is_independent_of_the_input_slice() {
    let accounts = vec![account("a-1", "Checking", false)];
    let AccountMatch::Found(account) = match_account(&accounts, "Checking") else {
        panic!("expected Found");
    };
    drop(accounts);
    assert_eq!(account.id, "a-1");
}

#[test]
fn ignores_closed_accounts() {
    let accounts = vec![account("a-1", "Checking", true)];
    assert!(matches!(
        match_account(&accounts, "Checking"),
        AccountMatch::NotFound
    ));
}

#[test]
fn reports_ambiguous_when_multiple_open_accounts_match() {
    let accounts = vec![
        account("a-1", "Checking", false),
        account("a-2", "checking", false),
    ];
    assert!(matches!(
        match_account(&accounts, "Checking"),
        AccountMatch::Ambiguous(m) if m.len() == 2
    ));
}

#[test]
fn reports_not_found_when_no_accounts_match() {
    let accounts = vec![account("a-1", "Savings", false)];
    assert!(matches!(
        match_account(&accounts, "Checking"),
        AccountMatch::NotFound
    ));
}
