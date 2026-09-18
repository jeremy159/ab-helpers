use chrono::NaiveDate;

use crate::{InterestPeriod, Money, apply_bank_payment, format_percent, parse_interest_rate};

#[test]
fn interest_rounds_when_round_true() {
    // balance=-50000 (owe $500), rate=0.00133978648017598, round=true
    // abs_prev=50000, interest=round(50000 * 0.001339786)=round(66.989)=67
    let r = apply_bank_payment(
        Money::from_cents(-50000),
        Money::from_cents(10000),
        0.00133978648017598,
        true,
    );
    assert_eq!(r.interest, Money::from_cents(-67));
}

#[test]
fn interest_floors_when_round_false() {
    // Same as above but floor: floor(66.989)=66
    let r = apply_bank_payment(
        Money::from_cents(-50000),
        Money::from_cents(10000),
        0.00133978648017598,
        false,
    );
    assert_eq!(r.interest, Money::from_cents(-66));
}

#[test]
fn new_balance_negative_account() {
    // prev=-50000, interest_abs=67(rounded), payment=10000
    // new_balance = -50000 - 67 + 10000 = -40067
    let r = apply_bank_payment(
        Money::from_cents(-50000),
        Money::from_cents(10000),
        0.00133978648017598,
        true,
    );
    assert_eq!(r.new_balance, Money::from_cents(-40067));
}

#[test]
fn new_balance_positive_account() {
    // prev=50000 (asset), payment=0, interest=67
    // new_balance = 50000 + 67 - 0 = 50067
    let r = apply_bank_payment(
        Money::from_cents(50000),
        Money::from_cents(0),
        0.00133978648017598,
        true,
    );
    assert_eq!(r.interest, Money::from_cents(67));
    assert_eq!(r.new_balance, Money::from_cents(50067));
}

#[test]
fn zero_interest_when_zero_balance() {
    let r = apply_bank_payment(
        Money::from_cents(0),
        Money::from_cents(0),
        0.00133978648017598,
        true,
    );
    assert_eq!(r.interest, Money::ZERO);
    assert_eq!(r.new_balance, Money::ZERO);
}

// InterestPeriod::cutoff_for — cutoff = last_tx_date - 1 day, same for both periods.

#[test]
fn cutoff_for_weekly_is_one_day_before() {
    let d = NaiveDate::from_ymd_opt(2024, 5, 18).unwrap();
    assert_eq!(
        InterestPeriod::Weekly.cutoff_for(d),
        NaiveDate::from_ymd_opt(2024, 5, 17).unwrap()
    );
}

#[test]
fn cutoff_for_monthly_is_one_day_before() {
    let d = NaiveDate::from_ymd_opt(2024, 5, 18).unwrap();
    assert_eq!(
        InterestPeriod::Monthly.cutoff_for(d),
        NaiveDate::from_ymd_opt(2024, 5, 17).unwrap()
    );
}

#[test]
fn cutoff_for_monthly_crosses_month_boundary() {
    let d = NaiveDate::from_ymd_opt(2024, 5, 1).unwrap();
    assert_eq!(
        InterestPeriod::Monthly.cutoff_for(d),
        NaiveDate::from_ymd_opt(2024, 4, 30).unwrap()
    );
}

// parse_interest_rate

#[test]
fn parse_interest_rate_finds_token_mid_note() {
    assert_eq!(
        parse_interest_rate("Kia Carnival 2026\ninterestRate:0.0699\nother stuff"),
        Some(0.0699)
    );
}

#[test]
fn parse_interest_rate_finds_token_at_end() {
    assert_eq!(parse_interest_rate("interestRate:0.0429"), Some(0.0429));
}

#[test]
fn parse_interest_rate_missing_token_returns_none() {
    assert_eq!(parse_interest_rate("just a regular note"), None);
}

#[test]
fn parse_interest_rate_malformed_value_returns_none() {
    assert_eq!(parse_interest_rate("interestRate:not-a-number"), None);
}

// format_percent

#[test]
fn format_percent_trims_trailing_zeros() {
    assert_eq!(format_percent(0.0699), "6.99%");
    assert_eq!(format_percent(0.0429), "4.29%");
    assert_eq!(format_percent(0.05), "5%");
    assert_eq!(format_percent(0.069), "6.9%");
}
