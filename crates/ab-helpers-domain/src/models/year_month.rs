use std::fmt;
use std::str::FromStr;

use chrono::{Datelike, Days, Local, Months, NaiveDate};

/// A calendar year and month, with no day component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct YearMonth {
    year: u32,
    month: u32,
}

impl YearMonth {
    pub fn new(year: u32, month: u32) -> Result<Self, YearMonthError> {
        if (1..=12).contains(&month) {
            Ok(Self { year, month })
        } else {
            Err(YearMonthError::InvalidMonth(month))
        }
    }

    pub fn current() -> Self {
        let today = Local::now().date_naive();
        Self {
            year: today.year() as u32,
            month: today.month(),
        }
    }

    pub fn year(self) -> u32 {
        self.year
    }

    pub fn month(self) -> u32 {
        self.month
    }

    /// The first day of the month.
    pub fn start_date(self) -> NaiveDate {
        NaiveDate::from_ymd_opt(self.year as i32, self.month, 1)
            .expect("month is validated at construction")
    }

    /// The last day of the month.
    pub fn end_date(self) -> NaiveDate {
        self.start_date()
            .checked_add_months(Months::new(1))
            .and_then(|d| d.checked_sub_days(Days::new(1)))
            .expect("month is validated at construction")
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum YearMonthError {
    #[error("expected `YYYY-MM`, got `{0}`")]
    InvalidFormat(String),
    #[error("month must be between 1 and 12, got {0}")]
    InvalidMonth(u32),
}

impl FromStr for YearMonth {
    type Err = YearMonthError;

    /// Parses `"YYYY-MM"`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (year_str, month_str) = s
            .split_once('-')
            .ok_or_else(|| YearMonthError::InvalidFormat(s.to_owned()))?;
        let year: u32 = year_str
            .parse()
            .map_err(|_| YearMonthError::InvalidFormat(s.to_owned()))?;
        let month: u32 = month_str
            .parse()
            .map_err(|_| YearMonthError::InvalidFormat(s.to_owned()))?;
        Self::new(year, month)
    }
}

impl fmt::Display for YearMonth {
    /// Formats as `"YYYY-MM"`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}", self.year, self.month)
    }
}
