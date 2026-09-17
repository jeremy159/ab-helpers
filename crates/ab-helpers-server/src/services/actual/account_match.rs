use actual::Account;

/// Result of matching a user-supplied account name against the accounts
/// known to Actual.
pub enum AccountMatch {
    Found(Account),
    NotFound,
    Ambiguous(Vec<Account>),
}

/// Matches `query` against open accounts, case-insensitively and ignoring
/// leading/trailing whitespace on both sides.
pub fn match_account(accounts: &[Account], query: &str) -> AccountMatch {
    let query = query.trim();

    let mut matches: Vec<Account> = accounts
        .iter()
        .filter(|a| !a.closed && a.name.trim().eq_ignore_ascii_case(query))
        .cloned()
        .collect();

    if matches.len() > 1 {
        tracing::debug!(query, candidates = matches.len(), "account name is ambiguous");
        return AccountMatch::Ambiguous(matches);
    }

    match matches.pop() {
        Some(account) => {
            tracing::trace!(query, account = %account.name, "account matched");
            AccountMatch::Found(account)
        }
        None => {
            tracing::trace!(query, "no account matched");
            AccountMatch::NotFound
        }
    }
}

/// Names of open accounts, sorted, suitable for showing a user what their
/// options were.
pub fn available_account_names(accounts: &[Account]) -> Vec<String> {
    let mut names: Vec<String> = accounts
        .iter()
        .filter(|a| !a.closed)
        .map(|a| &a.name)
        .cloned()
        .collect();

    names.sort();
    tracing::trace!(count = names.len(), "computed available account names");

    names
}
