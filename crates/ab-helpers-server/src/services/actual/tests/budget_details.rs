use std::sync::Arc;

use ab_helpers_domain::{BudgetMode, YearMonth};
use actual::{Account, ActualResult, Category, CategoryGroup, Payee, PayeeTotal};
use async_trait::async_trait;
use chrono::NaiveDate;

use crate::config::PersonSettings;

use super::super::budget_details::BudgetDetailsService;

struct FakeClient {
    payees: Vec<Payee>,
    payee_totals: Vec<PayeeTotal>,
    category_groups: Vec<CategoryGroup>,
}

#[async_trait]
impl actual::ActualReadRequests for FakeClient {
    async fn list_accounts(&self) -> ActualResult<Vec<Account>> {
        unimplemented!("not needed for budget-details tests")
    }
    async fn get_account_balance(&self, _id: &str) -> ActualResult<i64> {
        unimplemented!("not needed for budget-details tests")
    }
    async fn get_last_transaction(&self, _id: &str) -> ActualResult<actual::LastTransaction> {
        unimplemented!("not needed for budget-details tests")
    }
    async fn get_balance_at(&self, _id: &str, _date: NaiveDate) -> ActualResult<i64> {
        unimplemented!("not needed for budget-details tests")
    }
    async fn get_account_note(&self, _id: &str) -> ActualResult<Option<String>> {
        unimplemented!("not needed for budget-details tests")
    }
    async fn find_transaction(
        &self,
        _account_id: &str,
        _date: NaiveDate,
        _payee_name: &str,
    ) -> ActualResult<Option<actual::ExistingTransaction>> {
        unimplemented!("not needed for budget-details tests")
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
        _category_id: &str,
        _month: &str,
    ) -> ActualResult<Option<i64>> {
        Ok(None)
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
    })
}

#[tokio::test]
async fn build_details_includes_addback_row_and_adjusts_income_denominator() {
    let client = make_client();
    let svc = BudgetDetailsService::new(
        client,
        person_settings(),
        vec!["Assurance Santé".into()],
        true,
    );

    let (report, _unmatched) = svc
        .build_details(month(), BudgetMode::Actual)
        .await
        .unwrap();
    let details = report.details;

    // Adjusted income denominator: salary (500000) + addback (10000).
    let adjusted_total = 510_000f64;

    // "Obligations - Maison" group splits into top "Obligations" / sub
    // "Maison".
    assert_eq!(details.groups.len(), 1);
    let top = &details.groups[0];
    assert_eq!(top.name, "Obligations");
    assert_eq!(top.sub_groups.len(), 1);
    let sub = &top.sub_groups[0];
    assert_eq!(sub.name.as_deref(), Some("Maison"));
    assert_eq!(sub.rows.len(), 2);

    let addback_row = sub
        .rows
        .iter()
        .find(|r| r.name == "Assurance Santé")
        .expect("addback category present as a row");
    assert_eq!(
        addback_row.amount,
        ab_helpers_domain::Money::from_cents(10_000)
    );
    assert!((addback_row.percentage - 10_000f64 / adjusted_total).abs() < 1e-9);

    let common_row = sub
        .rows
        .iter()
        .find(|r| r.name == "Loyer")
        .expect("ordinary category present as a row");
    assert_eq!(
        common_row.amount,
        ab_helpers_domain::Money::from_cents(20_000)
    );
    // The percentage must reflect salary + addback, not just salary.
    assert!((common_row.percentage - 20_000f64 / adjusted_total).abs() < 1e-9);
}

/// Same as [`make_client`], but the "Assurance Santé" add-back category sits
/// in an income group (a paycheque's own split destination), alongside a
/// deposit category that must stay excluded.
fn make_client_with_addback_in_income_group() -> Arc<FakeClient> {
    Arc::new(FakeClient {
        payees: vec![make_payee("payee-alice", "Alice Salary")],
        payee_totals: vec![PayeeTotal {
            payee_id: "payee-alice".into(),
            total: 500_000,
        }],
        category_groups: vec![
            CategoryGroup {
                id: "group-income".into(),
                name: "Income".into(),
                is_income: true,
                hidden: false,
                categories: vec![
                    make_category("cat-addback", "Assurance Santé", 10_000),
                    make_category("cat-deposit", "Dépôt direct", 490_000),
                ],
            },
            make_group(
                "group-1",
                "Obligations - Maison",
                vec![make_category("cat-common", "Loyer", 20_000)],
            ),
        ],
    })
}

#[tokio::test]
async fn build_details_includes_addback_category_from_an_income_group() {
    let client = make_client_with_addback_in_income_group();
    let svc = BudgetDetailsService::new(
        client,
        person_settings(),
        vec!["Assurance Santé".into()],
        true,
    );

    let (report, unmatched) = svc
        .build_details(month(), BudgetMode::Actual)
        .await
        .unwrap();

    assert!(unmatched.is_empty());
    // Adjusted income denominator: salary (500000) + addback (10000).
    assert_eq!(
        report.income_with_addback,
        ab_helpers_domain::Money::from_cents(510_000)
    );

    let addback_row = report
        .details
        .groups
        .iter()
        .flat_map(|g| &g.sub_groups)
        .flat_map(|s| &s.rows)
        .find(|r| r.name == "Assurance Santé")
        .expect("addback category pulled in despite living in an income group");
    assert_eq!(
        addback_row.amount,
        ab_helpers_domain::Money::from_cents(10_000)
    );

    // The deposit category is not a configured addback, so it must stay out
    // of the breakdown entirely.
    let has_deposit_row = report
        .details
        .groups
        .iter()
        .flat_map(|g| &g.sub_groups)
        .flat_map(|s| &s.rows)
        .any(|r| r.name == "Dépôt direct");
    assert!(!has_deposit_row);
}

#[tokio::test]
async fn build_details_returns_configured_addback_categories_that_matched_nothing() {
    let client = make_client();
    let svc = BudgetDetailsService::new(
        client,
        person_settings(),
        vec!["Assurance Santé".into(), "Assurance Vie".into()],
        true,
    );

    let (_report, unmatched) = svc
        .build_details(month(), BudgetMode::Actual)
        .await
        .unwrap();

    // "Assurance Santé" matches `cat-addback` above; "Assurance Vie" matches
    // no fetched category and must come back as unmatched.
    assert_eq!(unmatched, vec!["Assurance Vie".to_string()]);
}
