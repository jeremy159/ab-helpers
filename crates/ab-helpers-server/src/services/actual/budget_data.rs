use std::collections::HashMap;

use ab_helpers_domain::{
    BudgetMode, Budgeter, ComputedSalary, ExpenseCategory, IncomeSchedule, Money, YearMonth,
};
use actual::Payee;

use crate::config::PersonSettings;
use crate::error::{ABHelpersResult, AppError};

use super::income_schedule_adapter::to_income_schedule;
use super::{ActualReadClient, PayeeMatch};

pub(super) async fn resolve_budget_data<R: ActualReadClient>(
    reader: &R,
    persons: &[PersonSettings],
    month: YearMonth,
    budget_mode: BudgetMode,
    use_budgeted_fallback: bool,
) -> ABHelpersResult<(Vec<Budgeter<ComputedSalary>>, Vec<ExpenseCategory>)> {
    let month_start = month.start_date();
    let month_end = month.end_date();
    let month_str = month.to_string();
    let month_str = month_str.as_str();

    let all_payees = reader.get_payees().await?;
    let mut resolved_persons: Vec<(String, String, Vec<String>)> = Vec::new();

    for person in persons {
        let salary_payee_id = resolve_payee_id(&all_payees, &person.salary_payee)?;
        let allocation_payee_ids = person
            .allocation_payees
            .iter()
            .map(|payee_name| resolve_payee_id(&all_payees, payee_name))
            .collect::<ABHelpersResult<Vec<_>>>()?;
        resolved_persons.push((person.name.clone(), salary_payee_id, allocation_payee_ids));
    }

    let schedules = reader.get_schedules().await?;

    let income_schedules: Vec<IncomeSchedule> = schedules
        .into_iter()
        .filter_map(to_income_schedule)
        .collect();

    let fallback_payee_ids = IncomeSchedule::payees_without_schedule(
        &income_schedules,
        resolved_persons
            .iter()
            .flat_map(|(_, salary_id, allocation_ids)| {
                std::iter::once(salary_id.as_str()).chain(allocation_ids.iter().map(String::as_str))
            }),
    );

    let fallback_totals: HashMap<String, Money> = if fallback_payee_ids.is_empty() {
        HashMap::new()
    } else {
        let fallback_payee_ids: Vec<String> = fallback_payee_ids
            .iter()
            .map(|id| (*id).to_owned())
            .collect();
        reader
            .get_payee_totals(&fallback_payee_ids, month_start, month_end)
            .await?
            .into_iter()
            .map(|t| (t.payee_id, Money::from_cents(t.total)))
            .collect()
    };

    let category_groups = reader.get_budget_month(month_str).await?;
    let mut categories = Vec::new();

    for group in &category_groups {
        if group.is_income || group.hidden {
            continue;
        }

        for category in &group.categories {
            if category.hidden {
                continue;
            }

            let projected_amount = resolve_expense_amount(
                reader,
                &category.id,
                month_str,
                category.budgeted,
                use_budgeted_fallback,
            )
            .await;

            tracing::debug!(
                category_name = category.name,
                category_id = category.id,
                budgeted = category.budgeted,
                resolved_cents = projected_amount.cents(),
                "resolved expense category amount"
            );

            categories.push(ExpenseCategory {
                id: category.id.clone(),
                name: category.name.clone(),
                projected_amount,
                group_name: group.name.clone(),
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

    Ok((budgeters_with_salary, categories))
}

/// Resolves a category's expected amount for the target month.
///
/// Actual's own goal/automation templates (`goal_def`) already smooth or
/// annualize whatever they're configured to. When a category has no such template,
/// we only have its plain `budgeted` figure for *this* month to go on; that's
/// used only if `use_budgeted_fallback` is set.
async fn resolve_expense_amount<R: ActualReadClient>(
    reader: &R,
    category_id: &str,
    month_str: &str,
    budgeted: Option<i64>,
    use_budgeted_fallback: bool,
) -> Money {
    match fetch_category_projection(reader, category_id, month_str).await {
        Some(target_projection) => Money::from_cents(target_projection),
        None if use_budgeted_fallback => ExpenseCategory::resolve_projected_amount(None, budgeted),
        None => Money::ZERO,
    }
}

async fn fetch_category_projection<R: ActualReadClient>(
    reader: &R,
    category_id: &str,
    month: &str,
) -> Option<i64> {
    match reader.get_category_projection(category_id, month).await {
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

pub(super) fn is_income_addback(addback_categories: &[String], category: &ExpenseCategory) -> bool {
    addback_categories
        .iter()
        .any(|addback_name| normalize(&category.name) == normalize(addback_name))
}

pub(super) fn unmatched_addback_categories(
    addback_categories: &[String],
    categories: &[ExpenseCategory],
) -> Vec<String> {
    addback_categories
        .iter()
        .filter(|addback_name| {
            !categories
                .iter()
                .any(|category| normalize(&category.name) == normalize(addback_name))
        })
        .cloned()
        .inspect(|addback_name| {
            tracing::warn!(
                addback_name,
                "income_addback_categories entry matched no fetched category (check for a typo, \
                 stray whitespace, or a hidden/renamed Actual category)"
            );
        })
        .collect()
}

fn normalize(name: &str) -> String {
    strip_leading_emoji(name.trim()).trim().to_lowercase()
}

fn strip_leading_emoji(name: &str) -> &str {
    let end = name
        .char_indices()
        .find(|(_, c)| c.is_alphanumeric())
        .map_or(name.len(), |(i, _)| i);
    &name[end..]
}
