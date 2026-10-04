//! Splits a household's monthly income and expenses across its budgeters.
//!
//! Each [`Budgeter`] moves through `Configured -> ComputedSalary ->
//! ComputedExpenses`, so a budgeter can't be charged expenses before its
//! salary is known: income comes from recurring [`crate::ScheduleTiming`]
//! occurrences, or, for a payee with no matching schedule, a historical
//! transaction total; each budgeter's share of common expenses is then
//! proportional to their salary, while individual expenses are matched to
//! them by name. Expense amounts themselves (Budget Automation dry-run,
//! falling back to `budgeted`) are resolved by the caller before reaching
//! this module, which only ever sees a plain [`Money`] per category.
//!
//! These types don't derive `Serialize`/`Deserialize`: nothing outside this
//! crate consumes a `BudgetSummary` directly, a CLI report reads it through
//! [`BudgeterExt`] instead.

use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use uuid::Uuid;

use super::money::Money;
use super::recurrence::ScheduleTiming;

/// A person's recurring (or one-off) income source: a payee plus its
/// resolved per-occurrence amount and timing.
#[derive(Debug, Clone)]
pub struct IncomeSchedule {
    pub payee_id: String,
    /// Cents per occurrence (already resolved: the fixed amount, or the
    /// midpoint of an `isbetween` range).
    pub amount: Money,
    pub timing: ScheduleTiming,
}

impl IncomeSchedule {
    /// The ids in `payee_ids` that none of `schedules` cover, in the order
    /// given - the payees a caller must fall back to a historical
    /// transaction total for.
    pub fn payees_without_schedule<'a>(
        schedules: &[IncomeSchedule],
        payee_ids: impl IntoIterator<Item = &'a str>,
    ) -> Vec<&'a str> {
        let scheduled: HashSet<&str> = schedules.iter().map(|s| s.payee_id.as_str()).collect();
        payee_ids
            .into_iter()
            .filter(|id| !scheduled.contains(id))
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct ExpenseCategory {
    pub id: String,
    pub name: String,
    pub projected_amount: Money,
}

impl ExpenseCategory {
    /// Whether this category's name contains `name` (case-insensitive),
    /// i.e. it's individual to the budgeter called `name`.
    fn belongs_to(&self, name: &str) -> bool {
        !name.trim().is_empty() && self.name.to_lowercase().contains(&name.to_lowercase())
    }

    /// Whether this category is individual to any of `budgeters`.
    fn matches_any_budgeter(&self, budgeters: &[Budgeter<ComputedSalary>]) -> bool {
        budgeters.iter().any(|b| self.belongs_to(b.name()))
    }

    /// Budget Automation's computed `projection` when available, else
    /// `budgeted`, else zero.
    pub fn resolve_projected_amount(projection: Option<i64>, budgeted: Option<i64>) -> Money {
        Money::from_cents(projection.or(budgeted).unwrap_or(0))
    }
}

/// Where a [`Budgeter`]'s salary for a payee came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SalarySource {
    /// Expanded from a matching recurring/one-off schedule.
    Schedule,
    /// No schedule matched this payee; summed from historical transactions
    /// over the target month instead.
    TransactionHistoryFallback,
}

/// Per-payee breakdown behind a [`Budgeter`]'s `salary_month`, for display.
#[derive(Debug, Clone)]
pub struct SalaryFragment {
    pub payee_id: String,
    pub occurrences: Vec<NaiveDate>,
    pub amount_per_occurrence: Money,
    pub total: Money,
    pub source: SalarySource,
}

pub trait BudgeterState {}

#[derive(Debug, Clone)]
pub struct Configured {
    pub id: Uuid,
    pub name: String,
    pub payee_ids: Vec<String>,
}
impl BudgeterState for Configured {}

#[derive(Debug, Clone)]
pub struct ComputedSalary {
    pub configured: Configured,
    pub salary_month: Money,
    pub fragments: Vec<SalaryFragment>,
}
impl BudgeterState for ComputedSalary {}

#[derive(Debug, Clone)]
pub struct ComputedExpenses {
    pub computed_salary: ComputedSalary,
    pub proportion: f64,
    pub common_expenses: Money,
    pub individual_expenses: Money,
    pub left_over: Money,
}
impl BudgeterState for ComputedExpenses {}

/// A single person moving through `Configured -> ComputedSalary ->
/// ComputedExpenses`.
#[derive(Debug, Clone)]
pub struct Budgeter<S: BudgeterState> {
    state: S,
}

#[derive(Debug, Clone)]
pub struct TotalComputedSalary {
    pub salary_month: Money,
}
impl BudgeterState for TotalComputedSalary {}

#[derive(Debug, Clone)]
pub struct TotalComputedExpenses {
    pub salary_month: Money,
    pub common_expenses: Money,
    pub individual_expenses: Money,
    pub left_over: Money,
}
impl BudgeterState for TotalComputedExpenses {}

/// The household total, mirroring [`Budgeter`] through
/// `ComputedSalary -> ComputedExpenses` (it has no `Configured` state: it's
/// always derived from the individual budgeters).
#[derive(Debug, Clone)]
pub struct TotalBudgeter<S: BudgeterState> {
    state: S,
}

pub trait BudgeterExt {
    fn id(&self) -> Uuid;

    fn name(&self) -> &str;

    fn salary_month(&self) -> Money {
        Money::ZERO
    }

    fn proportion(&self) -> f64 {
        0.0
    }

    fn common_expenses(&self) -> Money {
        Money::ZERO
    }

    fn individual_expenses(&self) -> Money {
        Money::ZERO
    }

    fn left_over(&self) -> Money {
        Money::ZERO
    }
}

impl Budgeter<Configured> {
    pub fn new(name: impl Into<String>, payee_ids: Vec<String>) -> Self {
        Self {
            state: Configured {
                id: Uuid::new_v4(),
                name: name.into(),
                payee_ids,
            },
        }
    }

    /// Sums this person's income for `[month_start, month_end]`: for each
    /// configured payee, expands any matching `schedules`' occurrences in
    /// that range and sums `amount * occurrences`.
    ///
    /// A payee with no matching schedule falls back to
    /// `fallback_totals` (a historical-transaction sum for that payee/month,
    /// resolved by the caller).
    pub fn compute_salary(
        self,
        schedules: &[IncomeSchedule],
        fallback_totals: &HashMap<String, Money>,
        month_start: NaiveDate,
        month_end: NaiveDate,
    ) -> Budgeter<ComputedSalary> {
        let mut fragments = Vec::new();
        let mut salary_month = Money::ZERO;

        for payee_id in &self.state.payee_ids {
            let payee_schedules: Vec<&IncomeSchedule> = schedules
                .iter()
                .filter(|s| &s.payee_id == payee_id)
                .collect();

            if payee_schedules.is_empty() {
                if let Some(total) = fallback_totals.get(payee_id) {
                    salary_month = salary_month + *total;

                    fragments.push(SalaryFragment {
                        payee_id: payee_id.clone(),
                        occurrences: Vec::new(),
                        amount_per_occurrence: *total,
                        total: *total,
                        source: SalarySource::TransactionHistoryFallback,
                    });
                }

                continue;
            }

            for schedule in payee_schedules {
                let occurrences = schedule.timing.expand_occurrences(month_start, month_end);

                if occurrences.is_empty() {
                    continue;
                }

                let total = Money::from_cents(schedule.amount.cents() * occurrences.len() as i64);

                salary_month = salary_month + total;

                fragments.push(SalaryFragment {
                    payee_id: payee_id.clone(),
                    occurrences,
                    amount_per_occurrence: schedule.amount,
                    total,
                    source: SalarySource::Schedule,
                });
            }
        }

        Budgeter {
            state: ComputedSalary {
                configured: self.state,
                salary_month,
                fragments,
            },
        }
    }
}

impl Budgeter<ComputedSalary> {
    /// `total`'s common/individual totals must already be computed (via
    /// [`TotalBudgeter::compute_expenses`]), and `individual_categories`
    /// must be the individual-only subset it returned.
    pub fn compute_expenses(
        self,
        total: &TotalBudgeter<TotalComputedExpenses>,
        individual_categories: &[&ExpenseCategory],
    ) -> Budgeter<ComputedExpenses> {
        let name = self.state.configured.name.clone();
        let salary_month = self.state.salary_month;

        let proportion = if total.salary_month().is_zero() {
            0.0
        } else {
            salary_month.cents() as f64 / total.salary_month().cents() as f64
        };

        let common_expenses =
            Money::from_cents((proportion * total.common_expenses().cents() as f64).round() as i64);

        let individual_expenses = individual_categories
            .iter()
            .filter(|c| c.belongs_to(&name))
            .fold(Money::ZERO, |acc, c| acc + c.projected_amount);

        let left_over = salary_month - common_expenses - individual_expenses;

        Budgeter {
            state: ComputedExpenses {
                computed_salary: self.state,
                proportion,
                common_expenses,
                individual_expenses,
                left_over,
            },
        }
    }
}

impl TotalBudgeter<TotalComputedSalary> {
    pub fn compute_salary(budgeters: &[Budgeter<ComputedSalary>]) -> Self {
        let salary_month = budgeters
            .iter()
            .fold(Money::ZERO, |acc, b| acc + b.salary_month());

        Self {
            state: TotalComputedSalary { salary_month },
        }
    }

    /// Classifies each category as individual (its name contains a
    /// configured budgeter's name, case-insensitive) or common, sums each
    /// bucket, and returns the total plus the individual-only subset, for
    /// each [`Budgeter::compute_expenses`] to re-filter by its own name.
    pub fn compute_expenses<'a>(
        self,
        categories: &'a [ExpenseCategory],
        budgeters: &[Budgeter<ComputedSalary>],
    ) -> (
        TotalBudgeter<TotalComputedExpenses>,
        Vec<&'a ExpenseCategory>,
    ) {
        let mut common_expenses = Money::ZERO;
        let mut individual_expenses = Money::ZERO;
        let mut individual_categories = Vec::new();

        for category in categories {
            if category.matches_any_budgeter(budgeters) {
                individual_expenses = individual_expenses + category.projected_amount;
                individual_categories.push(category);
            } else {
                common_expenses = common_expenses + category.projected_amount;
            }
        }

        let left_over = self.state.salary_month - common_expenses - individual_expenses;

        (
            TotalBudgeter {
                state: TotalComputedExpenses {
                    salary_month: self.state.salary_month,
                    common_expenses,
                    individual_expenses,
                    left_over,
                },
            },
            individual_categories,
        )
    }
}

impl BudgeterExt for Budgeter<Configured> {
    fn id(&self) -> Uuid {
        self.state.id
    }

    fn name(&self) -> &str {
        &self.state.name
    }
}

impl BudgeterExt for Budgeter<ComputedSalary> {
    fn id(&self) -> Uuid {
        self.state.configured.id
    }

    fn name(&self) -> &str {
        &self.state.configured.name
    }

    fn salary_month(&self) -> Money {
        self.state.salary_month
    }
}

impl Budgeter<ComputedSalary> {
    pub fn fragments(&self) -> &[SalaryFragment] {
        &self.state.fragments
    }
}

impl BudgeterExt for Budgeter<ComputedExpenses> {
    fn id(&self) -> Uuid {
        self.state.computed_salary.configured.id
    }

    fn name(&self) -> &str {
        &self.state.computed_salary.configured.name
    }

    fn salary_month(&self) -> Money {
        self.state.computed_salary.salary_month
    }

    fn proportion(&self) -> f64 {
        self.state.proportion
    }

    fn common_expenses(&self) -> Money {
        self.state.common_expenses
    }

    fn individual_expenses(&self) -> Money {
        self.state.individual_expenses
    }

    fn left_over(&self) -> Money {
        self.state.left_over
    }
}

impl BudgeterExt for TotalBudgeter<TotalComputedSalary> {
    fn id(&self) -> Uuid {
        Uuid::nil()
    }

    fn name(&self) -> &str {
        "Total"
    }

    fn salary_month(&self) -> Money {
        self.state.salary_month
    }
}

impl BudgeterExt for TotalBudgeter<TotalComputedExpenses> {
    fn id(&self) -> Uuid {
        Uuid::nil()
    }

    fn name(&self) -> &str {
        "Total"
    }

    fn salary_month(&self) -> Money {
        self.state.salary_month
    }

    fn proportion(&self) -> f64 {
        1.0
    }

    fn common_expenses(&self) -> Money {
        self.state.common_expenses
    }

    fn individual_expenses(&self) -> Money {
        self.state.individual_expenses
    }

    fn left_over(&self) -> Money {
        self.state.left_over
    }
}

/// Orchestrates the full computation
#[derive(Debug, Clone)]
pub struct BudgetSummary {
    pub budgeters: Vec<Budgeter<ComputedExpenses>>,
    pub total: TotalBudgeter<TotalComputedExpenses>,
}

impl BudgetSummary {
    pub fn build(
        budgeters_with_salary: Vec<Budgeter<ComputedSalary>>,
        categories: &[ExpenseCategory],
    ) -> Self {
        let total_salary = TotalBudgeter::compute_salary(&budgeters_with_salary);
        let (total, individual_categories) =
            total_salary.compute_expenses(categories, &budgeters_with_salary);

        let budgeters = budgeters_with_salary
            .into_iter()
            .map(|b| b.compute_expenses(&total, &individual_categories))
            .collect();

        Self { budgeters, total }
    }
}
