use std::fmt;
use std::num::ParseIntError;
use std::ops::{Add, Neg, Sub};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Monetary amount stored as signed integer cents.
///
/// Actual Budget represents money as integer cents internally and so do we, to
/// avoid floating-point drift when reconciling balances.
#[cfg_attr(any(feature = "testutils", test), derive(fake::Dummy))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Money(i64);

impl Money {
    pub const ZERO: Money = Money(0);

    pub const fn from_cents(cents: i64) -> Self {
        Self(cents)
    }

    pub const fn cents(self) -> i64 {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// Returns the value formatted with an explicit `+` sign for positive amounts.
    pub fn signed_str(self) -> String {
        if self.cents() > 0 {
            format!("+{self}")
        } else {
            format!("{self}")
        }
    }
}

impl Add for Money {
    type Output = Money;
    fn add(self, rhs: Money) -> Money {
        Money::from_cents(self.cents() + rhs.cents())
    }
}

impl Sub for Money {
    type Output = Money;
    fn sub(self, rhs: Money) -> Money {
        Money::from_cents(self.cents() - rhs.cents())
    }
}

impl Neg for Money {
    type Output = Money;
    fn neg(self) -> Money {
        Money::from_cents(-self.cents())
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ParseMoneyError {
    #[error("empty input")]
    Empty,
    #[error("more than one decimal separator")]
    MultipleDecimalPoints,
    #[error("more than two fractional digits")]
    TooManyFractionalDigits,
    #[error("invalid digit: {0}")]
    InvalidDigit(#[from] ParseIntError),
}

impl FromStr for Money {
    type Err = ParseMoneyError;

    /// Parses values like `"12"`, `"12.3"`, `"12.34"`, `"-5.05"`, `"+0"`.
    /// Allows up to two fractional digits.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if s.is_empty() {
            return Err(ParseMoneyError::Empty);
        }
        let (sign, rest) = if let Some(r) = s.strip_prefix('-') {
            (-1i64, r)
        } else if let Some(r) = s.strip_prefix('+') {
            (1i64, r)
        } else {
            (1i64, s)
        };
        if rest.is_empty() {
            return Err(ParseMoneyError::Empty);
        }

        let mut parts = rest.splitn(3, '.');
        let whole_str = parts.next().unwrap_or("0");
        let frac_str = parts.next();
        if parts.next().is_some() {
            return Err(ParseMoneyError::MultipleDecimalPoints);
        }

        let whole: i64 = if whole_str.is_empty() {
            0
        } else {
            whole_str.parse()?
        };

        let frac_cents: i64 = match frac_str {
            None => 0,
            Some("") => 0,
            Some(f) if f.len() > 2 => return Err(ParseMoneyError::TooManyFractionalDigits),
            Some(f) => {
                let parsed: i64 = f.parse()?;
                if f.len() == 1 { parsed * 10 } else { parsed }
            }
        };

        Ok(Money::from_cents(sign * (whole * 100 + frac_cents)))
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let cents = self.cents();
        let sign = if cents < 0 { "-" } else { "" };
        let abs = cents.unsigned_abs();
        let dollars = abs / 100;
        let frac = abs % 100;
        write!(f, "{sign}{dollars}.{frac:02}")
    }
}
