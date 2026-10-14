use crate::{BudgetDetails, ExpenseCategory, Money};

fn category(group_name: &str, name: &str, amount_cents: i64) -> ExpenseCategory {
    ExpenseCategory {
        id: name.to_string(),
        name: name.to_string(),
        projected_amount: Money::from_cents(amount_cents),
        group_name: group_name.to_string(),
    }
}

#[test]
fn splits_group_name_on_first_dash() {
    let categories = vec![category("Obligations - Maison", "Hydro", 10_000)];

    let details = BudgetDetails::build(&categories, Money::from_cents(100_000));

    assert_eq!(details.groups.len(), 1);
    assert_eq!(details.groups[0].name, "Obligations");
    assert_eq!(details.groups[0].sub_groups.len(), 1);
    assert_eq!(
        details.groups[0].sub_groups[0].name,
        Some("Maison".to_string())
    );
    assert_eq!(details.groups[0].sub_groups[0].rows[0].name, "Hydro");
}

#[test]
fn different_sub_groups_under_same_top_group_nest_in_insertion_order() {
    let categories = vec![
        category("Obligations - Taxes", "Property tax", 5_000),
        category("Obligations - Maison", "Hydro", 10_000),
        category("Obligations - Taxes", "School tax", 2_000),
    ];

    let details = BudgetDetails::build(&categories, Money::from_cents(100_000));

    assert_eq!(details.groups.len(), 1);
    let top = &details.groups[0];
    assert_eq!(top.name, "Obligations");
    assert_eq!(top.sub_groups.len(), 2);
    assert_eq!(top.sub_groups[0].name, Some("Taxes".to_string()));
    assert_eq!(top.sub_groups[1].name, Some("Maison".to_string()));
}

#[test]
fn same_top_and_sub_group_lands_in_one_subgroup_preserving_input_order() {
    let categories = vec![
        category("Obligations - Maison", "Hydro", 10_000),
        category("Obligations - Maison", "Internet", 3_000),
    ];

    let details = BudgetDetails::build(&categories, Money::from_cents(100_000));

    assert_eq!(details.groups.len(), 1);
    assert_eq!(details.groups[0].sub_groups.len(), 1);
    let rows = &details.groups[0].sub_groups[0].rows;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].name, "Hydro");
    assert_eq!(rows[1].name, "Internet");
}

#[test]
fn group_name_without_separator_produces_single_unnamed_subgroup() {
    let categories = vec![category("Groceries", "Supermarket", 20_000)];

    let details = BudgetDetails::build(&categories, Money::from_cents(100_000));

    assert_eq!(details.groups.len(), 1);
    assert_eq!(details.groups[0].name, "Groceries");
    assert_eq!(details.groups[0].sub_groups.len(), 1);
    assert_eq!(details.groups[0].sub_groups[0].name, None);
    assert_eq!(details.groups[0].sub_groups[0].rows[0].name, "Supermarket");
}

#[test]
fn groups_without_named_subgroups_are_moved_to_the_end_preserving_relative_order() {
    let categories = vec![
        category("Groceries", "Supermarket", 20_000),
        category("Obligations - Maison", "Hydro", 10_000),
        category("Fun money", "Restaurants", 5_000),
        category("Obligations - Taxes", "Property tax", 2_000),
    ];

    let details = BudgetDetails::build(&categories, Money::from_cents(100_000));

    let names: Vec<&str> = details.groups.iter().map(|g| g.name.as_str()).collect();
    assert_eq!(names, vec!["Obligations", "Groceries", "Fun money"]);
}

#[test]
fn percentage_is_amount_over_total_income() {
    let categories = vec![category("Groceries", "Supermarket", 16_310)];

    let details = BudgetDetails::build(&categories, Money::from_cents(100_000));

    let percentage = details.groups[0].sub_groups[0].rows[0].percentage;
    assert!((percentage - 0.1631).abs() < 1e-9);
}

#[test]
fn percentage_is_zero_when_total_income_is_zero() {
    let categories = vec![category("Groceries", "Supermarket", 16_310)];

    let details = BudgetDetails::build(&categories, Money::ZERO);

    let percentage = details.groups[0].sub_groups[0].rows[0].percentage;
    assert_eq!(percentage, 0.0);
}

#[test]
fn total_sums_every_category_across_groups() {
    let categories = vec![
        category("Groceries", "Supermarket", 20_000),
        category("Obligations - Maison", "Hydro", 10_000),
        category("Obligations - Taxes", "Property tax", 2_000),
    ];

    let details = BudgetDetails::build(&categories, Money::from_cents(100_000));

    assert_eq!(details.total, Money::from_cents(32_000));
}

#[test]
fn empty_categories_produce_empty_groups_and_zero_total() {
    let details = BudgetDetails::build(&[], Money::from_cents(100_000));

    assert_eq!(details.groups, vec![]);
    assert_eq!(details.total, Money::ZERO);
}

#[test]
fn trims_stray_whitespace_around_the_separator() {
    let categories = vec![
        category("Obligations  -  Maison", "Hydro", 10_000),
        category("Obligations - Maison", "Internet", 3_000),
    ];

    let details = BudgetDetails::build(&categories, Money::from_cents(100_000));

    // Both group names should land in the SAME top/sub group once trimmed,
    // not create a visually-identical duplicate.
    assert_eq!(details.groups.len(), 1);
    assert_eq!(details.groups[0].sub_groups.len(), 1);
    assert_eq!(details.groups[0].sub_groups[0].rows.len(), 2);
}

#[test]
fn trailing_separator_with_empty_sub_name_is_treated_as_unnamed() {
    let categories = vec![category("Obligations - ", "Hydro", 10_000)];

    let details = BudgetDetails::build(&categories, Money::from_cents(100_000));

    assert_eq!(details.groups[0].name, "Obligations");
    assert_eq!(details.groups[0].sub_groups[0].name, None);
}

#[test]
fn separator_only_group_name_produces_empty_top_name_and_unnamed_subgroup() {
    let categories = vec![category(" - ", "Hydro", 10_000)];

    let details = BudgetDetails::build(&categories, Money::from_cents(100_000));

    assert_eq!(details.groups[0].name, "");
    assert_eq!(details.groups[0].sub_groups[0].name, None);
}
