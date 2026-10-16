use std::collections::HashMap;
use std::sync::Arc;

use ab_helpers_domain::{BudgetMode, BudgeterExt, YearMonth};
use actual::{Account, ActualResult, Category, CategoryGroup, Payee, PayeeTotal};
use async_trait::async_trait;
use chrono::NaiveDate;

use crate::config::PersonSettings;

use super::super::budget_summary::BudgetSummaryService;

#[derive(Default)]
struct FakeClient {
    payees: Vec<Payee>,
    payee_totals: Vec<PayeeTotal>,
    category_groups: Vec<CategoryGroup>,
    /// category_id -> goal_def dry-run projection, as Actual would return it.
    /// A category absent from this map has no goal_def (fetch returns None).
    category_projections: HashMap<String, i64>,
}

#[async_trait]
impl actual::ActualReadRequests for FakeClient {
    async fn list_accounts(&self) -> ActualResult<Vec<Account>> {
        unimplemented!("not needed for budget-summary tests")
    }
    async fn get_account_balance(&self, _id: &str) -> ActualResult<i64> {
        unimplemented!("not needed for budget-summary tests")
    }
    async fn get_last_transaction(&self, _id: &str) -> ActualResult<actual::LastTransaction> {
        unimplemented!("not needed for budget-summary tests")
    }
    async fn get_balance_at(&self, _id: &str, _date: NaiveDate) -> ActualResult<i64> {
        unimplemented!("not needed for budget-summary tests")
    }
    async fn get_account_note(&self, _id: &str) -> ActualResult<Option<String>> {
        unimplemented!("not needed for budget-summary tests")
    }
    async fn find_transaction(
        &self,
        _account_id: &str,
        _date: NaiveDate,
        _payee_name: &str,
    ) -> ActualResult<Option<actual::ExistingTransaction>> {
        unimplemented!("not needed for budget-summary tests")
    }
    async fn get_budget_month(&self, _month: &str) -> ActualResult<Vec<CategoryGroup>> {
        Ok(self.category_groups.clone())
    }
    async fn get_payees(&self) -> ActualResult<Vec<Payee>> {
        Ok(self.payees.clone())
    }
    async fn get_schedules(&self) -> ActualResult<Vec<actual::Schedule>> {
        Ok(Vec::new())
    }
    async fn get_payee_totals(
        &self,
        _payee_ids: &[String],
        _start_date: NaiveDate,
        _end_date: NaiveDate,
    ) -> ActualResult<Vec<PayeeTotal>> {
        Ok(self.payee_totals.clone())
    }
    async fn get_category_projection(
        &self,
        category_id: &str,
        _month: &str,
    ) -> ActualResult<Option<i64>> {
        Ok(self.category_projections.get(category_id).copied())
    }
}

fn make_payee(id: &str, name: &str) -> Payee {
    Payee {
        id: id.into(),
        name: name.into(),
    }
}

fn make_category(id: &str, name: &str, budgeted: i64) -> Category {
    Category {
        id: id.into(),
        name: name.into(),
        is_income: false,
        hidden: false,
        budgeted: Some(budgeted),
    }
}

fn make_group(id: &str, name: &str, categories: Vec<Category>) -> CategoryGroup {
    CategoryGroup {
        id: id.into(),
        name: name.into(),
        is_income: false,
        hidden: false,
        categories,
    }
}

fn month() -> YearMonth {
    YearMonth::new(2026, 1).unwrap()
}

fn person_settings() -> Vec<PersonSettings> {
    vec![PersonSettings {
        name: "Alice".into(),
        salary_payee: "Alice Salary".into(),
        allocation_payees: Vec::new(),
    }]
}

/// Salary of 500000 (5000.00) for Alice, one "Assurance Santé" add-back
/// category (10000, 100.00), and one ordinary common category (20000,
/// 200.00).
fn make_client() -> Arc<FakeClient> {
    Arc::new(FakeClient {
        payees: vec![make_payee("payee-alice", "Alice Salary")],
        payee_totals: vec![PayeeTotal {
            payee_id: "payee-alice".into(),
            total: 500_000,
        }],
        category_groups: vec![make_group(
            "group-1",
            "Obligations - Maison",
            vec![
                make_category("cat-addback", "Assurance Santé", 10_000),
                make_category("cat-common", "Loyer", 20_000),
            ],
        )],
        ..Default::default()
    })
}

#[tokio::test]
async fn build_excludes_addback_category_from_common_and_individual_totals() {
    let client = make_client();
    let svc = BudgetSummaryService::new(
        client,
        person_settings(),
        vec!["Assurance Santé".into()],
        true,
    );

    let (summary, _unmatched) = svc.build(month(), BudgetMode::Actual).await.unwrap();

    // Only the ordinary category (200.00) should count, never the add-back
    // category (100.00).
    assert_eq!(
        summary.total.common_expenses(),
        ab_helpers_domain::Money::from_cents(20_000)
    );
    assert_eq!(
        summary.total.individual_expenses(),
        ab_helpers_domain::Money::from_cents(0)
    );
    assert_eq!(
        summary.total.left_over(),
        ab_helpers_domain::Money::from_cents(500_000 - 20_000)
    );
}

#[tokio::test]
async fn build_returns_configured_addback_categories_that_matched_nothing() {
    let client = make_client();
    let svc = BudgetSummaryService::new(
        client,
        person_settings(),
        vec!["Assurance Santé".into(), "Assurance Vie".into()],
        true,
    );

    let (_summary, unmatched) = svc.build(month(), BudgetMode::Actual).await.unwrap();

    // "Assurance Santé" matches `cat-addback` above; "Assurance Vie" matches
    // no fetched category and must come back as unmatched.
    assert_eq!(unmatched, vec!["Assurance Vie".to_string()]);
}

#[tokio::test]
async fn build_addback_match_is_case_insensitive_but_exact() {
    let client = Arc::new(FakeClient {
        payees: vec![make_payee("payee-alice", "Alice Salary")],
        payee_totals: vec![PayeeTotal {
            payee_id: "payee-alice".into(),
            total: 500_000,
        }],
        category_groups: vec![make_group(
            "group-1",
            "Obligations - Maison",
            vec![
                // Exact match, different case - must be excluded.
                make_category("cat-addback", "assurance santé", 10_000),
                // Contains the add-back name as a substring but isn't an
                // exact match - must NOT be excluded.
                make_category("cat-dentaire", "Assurance Santé Dentaire", 5_000),
            ],
        )],
        ..Default::default()
    });
    let svc = BudgetSummaryService::new(
        client,
        person_settings(),
        vec!["Assurance Santé".into()],
        true,
    );

    let (summary, _unmatched) = svc.build(month(), BudgetMode::Actual).await.unwrap();

    assert_eq!(
        summary.total.common_expenses(),
        ab_helpers_domain::Money::from_cents(5_000)
    );
}

#[tokio::test]
async fn goal_def_projection_is_used_as_is_regardless_of_budget_mode() {
    // Actual's own goal/automation templates already smooth or annualize
    // whatever they're configured to; our code must trust that dry-run
    // projection as-is instead of re-averaging it across other months.
    let client = Arc::new(FakeClient {
        payees: vec![make_payee("payee-alice", "Alice Salary")],
        payee_totals: vec![PayeeTotal {
            payee_id: "payee-alice".into(),
            total: 500_000,
        }],
        category_groups: vec![make_group(
            "group-1",
            "Obligations - Maison",
            vec![make_category("cat-garderie", "Garderie", 999_999)],
        )],
        category_projections: HashMap::from([("cat-garderie".to_string(), 262_167)]),
    });
    let svc = BudgetSummaryService::new(client, person_settings(), vec![], false);

    let (actual_summary, _) = svc.build(month(), BudgetMode::Actual).await.unwrap();
    let (annualized_summary, _) = svc.build(month(), BudgetMode::Annualized).await.unwrap();

    assert_eq!(
        actual_summary.total.common_expenses(),
        ab_helpers_domain::Money::from_cents(262_167)
    );
    assert_eq!(
        annualized_summary.total.common_expenses(),
        ab_helpers_domain::Money::from_cents(262_167)
    );
}

#[tokio::test]
async fn category_without_projection_defaults_to_zero_when_fallback_disabled() {
    let client = Arc::new(FakeClient {
        payees: vec![make_payee("payee-alice", "Alice Salary")],
        payee_totals: vec![PayeeTotal {
            payee_id: "payee-alice".into(),
            total: 500_000,
        }],
        category_groups: vec![make_group(
            "group-1",
            "Obligations - Maison",
            vec![make_category("cat-no-template", "Loyer", 20_000)],
        )],
        ..Default::default()
    });
    let svc = BudgetSummaryService::new(client, person_settings(), vec![], false);

    let (summary, _) = svc.build(month(), BudgetMode::Actual).await.unwrap();

    assert_eq!(
        summary.total.common_expenses(),
        ab_helpers_domain::Money::ZERO
    );
}
