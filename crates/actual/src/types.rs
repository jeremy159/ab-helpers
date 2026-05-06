use serde::{Deserialize, Serialize};

/// Account information returned by the bridge.
///
/// Mirrors `budgetize_domain::ActualAccount` but lives in the client crate to
/// keep the wire format independent of the domain model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Account {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub offbudget: bool,
    #[serde(default)]
    pub closed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListAccountsResponse {
    pub accounts: Vec<Account>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceResponse {
    /// Balance in integer cents.
    pub balance: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveTransaction {
    #[serde(rename = "accountId")]
    pub account_id: String,
    /// Amount in integer cents. Positive = inflow, negative = outflow.
    pub amount: i64,
    #[serde(rename = "payeeName", skip_serializing_if = "Option::is_none")]
    pub payee_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// `YYYY-MM-DD`. If `None`, the bridge uses the local current date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddTransactionResponse {
    pub id: String,
}
