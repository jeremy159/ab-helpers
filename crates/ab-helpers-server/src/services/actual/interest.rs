use chrono::Datelike;

pub enum InterestPeriod {
    Weekly,
    Monthly,
}

pub struct InterestConfig {
    pub account_id: String,
    pub rate: f64,
    pub payee_name: String,
    pub round: bool,
    pub period: InterestPeriod,
}

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

use std::sync::Arc;
use async_trait::async_trait;
use ab_helpers_domain::InterestOutcome;
use crate::error::{AppError, BudgetizeResult};

pub struct InterestService<C> {
    client: Arc<C>,
    config: InterestConfig,
}

impl<C> InterestService<C> {
    pub fn new(client: Arc<C>, config: InterestConfig) -> Self {
        Self { client, config }
    }
}

trait ActualClientBound:
    actual::AccountRequests + actual::TransactionRequests + Send + Sync {}

impl<T> ActualClientBound for T where
    T: actual::AccountRequests + actual::TransactionRequests + Send + Sync {}

impl<C: ActualClientBound + 'static> InterestService<C> {
    pub async fn apply(&self) -> BudgetizeResult<InterestOutcome> {
        // 1. Find account by ID
        let accounts = self.client.list_accounts().await.map_err(AppError::from_actual)?;
        let account = accounts
            .iter()
            .find(|a| a.id == self.config.account_id)
            .ok_or_else(|| AppError::ActualAccountNotFound(self.config.account_id.clone()))?;

        // 2. Closed guard
        if account.closed {
            tracing::warn!(
                account_id = %account.id,
                "account is closed, skipping interest run"
            );
            return Ok(InterestOutcome::AccountClosed);
        }

        // 3. Last transaction
        let last_tx = self.client
            .get_last_transaction(&account.id)
            .await
            .map_err(AppError::from_actual)?;

        // 4. Cutoff date
        let cutoff = match self.config.period {
            InterestPeriod::Weekly => last_tx.date - chrono::Duration::days(1),
            InterestPeriod::Monthly => mortgage_cutoff(last_tx.date),
        };

        // 5. Balance at cutoff
        let balance = self.client
            .get_balance_at(&account.id, cutoff)
            .await
            .map_err(AppError::from_actual)?;

        // 6. Compute interest
        let result = apply_bank_payment(balance, last_tx.amount, self.config.rate, self.config.round);

        if result.interest == 0 {
            return Ok(InterestOutcome::NoInterest { balance });
        }

        // 7. Notes string with formatted rate
        let rate_pct = format!("{:.2}%", self.config.rate * 100.0);
        let period_label = match self.config.period {
            InterestPeriod::Weekly => "semaine",
            InterestPeriod::Monthly => "mois",
        };
        let notes = format!("Intérêt pour 1 {period_label} à {rate_pct}");

        // 8. Ensure payee
        let payee_id = self.client
            .ensure_payee(&self.config.payee_name)
            .await
            .map_err(AppError::from_actual)?;

        // 9. Import transaction
        let import_tx = actual::ImportTransaction {
            account_id: account.id.clone(),
            date: last_tx.date,
            payee_id,
            amount: result.interest,
            notes: Some(notes),
            cleared: Some(true),
        };
        let transaction_id = self.client
            .import_transaction(import_tx)
            .await
            .map_err(AppError::from_actual)?;

        Ok(InterestOutcome::Applied {
            balance,
            interest: result.interest,
            new_balance: result.new_balance,
            transaction_id,
        })
    }
}

#[cfg(test)]
mod service_tests {
    use super::*;
    use std::sync::Arc;
    use async_trait::async_trait;
    use chrono::NaiveDate;
    use actual::{
        Account, ActualResult, AddTransactionResponse, ImportTransaction,
        LastTransaction, SaveTransaction,
    };

    struct FakeClient {
        accounts: Vec<Account>,
        last_tx: LastTransaction,
        balance: i64,
        payee_id: String,
        imported_tx: std::sync::Mutex<Option<ImportTransaction>>,
    }

    #[async_trait]
    impl actual::AccountRequests for FakeClient {
        async fn list_accounts(&self) -> ActualResult<Vec<Account>> {
            Ok(self.accounts.clone())
        }
        async fn get_account_balance(&self, _id: &str) -> ActualResult<i64> {
            Ok(self.balance)
        }
        async fn get_last_transaction(&self, _id: &str) -> ActualResult<LastTransaction> {
            Ok(self.last_tx.clone())
        }
        async fn ensure_payee(&self, _name: &str) -> ActualResult<String> {
            Ok(self.payee_id.clone())
        }
    }

    #[async_trait]
    impl actual::TransactionRequests for FakeClient {
        async fn add_transaction(&self, _tx: SaveTransaction) -> ActualResult<AddTransactionResponse> {
            Ok(AddTransactionResponse { id: "ignored".into() })
        }
        async fn get_balance_at(&self, _id: &str, _date: NaiveDate) -> ActualResult<i64> {
            Ok(self.balance)
        }
        async fn import_transaction(&self, tx: ImportTransaction) -> ActualResult<String> {
            *self.imported_tx.lock().unwrap() = Some(tx);
            Ok("tx-interest-1".into())
        }
    }

    fn make_account(id: &str, closed: bool) -> Account {
        Account { id: id.into(), name: "Test Loan".into(), offbudget: false, closed }
    }

    fn make_client(closed: bool) -> Arc<FakeClient> {
        Arc::new(FakeClient {
            accounts: vec![make_account("acc-1", closed)],
            last_tx: LastTransaction {
                date: NaiveDate::from_ymd_opt(2024, 5, 18).unwrap(),
                amount: 10000,
            },
            balance: -50000,
            payee_id: "payee-1".into(),
            imported_tx: Default::default(),
        })
    }

    fn kia_config() -> InterestConfig {
        InterestConfig {
            account_id: "acc-1".into(),
            rate: 0.00133978648017598,
            payee_name: "Loan Interest".into(),
            round: false,
            period: InterestPeriod::Weekly,
        }
    }

    #[tokio::test]
    async fn returns_account_closed_when_closed() {
        let svc = InterestService::new(make_client(true), kia_config());
        let outcome = svc.apply().await.unwrap();
        assert!(matches!(outcome, InterestOutcome::AccountClosed));
    }

    #[tokio::test]
    async fn applies_interest_and_imports_transaction() {
        let client = make_client(false);
        let svc = InterestService::new(client.clone(), kia_config());
        let outcome = svc.apply().await.unwrap();

        match outcome {
            InterestOutcome::Applied { interest, transaction_id, .. } => {
                assert_eq!(interest, -66); // floor(50000 * 0.00133978...) = 66, signed negative
                assert_eq!(transaction_id, "tx-interest-1");
            }
            other => panic!("unexpected: {other:?}"),
        }

        let tx = client.imported_tx.lock().unwrap().clone().expect("tx imported");
        assert_eq!(tx.account_id, "acc-1");
        assert_eq!(tx.payee_id, "payee-1");
        assert_eq!(tx.cleared, Some(true));
        assert!(tx.notes.as_deref().unwrap_or("").contains("semaine"));
    }

    #[tokio::test]
    async fn returns_no_interest_when_zero() {
        let client = Arc::new(FakeClient {
            accounts: vec![make_account("acc-1", false)],
            last_tx: LastTransaction {
                date: NaiveDate::from_ymd_opt(2024, 5, 18).unwrap(),
                amount: 0,
            },
            balance: 0, // zero balance → zero interest
            payee_id: "p".into(),
            imported_tx: Default::default(),
        });
        let svc = InterestService::new(client, kia_config());
        let outcome = svc.apply().await.unwrap();
        assert!(matches!(outcome, InterestOutcome::NoInterest { .. }));
    }
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
