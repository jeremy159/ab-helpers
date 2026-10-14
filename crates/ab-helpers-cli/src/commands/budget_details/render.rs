use ab_helpers_domain::{BudgetDetails, YearMonth};

pub(crate) struct Line {
    pub(crate) label: String,
    pub(crate) amount: Option<String>,
    pub(crate) percentage: Option<String>,
}

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

    fn total(amount: String) -> Self {
        Self {
            label: "Total".to_string(),
            amount: Some(amount),
            percentage: None,
        }
    }

    pub(crate) fn flatten(details: &BudgetDetails) -> Vec<Line> {
        let mut lines = Vec::new();

        for top_group in &details.groups {
            lines.push(Line::header(top_group.name.clone()));

            for sub_group in &top_group.sub_groups {
                let row_indent = if let Some(name) = &sub_group.name {
                    lines.push(Line::header(format!("  {name}")));
                    4
                } else {
                    2
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

pub(crate) fn print_details(month: YearMonth, details: &BudgetDetails) {
    let lines = Line::flatten(details);

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
    for line in &lines {
        line.print(label_width, amount_width, percentage_width);
    }
}
