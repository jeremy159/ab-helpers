use actual::Payee;

/// Result of matching a configured payee name against the payees known to
/// Actual - mirrors [`super::AccountMatch`].
pub enum PayeeMatch {
    Found(Payee),
    NotFound,
    Ambiguous(Vec<Payee>),
}

impl PayeeMatch {
    /// Matches `query` against `payees`, case-insensitively and ignoring
    /// leading/trailing whitespace on both sides (Actual allows duplicate payee
    /// names, hence `Ambiguous`).
    pub fn resolve(payees: &[Payee], query: &str) -> Self {
        let query = query.trim();

        let mut matches: Vec<Payee> = payees
            .iter()
            .filter(|p| p.name.trim().eq_ignore_ascii_case(query))
            .cloned()
            .collect();

        match matches.len() {
            0 => Self::NotFound,
            1 => Self::Found(matches.remove(0)),
            _ => Self::Ambiguous(matches),
        }
    }
}
