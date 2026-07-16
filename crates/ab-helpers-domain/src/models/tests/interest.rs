use chrono::NaiveDate;

use crate::{Money, apply_bank_payment, mortgage_cutoff};

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

// mortgage_cutoff tests — cutoff = last_tx_date - 1 month - 1 day.

#[test]
fn mortgage_cutoff_typical() {
    // Mid-month: straightforward subtraction
    let d = NaiveDate::from_ymd_opt(2024, 5, 18).unwrap();
    assert_eq!(
        mortgage_cutoff(d),
        NaiveDate::from_ymd_opt(2024, 4, 17).unwrap()
    );
}

#[test]
fn mortgage_cutoff_january_crosses_year() {
    // January: goes back to December of the previous year
    let d = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
    assert_eq!(
        mortgage_cutoff(d),
        NaiveDate::from_ymd_opt(2023, 12, 14).unwrap()
    );
}

#[test]
fn mortgage_cutoff_end_of_march_leap_year() {
    // Mar 31 - 1 month clamps to Feb 29 (leap year), then -1 day = Feb 28
    let d = NaiveDate::from_ymd_opt(2024, 3, 31).unwrap();
    assert_eq!(
        mortgage_cutoff(d),
        NaiveDate::from_ymd_opt(2024, 2, 28).unwrap()
    );
}

#[test]
fn mortgage_cutoff_end_of_march_non_leap_year() {
    // Mar 31 - 1 month clamps to Feb 28 (non-leap year), then -1 day = Feb 27
    let d = NaiveDate::from_ymd_opt(2023, 3, 31).unwrap();
    assert_eq!(
        mortgage_cutoff(d),
        NaiveDate::from_ymd_opt(2023, 2, 27).unwrap()
    );
}
