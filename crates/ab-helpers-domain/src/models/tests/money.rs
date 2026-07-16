use crate::{Money, ParseMoneyError};

#[test]
fn parses_whole_amount() {
    assert_eq!("12".parse::<Money>().unwrap(), Money::from_cents(1200));
}

#[test]
fn parses_one_decimal() {
    assert_eq!("12.3".parse::<Money>().unwrap(), Money::from_cents(1230));
}

#[test]
fn parses_two_decimals() {
    assert_eq!("12.34".parse::<Money>().unwrap(), Money::from_cents(1234));
}

#[test]
fn parses_negative() {
    assert_eq!("-5.05".parse::<Money>().unwrap(), Money::from_cents(-505));
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
    assert_eq!(Money::from_cents(1234).to_string(), "12.34");
}

#[test]
fn formats_negative() {
    assert_eq!(Money::from_cents(-505).to_string(), "-5.05");
}

#[test]
fn formats_padding_zero() {
    assert_eq!(Money::from_cents(105).to_string(), "1.05");
    assert_eq!(Money::from_cents(100).to_string(), "1.00");
}
