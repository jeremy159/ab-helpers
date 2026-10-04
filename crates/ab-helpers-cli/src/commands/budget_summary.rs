use super::error::CliError;
use ab_helpers_domain::{BudgetSummary, BudgeterExt, YearMonth};
use ab_helpers_server::config::Settings;
use ab_helpers_server::services::actual::BudgetSummaryService;
use clap::Args;

#[derive(Args, Debug)]
pub struct BudgetSummaryArgs {
    /// Month to report on, `YYYY-MM`. Defaults to the current month.
    #[arg(long)]
    pub month: Option<String>,
}

pub async fn run(settings: Settings, args: BudgetSummaryArgs) -> Result<(), CliError> {
    let month = match args.month {
        Some(month) => month
            .parse::<YearMonth>()
            .map_err(|e| CliError::Failure(anyhow::anyhow!(e)))?,
        None => YearMonth::current(),
    };

    tracing::info!(month = %month, "budget-summary started");

    let persons = settings.actual.budget_summary.persons.clone();
    if persons.is_empty() {
        println!(
            "No persons configured under `actual.budget_summary.persons`. Nothing to report."
        );
        return Ok(());
    }

    settings
        .actual
        .with_session(move |client| async move {
            let service = BudgetSummaryService::new(client, persons);
            let summary = service
                .build(month)
                .await
                .map_err(|e| CliError::Failure(e.into()))?;

            tracing::info!(
                budgeters = summary.budgeters.len(),
                total_salary = %summary.total.salary_month(),
                "budget-summary computed"
            );

            print_table(month, &summary);
            Ok(())
        })
        .await
}

fn print_table(month: YearMonth, summary: &BudgetSummary) {
    let mut columns: Vec<&dyn BudgeterExt> = summary
        .budgeters
        .iter()
        .map(|b| b as &dyn BudgeterExt)
        .collect();
    columns.push(&summary.total);

    let headers: Vec<String> = std::iter::once(String::new())
        .chain(columns.iter().map(|c| c.name().to_string()))
        .collect();

    let rows: Vec<(&str, Vec<String>)> = vec![
        (
            "Paie",
            columns.iter().map(|c| c.salary_month().to_string()).collect(),
        ),
        (
            "Fraction",
            columns
                .iter()
                .map(|c| format!("{:.0}%", c.proportion() * 100.0))
                .collect(),
        ),
        (
            "Estimation Dépenses S",
            columns
                .iter()
                .map(|c| c.common_expenses().to_string())
                .collect(),
        ),
        (
            "Estimation Dépenses J",
            columns
                .iter()
                .map(|c| c.individual_expenses().to_string())
                .collect(),
        ),
        (
            "Restant",
            columns.iter().map(|c| c.left_over().to_string()).collect(),
        ),
    ];

    let mut col_widths: Vec<usize> = headers.iter().map(|h| h.chars().count()).collect();
    for (label, values) in &rows {
        col_widths[0] = col_widths[0].max(label.chars().count());
        for (i, v) in values.iter().enumerate() {
            col_widths[i + 1] = col_widths[i + 1].max(v.chars().count());
        }
    }

    println!("== Budget summary ({month}) ==");
    print_row(&headers, &col_widths);
    for (label, values) in &rows {
        let mut cells = vec![label.to_string()];
        cells.extend(values.iter().cloned());
        print_row(&cells, &col_widths);
    }
}

fn print_row(cells: &[String], widths: &[usize]) {
    let line: Vec<String> = cells
        .iter()
        .zip(widths)
        .map(|(cell, width)| format!("{cell:<width$}"))
        .collect();
    println!("{}", line.join("  "));
}
