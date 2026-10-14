use super::budget_summary::ExpenseCategory;
use super::money::Money;

/// A single expense category's share of `total_income`.
#[derive(Debug, Clone, PartialEq)]
pub struct DetailRow {
    pub name: String,
    pub amount: Money,
    /// `amount / total_income`
    pub percentage: f64,
}

/// A named sub-level under a [`TopGroup`].
#[derive(Debug, Clone, PartialEq)]
pub struct SubGroup {
    pub name: Option<String>,
    pub rows: Vec<DetailRow>,
}

/// A top-level group, named after the part of the CategoryGroup name.
#[derive(Debug, Clone, PartialEq)]
pub struct TopGroup {
    pub name: String,
    pub sub_groups: Vec<SubGroup>,
}

/// The full nested breakdown, plus the grand total across every category.
#[derive(Debug, Clone, PartialEq)]
pub struct BudgetDetails {
    pub groups: Vec<TopGroup>,
    pub total: Money,
}

impl BudgetDetails {
    pub fn build(categories: &[ExpenseCategory], total_income: Money) -> Self {
        let mut groups: Vec<TopGroup> = Vec::new();

        for category in categories {
            let (top_name, sub_name) = split_group_name(&category.group_name);

            let top_group = match groups.iter_mut().find(|g| g.name == top_name) {
                Some(g) => g,
                None => {
                    groups.push(TopGroup {
                        name: top_name.clone(),
                        sub_groups: Vec::new(),
                    });
                    groups.last_mut().expect("just pushed")
                }
            };

            let sub_group = match top_group.sub_groups.iter_mut().find(|s| s.name == sub_name) {
                Some(s) => s,
                None => {
                    top_group.sub_groups.push(SubGroup {
                        name: sub_name.clone(),
                        rows: Vec::new(),
                    });
                    top_group.sub_groups.last_mut().expect("just pushed")
                }
            };

            let percentage = if total_income.is_zero() {
                0.0
            } else {
                category.projected_amount.cents() as f64 / total_income.cents() as f64
            };

            sub_group.rows.push(DetailRow {
                name: category.name.clone(),
                amount: category.projected_amount,
                percentage,
            });
        }

        let (named, unnamed): (Vec<TopGroup>, Vec<TopGroup>) = groups
            .into_iter()
            .partition(|g| g.sub_groups.iter().any(|s| s.name.is_some()));

        let groups = named.into_iter().chain(unnamed).collect();

        let total = categories
            .iter()
            .fold(Money::ZERO, |acc, c| acc + c.projected_amount);

        Self { groups, total }
    }
}

fn split_group_name(group_name: &str) -> (String, Option<String>) {
    match group_name.split_once(" - ") {
        Some((top, sub)) => {
            let sub = sub.trim();
            let sub = if sub.is_empty() {
                None
            } else {
                Some(sub.to_string())
            };
            (top.trim().to_string(), sub)
        }
        None => (group_name.trim().to_string(), None),
    }
}
