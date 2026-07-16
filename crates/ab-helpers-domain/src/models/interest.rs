use chrono::Months;
use chrono::NaiveDate;

use super::money::Money;

/// Reasons why an interest run is skipped without performing any writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterestSkip {
    AccountClosed,
    NoInterest { balance: Money, cutoff: NaiveDate },
}

/// Outcome of a dry-run interest execution.
#[derive(Debug)]
pub enum DryRunOutcome {
    Skip(InterestSkip),
    WouldApply {
        last_tx_date: NaiveDate,
        cutoff: NaiveDate,
        balance: Money,
        interest: Money,
        new_balance: Money,
        notes: String,
    },
}

/// Outcome of a live (write) interest execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveOutcome {
    Skip(InterestSkip),
    Applied {
        balance: Money,
        interest: Money,
        new_balance: Money,
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
    pub interest: Money,
    pub principal: Money,
    pub new_balance: Money,
}

pub fn apply_bank_payment(
    previous_balance: Money,
    payment: Money,
    rate: f64,
    round: bool,
) -> BankPaymentResult {
    let abs_prev = previous_balance.cents().unsigned_abs() as f64;
    let interest_abs = if round {
        (abs_prev * rate).round() as i64
    } else {
        (abs_prev * rate).floor() as i64
    };

    let prev = previous_balance.cents();
    let pay = payment.cents();

    let new_balance_cents = if prev >= 0 {
        prev + interest_abs - pay
    } else {
        prev - interest_abs + pay
    };

    let interest_signed = if prev < 0 {
        -interest_abs
    } else {
        interest_abs
    };
    let principal_abs = prev.unsigned_abs() as i64 - new_balance_cents.unsigned_abs() as i64;

    BankPaymentResult {
        interest: Money::from_cents(interest_signed),
        principal: Money::from_cents(principal_abs),
        new_balance: Money::from_cents(new_balance_cents),
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
    pub balance: Money,
    pub interest: Money,
    pub new_balance: Money,
    pub notes: String,
    pub payee_name: String,
}
