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

use chrono::{Datelike, NaiveDate};
use uuid::Uuid;

use super::money::Money;
use super::recurrence::{RecurringConfig, RecurringFrequency, ScheduleTiming};

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

/// Whether income/expenses use the target month's actual value, or a
/// calendar-year average (smooths out lumpy biweekly pay periods, annual
/// bills, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BudgetMode {
    #[default]
    Annualized,
    Actual,
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
    /// The payee whose schedule is this person's regular paycheque
    pub salary_payee_id: String,
    /// Other income payees (benefits, allowances, etc.)
    pub allocation_payee_ids: Vec<String>,
}
impl BudgeterState for Configured {}

impl Configured {
    fn all_payee_ids(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.salary_payee_id.as_str())
            .chain(self.allocation_payee_ids.iter().map(String::as_str))
    }
}

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
    pub salary_occurrence: Money,
    pub allocations_total: Money,
}
impl BudgeterState for TotalComputedSalary {}

#[derive(Debug, Clone)]
pub struct TotalComputedExpenses {
    pub salary_month: Money,
    pub salary_occurrence: Money,
    pub allocations_total: Money,
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

    fn salary_occurrence(&self) -> Money {
        Money::ZERO
    }

    fn allocations_total(&self) -> Money {
        Money::ZERO
    }
}

impl Budgeter<Configured> {
    pub fn new(
        name: impl Into<String>,
        salary_payee_id: impl Into<String>,
        allocation_payee_ids: Vec<String>,
    ) -> Self {
        Self {
            state: Configured {
                id: Uuid::new_v4(),
                name: name.into(),
                salary_payee_id: salary_payee_id.into(),
                allocation_payee_ids,
            },
        }
    }

    /// Sums this person's income for `[month_start, month_end]`. Under
    /// `BudgetMode::Annualized`, a recurring schedule is instead averaged
    /// over the whole calendar year (`OneOff` and the `fallback_totals`
    /// path are unaffected - there's no yearly pattern to average there).
    pub fn compute_salary(
        self,
        schedules: &[IncomeSchedule],
        fallback_totals: &HashMap<String, Money>,
        month_start: NaiveDate,
        month_end: NaiveDate,
        budget_mode: BudgetMode,
    ) -> Budgeter<ComputedSalary> {
        let mut fragments = Vec::new();
        let mut salary_month = Money::ZERO;

        for payee_id in self.state.all_payee_ids().collect::<Vec<_>>() {
            let payee_schedules: Vec<&IncomeSchedule> = schedules
                .iter()
                .filter(|s| s.payee_id == payee_id)
                .collect();

            if payee_schedules.is_empty() {
                if let Some(total) = fallback_totals.get(payee_id) {
                    salary_month = salary_month + *total;

                    fragments.push(SalaryFragment {
                        payee_id: payee_id.to_string(),
                        occurrences: Vec::new(),
                        amount_per_occurrence: *total,
                        total: *total,
                        source: SalarySource::TransactionHistoryFallback,
                    });
                }

                continue;
            }

            for schedule in payee_schedules {
                let fragment = match (budget_mode, &schedule.timing) {
                    (BudgetMode::Annualized, ScheduleTiming::Recurring(_)) => {
                        annualized_fragment(payee_id, schedule, month_start)
                    }
                    _ => actual_month_fragment(payee_id, schedule, month_start, month_end),
                };

                if let Some(fragment) = fragment {
                    salary_month = salary_month + fragment.total;
                    fragments.push(fragment);
                }
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

/// `amount * occurrences actually falling within [month_start, month_end]`.
fn actual_month_fragment(
    payee_id: &str,
    schedule: &IncomeSchedule,
    month_start: NaiveDate,
    month_end: NaiveDate,
) -> Option<SalaryFragment> {
    let occurrences = schedule.timing.expand_occurrences(month_start, month_end);
    if occurrences.is_empty() {
        return None;
    }

    let total = Money::from_cents(schedule.amount.cents() * occurrences.len() as i64);

    Some(SalaryFragment {
        payee_id: payee_id.to_string(),
        occurrences,
        amount_per_occurrence: schedule.amount,
        total,
        source: SalarySource::Schedule,
    })
}

/// `amount * canonical_occurrences_per_year(config) / 12`, rounded to the
/// nearest cent.
fn annualized_fragment(
    payee_id: &str,
    schedule: &IncomeSchedule,
    month_start: NaiveDate,
) -> Option<SalaryFragment> {
    let ScheduleTiming::Recurring(config) = &schedule.timing else {
        unreachable!("annualized_fragment is only called for Recurring schedules")
    };

    let year = month_start.year();
    let year_start = NaiveDate::from_ymd_opt(year, 1, 1).expect("valid year start");
    let year_end = NaiveDate::from_ymd_opt(year, 12, 31).expect("valid year end");

    let occurrences = schedule.timing.expand_occurrences(year_start, year_end);
    if occurrences.is_empty() {
        return None;
    }

    let yearly_total = schedule.amount.cents() as f64 * canonical_occurrences_per_year(config);
    let total = Money::from_cents((yearly_total / 12.0).round() as i64);

    Some(SalaryFragment {
        payee_id: payee_id.to_string(),
        occurrences,
        amount_per_occurrence: schedule.amount,
        total,
        source: SalarySource::Schedule,
    })
}

/// How many times a recurring schedule fires in a typical calendar year,
/// derived purely from its frequency/interval.
fn canonical_occurrences_per_year(config: &RecurringConfig) -> f64 {
    let interval = config.interval.max(1) as f64;
    match config.frequency {
        RecurringFrequency::Daily => 365.0 / interval,
        RecurringFrequency::Weekly => 52.0 / interval,
        RecurringFrequency::Monthly => {
            let dates_per_period = config.patterns.len().max(1) as f64;
            12.0 / interval * dates_per_period
        }
        RecurringFrequency::Yearly => 1.0 / interval,
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
        let salary_occurrence = budgeters
            .iter()
            .fold(Money::ZERO, |acc, b| acc + b.salary_occurrence());
        let allocations_total = budgeters
            .iter()
            .fold(Money::ZERO, |acc, b| acc + b.allocations_total());

        Self {
            state: TotalComputedSalary {
                salary_month,
                salary_occurrence,
                allocations_total,
            },
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
                    salary_occurrence: self.state.salary_occurrence,
                    allocations_total: self.state.allocations_total,
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

    fn salary_occurrence(&self) -> Money {
        salary_occurrence(&self.state.configured, &self.state.fragments)
    }

    fn allocations_total(&self) -> Money {
        allocations_total(&self.state.configured, &self.state.fragments)
    }
}

impl Budgeter<ComputedSalary> {
    pub fn fragments(&self) -> &[SalaryFragment] {
        &self.state.fragments
    }
}

/// Sum of `amount_per_occurrence` across fragments for `configured`'s salary
/// payee (normally just one fragment/schedule).
fn salary_occurrence(configured: &Configured, fragments: &[SalaryFragment]) -> Money {
    fragments
        .iter()
        .filter(|f| f.payee_id == configured.salary_payee_id)
        .fold(Money::ZERO, |acc, f| acc + f.amount_per_occurrence)
}

/// Sum of `total` across fragments for every payee other than `configured`'s
/// salary payee.
fn allocations_total(configured: &Configured, fragments: &[SalaryFragment]) -> Money {
    fragments
        .iter()
        .filter(|f| f.payee_id != configured.salary_payee_id)
        .fold(Money::ZERO, |acc, f| acc + f.total)
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

    fn salary_occurrence(&self) -> Money {
        salary_occurrence(
            &self.state.computed_salary.configured,
            &self.state.computed_salary.fragments,
        )
    }

    fn allocations_total(&self) -> Money {
        allocations_total(
            &self.state.computed_salary.configured,
            &self.state.computed_salary.fragments,
        )
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

    fn salary_occurrence(&self) -> Money {
        self.state.salary_occurrence
    }

    fn allocations_total(&self) -> Money {
        self.state.allocations_total
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

    fn salary_occurrence(&self) -> Money {
        self.state.salary_occurrence
    }

    fn allocations_total(&self) -> Money {
        self.state.allocations_total
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
