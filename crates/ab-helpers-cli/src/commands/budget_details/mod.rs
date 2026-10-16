use super::budget_summary::BudgetModeArg;
use super::error::CliError;
use ab_helpers_domain::{BudgetMode, YearMonth};
use ab_helpers_server::config::Settings;
use ab_helpers_server::services::actual::BudgetDetailsService;
use clap::Args;
use render::print_details;

pub(crate) mod render;

#[derive(Args, Debug)]
pub struct BudgetDetailsArgs {
    /// Month to report on, `YYYY-MM`. Defaults to the current month.
    #[arg(long)]
    pub month: Option<String>,

    /// How salary and Budget-Automation-projected expenses are computed:
    /// `annualized` (calendar-year average, smoothing lumpy pay periods and
    /// annual bills) or `actual` (target month only).
    #[arg(long, value_enum, default_value = "annualized")]
    pub budget_mode: BudgetModeArg,

    /// When a category has no Actual goal/automation template, fall back to
    /// its plain `budgeted` amount for the target month instead of showing $0.
    #[arg(long)]
    pub use_budgeted_fallback: bool,
}

pub async fn run(settings: Settings, args: BudgetDetailsArgs) -> Result<(), CliError> {
    let month = match args.month {
        Some(month) => month
            .parse::<YearMonth>()
            .map_err(|e| CliError::Failure(anyhow::anyhow!(e)))?,
        None => YearMonth::current(),
    };

    let budget_mode: BudgetMode = args.budget_mode.into();

    tracing::info!(month = %month, ?budget_mode, "budget-details started");

    let persons = settings.actual.budget_summary.persons.clone();
    if persons.is_empty() {
        println!("No persons configured under `actual.budget_summary.persons`. Nothing to report.");
        return Ok(());
    }
    let income_addback_categories = settings
        .actual
        .budget_details
        .income_addback_categories
        .clone();
    let use_budgeted_fallback = args.use_budgeted_fallback;

    settings
        .actual
        .with_session(move |client| async move {
            let service = BudgetDetailsService::new(
                client,
                persons,
                income_addback_categories,
                use_budgeted_fallback,
            );
            let (report, unmatched_addback_categories) = service
                .build_details(month, budget_mode)
                .await
                .map_err(|e| CliError::Failure(e.into()))?;

            tracing::info!(
                groups = report.details.groups.len(),
                total = %report.details.total,
                "budget-details computed"
            );

            for name in &unmatched_addback_categories {
                super::print_warning(&format!(
                    "income_addback_categories entry \"{name}\" matched no fetched category \
                     (check for a typo, stray whitespace, or a hidden/renamed Actual category)"
                ));
            }

            print_details(month, &report);
            Ok(())
        })
        .await
}
