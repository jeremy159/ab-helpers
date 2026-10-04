use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

fn deserialize_naive_date<'de, D: serde::Deserializer<'de>>(d: D) -> Result<NaiveDate, D::Error> {
    let s = String::deserialize(d)?;
    NaiveDate::parse_from_str(&s, "%Y-%m-%d").map_err(serde::de::Error::custom)
}

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
#[serde(rename_all = "camelCase")]
pub struct SaveTransaction {
    pub account_id: String,
    /// Amount in integer cents. Positive = inflow, negative = outflow.
    pub amount: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
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
    #[serde(deserialize_with = "deserialize_naive_date")]
    pub date: NaiveDate,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindTransactionResponse {
    pub found: bool,
    #[serde(default)]
    pub payee_name: Option<String>,
    #[serde(default)]
    pub date: Option<NaiveDate>,
    #[serde(default)]
    pub amount: Option<i64>,
}

/// An existing transaction found on an account for a given date/payee - what
/// we already know about it without a second round-trip.
#[cfg_attr(any(feature = "testutils", test), derive(fake::Dummy))]
#[derive(Debug, Clone)]
pub struct ExistingTransaction {
    pub payee_name: String,
    pub date: NaiveDate,
    pub amount: i64,
}

#[cfg_attr(any(feature = "testutils", test), derive(fake::Dummy))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountNoteResponse {
    pub note: Option<String>,
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
#[serde(rename_all = "camelCase")]
pub struct ImportTransactionRequest {
    pub account_id: String,
    pub date: String,
    pub payee_id: String,
    pub amount: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cleared: Option<bool>,
}

/// A budget category, with its budgeted amount for the month a
/// [`BudgetMonth`] was fetched for.
#[cfg_attr(any(feature = "testutils", test), derive(fake::Dummy))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub is_income: bool,
    #[serde(default)]
    pub hidden: bool,
    /// Integer cents. `None` in envelope-budget mode for income categories,
    /// which carry no `budgeted` value.
    #[serde(default)]
    pub budgeted: Option<i64>,
}

#[cfg_attr(any(feature = "testutils", test), derive(fake::Dummy))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CategoryGroup {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub is_income: bool,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub categories: Vec<Category>,
}

#[cfg_attr(any(feature = "testutils", test), derive(fake::Dummy))]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetMonthResponse {
    pub month: String,
    pub category_groups: Vec<CategoryGroup>,
}

#[cfg_attr(any(feature = "testutils", test), derive(fake::Dummy))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Payee {
    pub id: String,
    pub name: String,
}

#[cfg_attr(any(feature = "testutils", test), derive(fake::Dummy))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayeesResponse {
    pub payees: Vec<Payee>,
}

/// A single recurrence pattern entry within a [`RecurringConfig`]: either a
/// day-of-month (`pattern_type: Day`, `value` = 1-31) or an nth weekday of
/// the month (`pattern_type` = the weekday, `value` = which occurrence,
/// e.g. 2nd Tuesday).
#[cfg_attr(any(feature = "testutils", test), derive(fake::Dummy))]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecurringPattern {
    pub value: i32,
    #[serde(rename = "type")]
    pub pattern_type: RecurringPatternType,
}

#[cfg_attr(any(feature = "testutils", test), derive(fake::Dummy))]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum RecurringPatternType {
    #[serde(rename = "day")]
    Day,
    Su,
    Mo,
    Tu,
    We,
    Th,
    Fr,
    Sa,
}

#[cfg_attr(any(feature = "testutils", test), derive(fake::Dummy))]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RecurringFrequency {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

#[cfg_attr(any(feature = "testutils", test), derive(fake::Dummy))]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecurringEndMode {
    Never,
    #[serde(rename = "after_n_occurrences")]
    AfterOccurrences,
    OnDate,
}

/// Actual's recurrence config (`RecurringConfig`), as returned in a schedule's
/// `date` field when the schedule repeats.
#[cfg_attr(any(feature = "testutils", test), derive(fake::Dummy))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecurringConfig {
    pub frequency: RecurringFrequency,
    #[serde(default)]
    pub interval: Option<u32>,
    #[serde(default)]
    pub patterns: Option<Vec<RecurringPattern>>,
    #[serde(default)]
    pub skip_weekend: Option<bool>,
    /// `YYYY-MM-DD`.
    pub start: String,
    #[serde(default)]
    pub end_mode: Option<RecurringEndMode>,
    #[serde(default)]
    pub end_occurrences: Option<u32>,
    /// `YYYY-MM-DD`.
    #[serde(default)]
    pub end_date: Option<String>,
    #[serde(default)]
    pub weekend_solve_mode: Option<WeekendSolveMode>,
}

#[cfg_attr(any(feature = "testutils", test), derive(fake::Dummy))]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WeekendSolveMode {
    Before,
    After,
}

/// A schedule's `date` condition: either a recurrence, or a plain one-off
/// `YYYY-MM-DD` date.
#[cfg_attr(any(feature = "testutils", test), derive(fake::Dummy))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum ScheduleDate {
    Recurring(RecurringConfig),
    Fixed(String),
}

/// A schedule's `amount` condition: either a fixed value, or an
/// `isbetween` range (`amountOp: "isbetween"`).
#[cfg_attr(any(feature = "testutils", test), derive(fake::Dummy))]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum ScheduleAmount {
    Fixed(i64),
    Between { num1: i64, num2: i64 },
}

impl ScheduleAmount {
    /// The best single-value estimate for this amount: the fixed value, or
    /// the midpoint of the range for `isbetween`.
    pub fn estimate(&self) -> i64 {
        match self {
            ScheduleAmount::Fixed(v) => *v,
            ScheduleAmount::Between { num1, num2 } => (num1 + num2) / 2,
        }
    }
}

#[cfg_attr(any(feature = "testutils", test), derive(fake::Dummy))]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Schedule {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub payee_id: Option<String>,
    #[serde(default)]
    pub account_id: Option<String>,
    pub amount: ScheduleAmount,
    pub amount_op: String,
    pub date: ScheduleDate,
    #[serde(default)]
    pub next_date: Option<String>,
    #[serde(default)]
    pub completed: bool,
    #[serde(default)]
    pub posts_transaction: bool,
}

#[cfg_attr(any(feature = "testutils", test), derive(fake::Dummy))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulesResponse {
    pub schedules: Vec<Schedule>,
}

#[cfg_attr(any(feature = "testutils", test), derive(fake::Dummy))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PayeeTotal {
    pub payee_id: String,
    pub total: i64,
}

#[cfg_attr(any(feature = "testutils", test), derive(fake::Dummy))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayeeTotalsResponse {
    pub totals: Vec<PayeeTotal>,
}

#[cfg_attr(any(feature = "testutils", test), derive(fake::Dummy))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryProjectionResponse {
    /// `None` if the category has no Budget Automation goal defined
    /// (`goal_def` absent) - callers should fall back to `budgeted` from
    /// [`BudgetMonthResponse`] in that case.
    pub budgeted: Option<i64>,
}
