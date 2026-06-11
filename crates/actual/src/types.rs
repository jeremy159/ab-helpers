use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// Account information returned by the bridge.
#[cfg_attr(any(feature = "testutils", test), derive(fake::Dummy))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Account {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub offbudget: bool,
    #[serde(default)]
    pub closed: bool,
}

#[cfg_attr(any(feature = "testutils", test), derive(fake::Dummy))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListAccountsResponse {
    pub accounts: Vec<Account>,
}

#[cfg_attr(any(feature = "testutils", test), derive(fake::Dummy))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceResponse {
    /// Balance in integer cents.
    pub balance: i64,
}

#[cfg_attr(any(feature = "testutils", test), derive(fake::Dummy))]
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

#[cfg_attr(any(feature = "testutils", test), derive(fake::Dummy))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddTransactionResponse {
    pub id: String,
}

#[cfg_attr(any(feature = "testutils", test), derive(fake::Dummy))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LastTransactionResponse {
    pub date: String,
    pub amount: i64,
}

#[cfg_attr(any(feature = "testutils", test), derive(fake::Dummy))]
#[derive(Debug, Clone)]
pub struct LastTransaction {
    pub date: NaiveDate,
    pub amount: i64,
}

#[cfg_attr(any(feature = "testutils", test), derive(fake::Dummy))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnsurePayeeResponse {
    pub id: String,
}

#[cfg_attr(any(feature = "testutils", test), derive(fake::Dummy))]
#[derive(Debug, Clone)]
pub struct ImportTransaction {
    pub account_id: String,
    pub date: NaiveDate,
    pub payee_id: String,
    pub amount: i64,
    pub notes: Option<String>,
    pub cleared: Option<bool>,
}

/// Wire format for import-transaction bridge call.
#[cfg_attr(any(feature = "testutils", test), derive(fake::Dummy))]
#[derive(Debug, Clone, Serialize)]
pub struct ImportTransactionRequest {
    #[serde(rename = "accountId")]
    pub account_id: String,
    pub date: String,
    #[serde(rename = "payeeId")]
    pub payee_id: String,
    pub amount: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cleared: Option<bool>,
}
