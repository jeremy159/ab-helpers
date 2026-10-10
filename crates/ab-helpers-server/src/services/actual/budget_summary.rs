use std::collections::HashMap;
use std::sync::Arc;

use ab_helpers_domain::{
    BudgetMode, BudgetSummary, Budgeter, ComputedSalary, ExpenseCategory, IncomeSchedule, Money,
    YearMonth,
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
///
/// `budget_mode` controls both income and expenses - see
/// [`Budgeter::compute_salary`] and [`Self::resolve_expense_amount`].
pub struct BudgetSummaryService<R> {
    reader: Arc<R>,
    persons: Vec<PersonSettings>,
}

impl<R: ActualReadClient + 'static> BudgetSummaryService<R> {
    pub fn new(reader: Arc<R>, persons: Vec<PersonSettings>) -> Self {
        Self { reader, persons }
    }

    pub async fn build(
        &self,
        month: YearMonth,
        budget_mode: BudgetMode,
    ) -> ABHelpersResult<BudgetSummary> {
        let month_start = month.start_date();
        let month_end = month.end_date();
        let month_str = month.to_string();
        let month_str = month_str.as_str();

        let all_payees = self.reader.get_payees().await?;
        let mut resolved_persons: Vec<(String, String, Vec<String>)> = Vec::new();
        for person in &self.persons {
            let salary_payee_id = resolve_payee_id(&all_payees, &person.salary_payee)?;
            let allocation_payee_ids = person
                .allocation_payees
                .iter()
                .map(|payee_name| resolve_payee_id(&all_payees, payee_name))
                .collect::<ABHelpersResult<Vec<_>>>()?;
            resolved_persons.push((person.name.clone(), salary_payee_id, allocation_payee_ids));
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
                .flat_map(|(_, salary_id, allocation_ids)| {
                    std::iter::once(salary_id.as_str())
                        .chain(allocation_ids.iter().map(String::as_str))
                }),
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
                let projected_amount = self
                    .resolve_expense_amount(
                        &category.id,
                        month,
                        month_str,
                        category.budgeted,
                        budget_mode,
                    )
                    .await;
                categories.push(ExpenseCategory {
                    id: category.id.clone(),
                    name: category.name.clone(),
                    projected_amount,
                });
            }
        }

        let budgeters_with_salary: Vec<Budgeter<ComputedSalary>> = resolved_persons
            .into_iter()
            .map(|(name, salary_payee_id, allocation_payee_ids)| {
                Budgeter::new(name, salary_payee_id, allocation_payee_ids).compute_salary(
                    &income_schedules,
                    &fallback_totals,
                    month_start,
                    month_end,
                    budget_mode,
                )
            })
            .collect();

        Ok(BudgetSummary::build(budgeters_with_salary, &categories))
    }

    /// A category's monthly expense amount. Annualized: averages the
    /// projection across `month`'s calendar year instead of using just
    /// `month`'s (skipping months with no projection) - but only when
    /// `month` itself has one, since a `budgeted` fallback is a flat figure,
    /// not a projection with a pattern to average.
    async fn resolve_expense_amount(
        &self,
        category_id: &str,
        month: YearMonth,
        month_str: &str,
        budgeted: Option<i64>,
        budget_mode: BudgetMode,
    ) -> Money {
        let Some(target_projection) = self.fetch_category_projection(category_id, month_str).await
        else {
            return ExpenseCategory::resolve_projected_amount(None, budgeted);
        };

        if budget_mode == BudgetMode::Actual {
            return ExpenseCategory::resolve_projected_amount(Some(target_projection), budgeted);
        }

        let mut sum = target_projection;
        let mut count = 1i64;
        for other_month in 1..=12u32 {
            if other_month == month.month() {
                continue;
            }
            let Ok(other_month) = YearMonth::new(month.year(), other_month) else {
                continue;
            };
            if let Some(projection) = self
                .fetch_category_projection(category_id, &other_month.to_string())
                .await
            {
                sum += projection;
                count += 1;
            }
        }

        Money::from_cents((sum as f64 / count as f64).round() as i64)
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
