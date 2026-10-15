use ab_helpers_domain::{BudgetDetails, Money, YearMonth};
use ab_helpers_server::services::actual::BudgetDetailsReport;

pub(crate) struct Line {
    pub(crate) label: String,
    pub(crate) amount: Option<String>,
    pub(crate) percentage: Option<String>,
}

const INDENT_STEP: usize = 4;

impl Line {
    fn header(label: String) -> Self {
        Self {
            label,
            amount: None,
            percentage: None,
        }
    }

    fn row(label: String, amount: String, percentage: String) -> Self {
        Self {
            label,
            amount: Some(amount),
            percentage: Some(percentage),
        }
    }

    fn amount_only(label: String, amount: String) -> Self {
        Self {
            label,
            amount: Some(amount),
            percentage: None,
        }
    }

    fn total(amount: String) -> Self {
        Self::amount_only("Total".to_string(), amount)
    }

    pub(crate) fn flatten(details: &BudgetDetails) -> Vec<Line> {
        let mut lines = Vec::new();

        for top_group in &details.groups {
            lines.push(Line::header(top_group.name.clone()));

            for sub_group in &top_group.sub_groups {
                let row_indent = if let Some(name) = &sub_group.name {
                    lines.push(Line::header(format!("{}{name}", " ".repeat(INDENT_STEP))));
                    INDENT_STEP * 2
                } else {
                    INDENT_STEP
                };

                for row in &sub_group.rows {
                    lines.push(Line::row(
                        format!("{}{}", " ".repeat(row_indent), row.name),
                        row.amount.to_string(),
                        format!("{:.2}%", row.percentage * 100.0),
                    ));
                }
            }
        }

        lines.push(Line::total(details.total.to_string()));

        lines
    }

    fn print(&self, label_width: usize, amount_width: usize, percentage_width: usize) {
        match &self.amount {
            None => println!("{}", self.label),
            Some(amount) => match &self.percentage {
                Some(percentage) => {
                    let label = &self.label;
                    println!(
                        "{label:label_width$}  {amount:>amount_width$}  {percentage:>percentage_width$}"
                    )
                }
                None => {
                    let label = &self.label;
                    println!("{label:label_width$}  {amount:>amount_width$}")
                }
            },
        }
    }
}

pub(crate) fn print_details(month: YearMonth, report: &BudgetDetailsReport) {
    let details = &report.details;
    let income_with_addback = report.income_with_addback;
    let leftover = income_with_addback - details.total;

    let mut lines = vec![
        Line::amount_only("Revenu total".to_string(), report.total_income.to_string()),
        Line::row(
            "Revenu (avec ajouts)".to_string(),
            income_with_addback.to_string(),
            percentage_of(income_with_addback, income_with_addback),
        ),
    ];
    let separator_index = lines.len();

    let mut table_lines = Line::flatten(details);
    table_lines.pop(); // Remove Total row.
    lines.extend(table_lines);

    lines.push(Line::row(
        "Total des dépenses".to_string(),
        details.total.to_string(),
        percentage_of(details.total, income_with_addback),
    ));
    lines.push(Line::row(
        format!("Restant  ({income_with_addback} - {})", details.total),
        leftover.to_string(),
        percentage_of(leftover, income_with_addback),
    ));

    let label_width = lines
        .iter()
        .map(|l| l.label.chars().count())
        .max()
        .unwrap_or(0);
    let amount_width = lines
        .iter()
        .filter_map(|l| l.amount.as_ref())
        .map(|a| a.chars().count())
        .max()
        .unwrap_or(0);
    let percentage_width = lines
        .iter()
        .filter_map(|l| l.percentage.as_ref())
        .map(|p| p.chars().count())
        .max()
        .unwrap_or(0);

    println!("== Budget details ({month}) ==");
    for (i, line) in lines.iter().enumerate() {
        if i == separator_index {
            println!("{}  {}", " ".repeat(label_width), "-".repeat(amount_width));
        }
        line.print(label_width, amount_width, percentage_width);
    }
}

fn percentage_of(amount: Money, total: Money) -> String {
    let percentage = if total.is_zero() {
        0.0
    } else {
        amount.cents() as f64 / total.cents() as f64 * 100.0
    };
    format!("{percentage:.2}%")
}
