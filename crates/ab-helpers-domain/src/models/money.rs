use std::fmt;
use std::num::ParseIntError;
use std::ops::{Add, Neg, Sub};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Monetary amount stored as signed integer cents.
///
/// Actual Budget represents money as integer cents internally and so do we, to
/// avoid floating-point drift when reconciling balances.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Money(pub i64);

impl Money {
    pub const ZERO: Money = Money(0);

    pub fn from_cents(cents: i64) -> Self {
        Self(cents)
    }

    pub fn cents(self) -> i64 {
        self.0
    }

    pub fn is_zero(self) -> bool {
        self.0 == 0
    }
}

impl Add for Money {
    type Output = Money;
    fn add(self, rhs: Money) -> Money {
        Money(self.0 + rhs.0)
    }
}

impl Sub for Money {
    type Output = Money;
    fn sub(self, rhs: Money) -> Money {
        Money(self.0 - rhs.0)
    }
}

impl Neg for Money {
    type Output = Money;
    fn neg(self) -> Money {
        Money(-self.0)
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
        let (sign, rest) = match s.as_bytes()[0] {
            b'-' => (-1i64, &s[1..]),
            b'+' => (1i64, &s[1..]),
            _ => (1i64, s),
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

        let whole: i64 = if whole_str.is_empty() { 0 } else { whole_str.parse()? };

        let frac_cents: i64 = match frac_str {
            None => 0,
            Some(f) if f.is_empty() => 0,
            Some(f) if f.len() > 2 => return Err(ParseMoneyError::TooManyFractionalDigits),
            Some(f) => {
                let parsed: i64 = f.parse()?;
                if f.len() == 1 { parsed * 10 } else { parsed }
            }
        };

        Ok(Money(sign * (whole * 100 + frac_cents)))
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let cents = self.0;
        let sign = if cents < 0 { "-" } else { "" };
        let abs = cents.unsigned_abs();
        let dollars = abs / 100;
        let frac = abs % 100;
        write!(f, "{sign}{dollars}.{frac:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_whole_amount() {
        assert_eq!("12".parse::<Money>().unwrap(), Money(1200));
    }

    #[test]
    fn parses_one_decimal() {
        assert_eq!("12.3".parse::<Money>().unwrap(), Money(1230));
    }

    #[test]
    fn parses_two_decimals() {
        assert_eq!("12.34".parse::<Money>().unwrap(), Money(1234));
    }

    #[test]
    fn parses_negative() {
        assert_eq!("-5.05".parse::<Money>().unwrap(), Money(-505));
    }

    #[test]
    fn rejects_three_decimals() {
        assert!(matches!(
            "12.345".parse::<Money>(),
            Err(ParseMoneyError::TooManyFractionalDigits)
        ));
    }

    #[test]
    fn rejects_two_dots() {
        assert!(matches!(
            "12.3.4".parse::<Money>(),
            Err(ParseMoneyError::MultipleDecimalPoints)
        ));
    }

    #[test]
    fn rejects_empty() {
        assert_eq!("".parse::<Money>(), Err(ParseMoneyError::Empty));
    }

    #[test]
    fn formats_positive() {
        assert_eq!(Money(1234).to_string(), "12.34");
    }

    #[test]
    fn formats_negative() {
        assert_eq!(Money(-505).to_string(), "-5.05");
    }

    #[test]
    fn formats_padding_zero() {
        assert_eq!(Money(105).to_string(), "1.05");
        assert_eq!(Money(100).to_string(), "1.00");
    }
}
