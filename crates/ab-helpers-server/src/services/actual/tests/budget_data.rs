use ab_helpers_domain::{ExpenseCategory, Money};

use super::super::budget_data::{is_income_addback, unmatched_addback_categories};

fn category(id: &str, name: &str) -> ExpenseCategory {
    ExpenseCategory {
        id: id.to_string(),
        name: name.to_string(),
        projected_amount: Money::from_cents(0),
        group_name: "group".to_string(),
    }
}

#[test]
fn is_income_addback_matches_case_insensitive_but_exact() {
    let addback = vec!["Assurance Santé".to_string()];

    assert!(is_income_addback(
        &addback,
        &category("cat-addback", "assurance santé")
    ));
    assert!(!is_income_addback(
        &addback,
        &category("cat-dentaire", "Assurance Santé Dentaire")
    ));
}

#[test]
fn is_income_addback_ignores_leading_emoji() {
    let addback = vec!["Assurance Santé".to_string()];

    assert!(is_income_addback(
        &addback,
        &category("cat-addback", "🏥 Assurance Santé")
    ));
}

#[test]
fn unmatched_addback_categories_reports_entries_with_no_match() {
    let addback = vec!["Assurance Santé".to_string(), "Assurance Vie".to_string()];
    let categories = vec![category("cat-addback", "🏥 Assurance Santé")];

    assert_eq!(
        unmatched_addback_categories(&addback, &categories),
        vec!["Assurance Vie".to_string()]
    );
}
