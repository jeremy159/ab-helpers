use chrono::Datelike;

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

    let interest_signed = if previous_balance < 0 { -interest_abs } else { interest_abs };
    let principal = previous_balance.unsigned_abs() as i64
        - new_balance.unsigned_abs() as i64;

    BankPaymentResult {
        interest: interest_signed,
        principal,
        new_balance,
    }
}

/// Replicate JS mortgage cutoff: setDate(getMonth()-1); setDate(getDate()-1).
pub fn mortgage_cutoff(last_tx_date: chrono::NaiveDate) -> chrono::NaiveDate {
    let month0 = last_tx_date.month0() as i64; // 0-indexed (Jan=0)
    let year = last_tx_date.year();
    let month = last_tx_date.month();

    // JS: cutoff.setDate(cutoff.getMonth() - 1)
    let step1 = set_day_js(year, month, month0 - 1);

    // JS: cutoff.setDate(cutoff.getDate() - 1)
    set_day_js(step1.year(), step1.month(), step1.day() as i64 - 1)
}

fn set_day_js(year: i32, month: u32, day: i64) -> chrono::NaiveDate {
    chrono::NaiveDate::from_ymd_opt(year, month, 1).unwrap()
        + chrono::Duration::days(day - 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    // apply_bank_payment tests

    #[test]
    fn interest_rounds_when_round_true() {
        // balance=-50000 (owe $500), rate=0.00133978648017598, round=true
        // abs_prev=50000, interest=round(50000 * 0.001339786)=round(66.989)=67
        let r = apply_bank_payment(-50000, 10000, 0.00133978648017598, true);
        assert_eq!(r.interest, -67);
    }

    #[test]
    fn interest_floors_when_round_false() {
        // Same as above but floor: floor(66.989)=66
        let r = apply_bank_payment(-50000, 10000, 0.00133978648017598, false);
        assert_eq!(r.interest, -66);
    }

    #[test]
    fn new_balance_negative_account() {
        // prev=-50000, interest_abs=67(rounded), payment=10000
        // new_balance = -50000 - 67 + 10000 = -40067
        let r = apply_bank_payment(-50000, 10000, 0.00133978648017598, true);
        assert_eq!(r.new_balance, -40067);
    }

    #[test]
    fn new_balance_positive_account() {
        // prev=50000 (asset), payment=0, interest=67
        // new_balance = 50000 + 67 - 0 = 50067
        let r = apply_bank_payment(50000, 0, 0.00133978648017598, true);
        assert_eq!(r.interest, 67);
        assert_eq!(r.new_balance, 50067);
    }

    #[test]
    fn zero_interest_when_zero_balance() {
        let r = apply_bank_payment(0, 0, 0.00133978648017598, true);
        assert_eq!(r.interest, 0);
        assert_eq!(r.new_balance, 0);
    }

    // mortgage_cutoff tests

    #[test]
    fn mortgage_cutoff_may() {
        // May 18: month0=4, step1=setDay(4-1)=May 3, step2=setDay(3-1)=May 2
        let d = NaiveDate::from_ymd_opt(2024, 5, 18).unwrap();
        assert_eq!(mortgage_cutoff(d), NaiveDate::from_ymd_opt(2024, 5, 2).unwrap());
    }

    #[test]
    fn mortgage_cutoff_february() {
        // Feb 18: month0=1, step1=setDay(0)=Jan 31, step2=setDay(30)=Jan 30
        let d = NaiveDate::from_ymd_opt(2024, 2, 18).unwrap();
        assert_eq!(mortgage_cutoff(d), NaiveDate::from_ymd_opt(2024, 1, 30).unwrap());
    }

    #[test]
    fn mortgage_cutoff_january() {
        // Jan 18: month0=0, step1=setDay(-1)=Dec 30 2023, step2=setDay(29)=Dec 29 2023
        let d = NaiveDate::from_ymd_opt(2024, 1, 18).unwrap();
        assert_eq!(mortgage_cutoff(d), NaiveDate::from_ymd_opt(2023, 12, 29).unwrap());
    }
}
