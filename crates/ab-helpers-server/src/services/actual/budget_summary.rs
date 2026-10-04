use std::collections::HashMap;
use std::sync::Arc;

use ab_helpers_domain::{
    BudgetSummary, Budgeter, ComputedSalary, ExpenseCategory, IncomeSchedule, Money, YearMonth,
};
use actual::Payee;

use crate::config::PersonSettings;
use crate::error::{ABHelpersResult, AppError};

use super::income_schedule_adapter::to_income_schedule;
use super::{ActualReadClient, PayeeMatch};

/// Builds an Actual-derived [`BudgetSummary`] for a month: resolves
/// configured payees, sums each person's income from schedules (falling
/// back to historical transaction totals), and classifies/sums expense
/// categories via their Budget Automation projection (falling back to
/// `budgeted`).
pub struct BudgetSummaryService<R> {
    reader: Arc<R>,
    persons: Vec<PersonSettings>,
}

impl<R: ActualReadClient + 'static> BudgetSummaryService<R> {
    pub fn new(reader: Arc<R>, persons: Vec<PersonSettings>) -> Self {
        Self { reader, persons }
    }

    pub async fn build(&self, month: YearMonth) -> ABHelpersResult<BudgetSummary> {
        let month_start = month.start_date();
        let month_end = month.end_date();
        let month_str = month.to_string();
        let month_str = month_str.as_str();

        let all_payees = self.reader.get_payees().await?;
        let mut resolved_persons: Vec<(String, Vec<String>)> = Vec::new();
        for person in &self.persons {
            let payee_ids = person
                .payees
                .iter()
                .map(|payee_name| resolve_payee_id(&all_payees, payee_name))
                .collect::<ABHelpersResult<Vec<_>>>()?;
            resolved_persons.push((person.name.clone(), payee_ids));
        }

        let schedules = self.reader.get_schedules().await?;
        let income_schedules: Vec<IncomeSchedule> = schedules
            .into_iter()
            .filter_map(to_income_schedule)
            .collect();

        let fallback_payee_ids = IncomeSchedule::payees_without_schedule(
            &income_schedules,
            resolved_persons
                .iter()
                .flat_map(|(_, ids)| ids.iter().map(String::as_str)),
        );

        let fallback_totals: HashMap<String, Money> = if fallback_payee_ids.is_empty() {
            HashMap::new()
        } else {
            let fallback_payee_ids: Vec<String> = fallback_payee_ids
                .iter()
                .map(|id| (*id).to_owned())
                .collect();
            self.reader
                .get_payee_totals(&fallback_payee_ids, month_start, month_end)
                .await?
                .into_iter()
                .map(|t| (t.payee_id, Money::from_cents(t.total)))
                .collect()
        };

        let category_groups = self.reader.get_budget_month(month_str).await?;
        let mut categories = Vec::new();
        for group in &category_groups {
            if group.is_income || group.hidden {
                continue;
            }
            for category in &group.categories {
                if category.hidden {
                    continue;
                }
                let projection = self
                    .fetch_category_projection(&category.id, month_str)
                    .await;
                categories.push(ExpenseCategory {
                    id: category.id.clone(),
                    name: category.name.clone(),
                    projected_amount: ExpenseCategory::resolve_projected_amount(
                        projection,
                        category.budgeted,
                    ),
                });
            }
        }

        let budgeters_with_salary: Vec<Budgeter<ComputedSalary>> = resolved_persons
            .into_iter()
            .map(|(name, payee_ids)| {
                Budgeter::new(name, payee_ids).compute_salary(
                    &income_schedules,
                    &fallback_totals,
                    month_start,
                    month_end,
                )
            })
            .collect();

        Ok(BudgetSummary::build(budgeters_with_salary, &categories))
    }

    /// Budget Automation's computed projection, or `None` if absent/failed -
    /// see the `get-category-projection` doc comments in the bridge and
    /// `ActualReadRequests` for why this can fail/be absent. The caller
    /// falls back to `budgeted` in that case.
    async fn fetch_category_projection(&self, category_id: &str, month: &str) -> Option<i64> {
        match self
            .reader
            .get_category_projection(category_id, month)
            .await
        {
            Ok(projection) => projection,
            Err(error) => {
                tracing::warn!(
                    category_id,
                    month,
                    ?error,
                    "get-category-projection failed; falling back to budgeted"
                );
                None
            }
        }
    }
}

fn resolve_payee_id(all_payees: &[Payee], payee_name: &str) -> ABHelpersResult<String> {
    match PayeeMatch::resolve(all_payees, payee_name) {
        PayeeMatch::Found(payee) => Ok(payee.id),
        PayeeMatch::NotFound => Err(AppError::ActualPayeeNotFound {
            name: payee_name.to_owned(),
            available: all_payees.to_vec(),
        }),
        PayeeMatch::Ambiguous(matches) => Err(AppError::ActualPayeeAmbiguous {
            name: payee_name.to_owned(),
            matches,
        }),
    }
}
