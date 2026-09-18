use chrono::NaiveDate;

use super::money::Money;

/// Reasons why an interest run is skipped without performing any writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterestSkip {
    AccountClosed,
    NoInterest {
        balance: Money,
        cutoff: NaiveDate,
    },
    AlreadyApplied {
        payee_name: String,
        date: NaiveDate,
        amount: Money,
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
    /// the day before the last transaction.
    pub fn cutoff_for(&self, last_tx_date: NaiveDate) -> NaiveDate {
        last_tx_date - chrono::Duration::days(1)
    }

    /// French period label used in interest notes.
    pub fn notes_label(&self) -> &'static str {
        match self {
            InterestPeriod::Weekly => "semaine",
            InterestPeriod::Monthly => "mois",
        }
    }
}

/// Parses the `interestRate:<value>` token out of an Actual account note
///
/// e.g. `"...interestRate:0.0699 ..."` -> `Some(0.0699)`
pub fn parse_interest_rate(note: &str) -> Option<f64> {
    let after = note.split("interestRate:").nth(1)?;
    let token = after.split(char::is_whitespace).next()?;
    token.parse::<f64>().ok()
}

/// Formats a rate as a percentage with up to 2 decimals, dropping trailing
/// zeros
///
/// e.g. `0.0699` -> `"6.99%"`, `0.05` -> `"5%"`, `0.069` -> `"6.9%"`
pub fn format_percent(rate: f64) -> String {
    let rounded = (rate * 100.0 * 100.0).round() / 100.0;
    let s = format!("{rounded:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');

    format!("{s}%")
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

#[derive(Debug)]
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
