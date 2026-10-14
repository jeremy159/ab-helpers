use std::sync::Arc;

use ab_helpers_domain::{BudgetMode, BudgetSummary, ExpenseCategory, YearMonth};

use crate::config::PersonSettings;
use crate::error::ABHelpersResult;

use super::ActualReadClient;
use super::budget_data::{is_income_addback, resolve_budget_data, unmatched_addback_categories};

pub struct BudgetSummaryService<R> {
    reader: Arc<R>,
    persons: Vec<PersonSettings>,
    income_addback_categories: Vec<String>,
}

impl<R: ActualReadClient + 'static> BudgetSummaryService<R> {
    pub fn new(
        reader: Arc<R>,
        persons: Vec<PersonSettings>,
        income_addback_categories: Vec<String>,
    ) -> Self {
        Self {
            reader,
            persons,
            income_addback_categories,
        }
    }

    pub async fn build(
        &self,
        month: YearMonth,
        budget_mode: BudgetMode,
    ) -> ABHelpersResult<(BudgetSummary, Vec<String>)> {
        let (budgeters_with_salary, categories) =
            resolve_budget_data(self.reader.as_ref(), &self.persons, month, budget_mode).await?;

        let unmatched = unmatched_addback_categories(&self.income_addback_categories, &categories);

        let filtered_categories: Vec<ExpenseCategory> = categories
            .into_iter()
            .filter(|category| !is_income_addback(&self.income_addback_categories, category))
            .collect();

        let summary = BudgetSummary::build(budgeters_with_salary, &filtered_categories);
        Ok((summary, unmatched))
    }
}
