#[cfg_attr(any(feature = "testutils", test), derive(fake::Dummy))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterestOutcome {
    AccountClosed,
    NoInterest {
        balance: i64,
    },
    Applied {
        balance: i64,
        interest: i64,
        new_balance: i64,
        transaction_id: String,
    },
}
