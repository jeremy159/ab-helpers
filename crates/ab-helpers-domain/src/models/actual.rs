use serde::{Deserialize, Serialize};

use super::Money;

/// An account as exposed by Actual Budget.
///
/// `id` is Actual's UUID for the account; `name` is the user-facing label we
/// match on from the CLI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActualAccount {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub offbudget: bool,
    #[serde(default)]
    pub closed: bool,
}

/// Result of reconciling an account to a target balance.
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
