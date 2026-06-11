use super::Money;

/// Result of reconciling an account to a target balance.
#[cfg_attr(any(feature = "testutils", test), derive(fake::Dummy))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconcileOutcome {
    /// The current balance already matched the target; no transaction was created.
    AlreadyAtTarget { balance: Money },
    /// An adjustment transaction was posted.
    Adjusted {
        previous: Money,
        target: Money,
        adjustment: Money,
        transaction_id: String,
    },
}
