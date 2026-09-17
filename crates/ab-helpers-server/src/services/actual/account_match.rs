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
        tracing::debug!(
            query,
            candidates = matches.len(),
            "account name is ambiguous"
        );
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

/// Splits `accounts` into (on-budget, off-budget), each sorted by name.
pub fn split_by_budget(accounts: &[Account]) -> (Vec<&Account>, Vec<&Account>) {
    let (mut on_budget, mut off_budget): (Vec<&Account>, Vec<&Account>) =
        accounts.iter().partition(|a| !a.offbudget);
    on_budget.sort_by_key(|a| &a.name);
    off_budget.sort_by_key(|a| &a.name);
    (on_budget, off_budget)
}

/// Open accounts, sorted by name, suitable for showing a user what their
/// options were.
pub fn available_accounts(accounts: &[Account]) -> Vec<Account> {
    let mut accounts: Vec<Account> = accounts.iter().filter(|a| !a.closed).cloned().collect();
    accounts.sort_by(|a, b| a.name.cmp(&b.name));
    tracing::trace!(count = accounts.len(), "computed available accounts");
    accounts
}

pub fn format_account_groups(accounts: &[Account], line: impl Fn(&Account) -> String) -> String {
    let (on_budget, off_budget) = split_by_budget(accounts);

    [("On budget", on_budget), ("Off budget", off_budget)]
        .into_iter()
        .filter(|(_, group)| !group.is_empty())
        .flat_map(|(label, group)| {
            std::iter::once(format!("{label}:"))
                .chain(group.into_iter().map(|a| format!("  {}", line(a))))
        })
        .collect::<Vec<_>>()
        .join("\n")
}
