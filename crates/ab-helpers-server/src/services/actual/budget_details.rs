use std::sync::Arc;

use ab_helpers_domain::{BudgetDetails, BudgetMode, BudgeterExt, Money, YearMonth};

use crate::config::PersonSettings;
use crate::error::ABHelpersResult;

use super::ActualReadClient;
use super::budget_data::{is_income_addback, resolve_budget_data, unmatched_addback_categories};

pub struct BudgetDetailsService<R> {
    reader: Arc<R>,
    persons: Vec<PersonSettings>,
    income_addback_categories: Vec<String>,
    use_budgeted_fallback: bool,
}

pub struct BudgetDetailsReport {
    pub details: BudgetDetails,
    pub total_income: Money,
    pub income_with_addback: Money,
}

impl<R: ActualReadClient + 'static> BudgetDetailsService<R> {
    pub fn new(
        reader: Arc<R>,
        persons: Vec<PersonSettings>,
        income_addback_categories: Vec<String>,
        use_budgeted_fallback: bool,
    ) -> Self {
        Self {
            reader,
            persons,
            income_addback_categories,
            use_budgeted_fallback,
        }
    }

    pub async fn build_details(
        &self,
        month: YearMonth,
        budget_mode: BudgetMode,
    ) -> ABHelpersResult<(BudgetDetailsReport, Vec<String>)> {
        let (budgeters_with_salary, categories) = resolve_budget_data(
            self.reader.as_ref(),
            &self.persons,
            month,
            budget_mode,
            self.use_budgeted_fallback,
        )
        .await?;

        let unmatched = unmatched_addback_categories(&self.income_addback_categories, &categories);

        let total_income = budgeters_with_salary
            .iter()
            .fold(Money::ZERO, |acc, b| acc + b.salary_month());

        let addback_total = categories
            .iter()
            .filter(|category| is_income_addback(&self.income_addback_categories, category))
            .fold(Money::ZERO, |acc, c| acc + c.projected_amount);

        let income_with_addback = total_income + addback_total;

        let details = BudgetDetails::build(&categories, income_with_addback);
        let report = BudgetDetailsReport {
            details,
            total_income,
            income_with_addback,
        };
        Ok((report, unmatched))
    }
}
