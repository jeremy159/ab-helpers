use chrono::Months;
use chrono::NaiveDate;

/// Reasons why an interest run is skipped without performing any writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterestSkip {
    AccountClosed,
    NoInterest { balance: i64, cutoff: NaiveDate },
}

/// Outcome of a dry-run interest execution.
#[derive(Debug)]
pub enum DryRunOutcome {
    Skip(InterestSkip),
    WouldApply {
        last_tx_date: NaiveDate,
        cutoff: NaiveDate,
        balance: i64,
        interest: i64,
        new_balance: i64,
        notes: String,
    },
}

/// Outcome of a live (write) interest execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveOutcome {
    Skip(InterestSkip),
    Applied {
        balance: i64,
        interest: i64,
        new_balance: i64,
        transaction_id: String,
    },
}

/// Which period the interest calculation covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterestPeriod {
    Weekly,
    Monthly,
}

impl InterestPeriod {
    /// Returns the cutoff date: the point-in-time balance snapshot used as the interest base.
    /// Weekly: the day before the last transaction. Monthly: see [`mortgage_cutoff`].
    pub fn cutoff_for(&self, last_tx_date: NaiveDate) -> NaiveDate {
        match self {
            InterestPeriod::Weekly => last_tx_date - chrono::Duration::days(1),
            InterestPeriod::Monthly => mortgage_cutoff(last_tx_date),
        }
    }

    /// French period label used in interest notes.
    pub fn notes_label(&self) -> &'static str {
        match self {
            InterestPeriod::Weekly => "semaine",
            InterestPeriod::Monthly => "mois",
        }
    }
}

pub struct BankPaymentResult {
    pub interest: i64,
    pub principal: i64,
    pub new_balance: i64,
}

pub fn apply_bank_payment(
    previous_balance: i64,
    payment: i64,
    rate: f64,
    round: bool,
) -> BankPaymentResult {
    let abs_prev = previous_balance.unsigned_abs() as f64;
    let interest_abs = if round {
        (abs_prev * rate).round() as i64
    } else {
        (abs_prev * rate).floor() as i64
    };

    let new_balance = if previous_balance >= 0 {
        previous_balance + interest_abs - payment
    } else {
        previous_balance - interest_abs + payment
    };

    let interest_signed = if previous_balance < 0 {
        -interest_abs
    } else {
        interest_abs
    };
    let principal = previous_balance.unsigned_abs() as i64 - new_balance.unsigned_abs() as i64;

    BankPaymentResult {
        interest: interest_signed,
        principal,
        new_balance,
    }
}

/// Cutoff date for monthly mortgage interest: one month and one day before the last transaction.
/// End-of-month dates clamp to the last day of the target month (e.g. Mar 31 → Feb 28/29 → Feb 27/28).
pub fn mortgage_cutoff(last_tx_date: NaiveDate) -> NaiveDate {
    last_tx_date
        .checked_sub_months(Months::new(1))
        .expect("transaction date is too close to NaiveDate::MIN")
        .pred_opt()
        .expect("transaction date is too close to NaiveDate::MIN")
}

pub struct InterestPlan {
    pub account_id: String,
    pub last_tx_date: NaiveDate,
    pub cutoff: NaiveDate,
    pub balance: i64,
    pub interest: i64,
    pub new_balance: i64,
    pub notes: String,
    pub payee_name: String,
}
