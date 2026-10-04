use chrono::NaiveDate;

use crate::{YearMonth, YearMonthError};

#[test]
fn parses_year_month() {
    let ym: YearMonth = "2026-03".parse().unwrap();
    assert_eq!(ym.year(), 2026);
    assert_eq!(ym.month(), 3);
}

#[test]
fn rejects_malformed_input() {
    assert!(matches!(
        "2026-03-01".parse::<YearMonth>(),
        Err(YearMonthError::InvalidFormat(_))
    ));
    assert!(matches!(
        "2026".parse::<YearMonth>(),
        Err(YearMonthError::InvalidFormat(_))
    ));
}

#[test]
fn rejects_out_of_range_month() {
    assert!(matches!(
        "2026-13".parse::<YearMonth>(),
        Err(YearMonthError::InvalidMonth(13))
    ));
    assert!(matches!(
        "2026-00".parse::<YearMonth>(),
        Err(YearMonthError::InvalidMonth(0))
    ));
}

#[test]
fn formats_as_year_dash_month() {
    assert_eq!(YearMonth::new(2026, 3).unwrap().to_string(), "2026-03");
}

#[test]
fn start_and_end_date_for_ordinary_month() {
    let ym = YearMonth::new(2026, 3).unwrap();
    assert_eq!(ym.start_date(), NaiveDate::from_ymd_opt(2026, 3, 1).unwrap());
    assert_eq!(ym.end_date(), NaiveDate::from_ymd_opt(2026, 3, 31).unwrap());
}

#[test]
fn end_date_wraps_into_next_year_for_december() {
    let ym = YearMonth::new(2026, 12).unwrap();
    assert_eq!(ym.end_date(), NaiveDate::from_ymd_opt(2026, 12, 31).unwrap());
}

#[test]
fn end_date_handles_leap_february() {
    let ym = YearMonth::new(2024, 2).unwrap();
    assert_eq!(ym.end_date(), NaiveDate::from_ymd_opt(2024, 2, 29).unwrap());
}
