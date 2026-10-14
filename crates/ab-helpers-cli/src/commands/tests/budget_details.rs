use crate::commands::budget_details::render::Line;
use ab_helpers_domain::{BudgetDetails, DetailRow, Money, SubGroup, TopGroup};

fn sample_details() -> BudgetDetails {
    BudgetDetails {
        groups: vec![
            TopGroup {
                name: "Obligations".to_string(),
                sub_groups: vec![
                    SubGroup {
                        name: Some("Maison".to_string()),
                        rows: vec![
                            DetailRow {
                                name: "Hypothèque".to_string(),
                                amount: Money::from_cents(195_070),
                                percentage: 0.1631,
                            },
                            DetailRow {
                                name: "Assurance Habitation".to_string(),
                                amount: Money::from_cents(15_188),
                                percentage: 0.0127,
                            },
                        ],
                    },
                    SubGroup {
                        name: Some("Taxes".to_string()),
                        rows: vec![DetailRow {
                            name: "Taxes Municipales #1".to_string(),
                            amount: Money::from_cents(4_618),
                            percentage: 0.0039,
                        }],
                    },
                ],
            },
            TopGroup {
                name: "Divers".to_string(),
                sub_groups: vec![SubGroup {
                    name: None,
                    rows: vec![DetailRow {
                        name: "Cadeaux".to_string(),
                        amount: Money::from_cents(2_500),
                        percentage: 0.0021,
                    }],
                }],
            },
        ],
        total: Money::from_cents(217_376),
    }
}

#[test]
fn build_lines_indents_named_subgroups_under_their_header_and_unnamed_rows_one_level() {
    let details = sample_details();
    let lines = Line::flatten(&details);
    let labels: Vec<&str> = lines.iter().map(|l| l.label.as_str()).collect();

    assert_eq!(
        labels,
        vec![
            "Obligations",
            "  Maison",
            "    Hypothèque",
            "    Assurance Habitation",
            "  Taxes",
            "    Taxes Municipales #1",
            "Divers",
            "  Cadeaux",
            "Total",
        ]
    );
}

#[test]
fn build_lines_formats_amount_and_percentage_for_rows_only() {
    let details = sample_details();
    let lines = Line::flatten(&details);

    let hypotheque = lines
        .iter()
        .find(|l| l.label.trim() == "Hypothèque")
        .unwrap();
    assert_eq!(hypotheque.amount.as_deref(), Some("$1 950.70"));
    assert_eq!(hypotheque.percentage.as_deref(), Some("16.31%"));

    let obligations = lines.iter().find(|l| l.label == "Obligations").unwrap();
    assert!(obligations.amount.is_none());
    assert!(obligations.percentage.is_none());

    let total = lines.iter().find(|l| l.label == "Total").unwrap();
    assert_eq!(total.amount.as_deref(), Some("$2 173.76"));
    assert!(total.percentage.is_none());
}
