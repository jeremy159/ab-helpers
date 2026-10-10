use std::collections::HashMap;

use chrono::NaiveDate;

use crate::{
    BudgetMode, BudgetSummary, Budgeter, BudgeterExt, ExpenseCategory, IncomeSchedule, Money,
    RecurringConfig, RecurringEnd, RecurringFrequency, SalarySource, ScheduleTiming,
    WeekendSolveMode,
};

fn schedule(payee_id: &str, amount_cents: i64, day: NaiveDate) -> IncomeSchedule {
    IncomeSchedule {
        payee_id: payee_id.to_string(),
        amount: Money::from_cents(amount_cents),
        timing: ScheduleTiming::OneOff(day),
    }
}

fn category(name: &str, amount_cents: i64) -> ExpenseCategory {
    ExpenseCategory {
        id: name.to_string(),
        name: name.to_string(),
        projected_amount: Money::from_cents(amount_cents),
    }
}

#[test]
fn splits_common_expenses_proportionally_by_income() {
    let month_start = NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();
    let month_end = NaiveDate::from_ymd_opt(2026, 3, 31).unwrap();

    let jeremy = Budgeter::new("Jeremy", "payee-jeremy", vec![]).compute_salary(
        &[schedule(
            "payee-jeremy",
            400_000,
            NaiveDate::from_ymd_opt(2026, 3, 15).unwrap(),
        )],
        &HashMap::new(),
        month_start,
        month_end,
        BudgetMode::Actual,
    );
    let sandryne = Budgeter::new("Sandryne", "payee-sandryne", vec![]).compute_salary(
        &[schedule(
            "payee-sandryne",
            600_000,
            NaiveDate::from_ymd_opt(2026, 3, 15).unwrap(),
        )],
        &HashMap::new(),
        month_start,
        month_end,
        BudgetMode::Actual,
    );

    assert_eq!(jeremy.salary_month(), Money::from_cents(400_000));
    assert_eq!(sandryne.salary_month(), Money::from_cents(600_000));

    let categories = vec![
        category("Groceries", 100_000),
        category("Auto - Jeremy", 20_000),
        category("Phone - Sandryne", 5_000),
    ];

    let summary = BudgetSummary::build(vec![jeremy, sandryne], &categories);

    assert_eq!(summary.total.salary_month(), Money::from_cents(1_000_000));
    assert_eq!(summary.total.common_expenses(), Money::from_cents(100_000));
    assert_eq!(
        summary.total.individual_expenses(),
        Money::from_cents(25_000)
    );

    let jeremy = summary
        .budgeters
        .iter()
        .find(|b| b.name() == "Jeremy")
        .unwrap();
    let sandryne = summary
        .budgeters
        .iter()
        .find(|b| b.name() == "Sandryne")
        .unwrap();

    assert!((jeremy.proportion() - 0.4).abs() < 1e-9);
    assert!((sandryne.proportion() - 0.6).abs() < 1e-9);

    // common split 40/60 of 100_000: 40_000 / 60_000
    assert_eq!(jeremy.common_expenses(), Money::from_cents(40_000));
    assert_eq!(sandryne.common_expenses(), Money::from_cents(60_000));

    assert_eq!(jeremy.individual_expenses(), Money::from_cents(20_000));
    assert_eq!(sandryne.individual_expenses(), Money::from_cents(5_000));

    assert_eq!(
        jeremy.left_over(),
        Money::from_cents(400_000 - 40_000 - 20_000)
    );
    assert_eq!(
        sandryne.left_over(),
        Money::from_cents(600_000 - 60_000 - 5_000)
    );
}

fn biweekly_schedule(payee_id: &str, amount_cents: i64, start: NaiveDate) -> IncomeSchedule {
    IncomeSchedule {
        payee_id: payee_id.to_string(),
        amount: Money::from_cents(amount_cents),
        timing: ScheduleTiming::Recurring(RecurringConfig {
            frequency: RecurringFrequency::Weekly,
            interval: 2,
            patterns: Vec::new(),
            skip_weekend: false,
            weekend_solve_mode: WeekendSolveMode::After,
            start,
            end: RecurringEnd::Never,
        }),
    }
}

#[test]
fn annualized_salary_smooths_a_three_paycheque_month() {
    // Biweekly starting Fri 2026-01-02: 2026-01 has 3 occurrences (Jan 2,
    // 16, 30) while the calendar year has 26, so "actual" and "annualized"
    // should disagree for this month.
    let month_start = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
    let month_end = NaiveDate::from_ymd_opt(2026, 1, 31).unwrap();
    let schedule = biweekly_schedule(
        "payee-jeremy",
        200_000,
        NaiveDate::from_ymd_opt(2026, 1, 2).unwrap(),
    );

    let actual = Budgeter::new("Jeremy", "payee-jeremy", vec![]).compute_salary(
        std::slice::from_ref(&schedule),
        &HashMap::new(),
        month_start,
        month_end,
        BudgetMode::Actual,
    );
    assert_eq!(actual.salary_month(), Money::from_cents(600_000));

    let annualized = Budgeter::new("Jeremy", "payee-jeremy", vec![]).compute_salary(
        &[schedule],
        &HashMap::new(),
        month_start,
        month_end,
        BudgetMode::Annualized,
    );
    // 26 occurrences/year * 200_000 / 12 = 433_333.33 -> rounds to 433_333.
    assert_eq!(annualized.salary_month(), Money::from_cents(433_333));
}

#[test]
fn annualized_salary_ignores_a_27_pay_date_year() {
    // Biweekly starting 2024-08-01: 2026 happens to contain 27 pay dates
    // (both 2026-01-01 and 2026-12-31 land on the 14-day cadence), but the
    // annualized projection should still use the canonical 26/year for a
    // biweekly schedule, not the 27 actually falling in this year.
    let month_start = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
    let month_end = NaiveDate::from_ymd_opt(2026, 1, 31).unwrap();
    let schedule = biweekly_schedule(
        "payee-jeremy",
        284_998,
        NaiveDate::from_ymd_opt(2024, 8, 1).unwrap(),
    );

    let annualized = Budgeter::new("Jeremy", "payee-jeremy", vec![]).compute_salary(
        &[schedule],
        &HashMap::new(),
        month_start,
        month_end,
        BudgetMode::Annualized,
    );

    // 26 occurrences/year * 284_998 / 12 = 617_495.67 -> rounds to 617_496,
    // not 27 * 284_998 / 12 = 641_245.5.
    assert_eq!(annualized.salary_month(), Money::from_cents(617_496));
}

#[test]
fn zero_total_income_yields_zero_proportion_not_a_panic() {
    let month_start = NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();
    let month_end = NaiveDate::from_ymd_opt(2026, 3, 31).unwrap();

    let jeremy = Budgeter::new("Jeremy", "payee-jeremy", vec![]).compute_salary(
        &[],
        &HashMap::new(),
        month_start,
        month_end,
        BudgetMode::Actual,
    );
    assert_eq!(jeremy.salary_month(), Money::ZERO);

    let categories = vec![category("Groceries", 100_000)];
    let summary = BudgetSummary::build(vec![jeremy], &categories);

    let jeremy = &summary.budgeters[0];
    assert_eq!(jeremy.proportion(), 0.0);
    assert_eq!(jeremy.common_expenses(), Money::ZERO);
    assert_eq!(jeremy.left_over(), Money::ZERO);
}

#[test]
fn falls_back_to_transaction_history_when_payee_has_no_schedule() {
    let month_start = NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();
    let month_end = NaiveDate::from_ymd_opt(2026, 3, 31).unwrap();

    let mut fallback = HashMap::new();
    fallback.insert("payee-jeremy".to_string(), Money::from_cents(12_345));

    let jeremy = Budgeter::new("Jeremy", "payee-jeremy", vec![]).compute_salary(
        &[],
        &fallback,
        month_start,
        month_end,
        BudgetMode::Actual,
    );

    assert_eq!(jeremy.salary_month(), Money::from_cents(12_345));
    assert_eq!(
        jeremy.fragments()[0].source,
        SalarySource::TransactionHistoryFallback
    );
}

#[test]
fn splits_salary_occurrence_from_allocations() {
    let month_start = NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();
    let month_end = NaiveDate::from_ymd_opt(2026, 3, 31).unwrap();

    let mut fallback = HashMap::new();
    fallback.insert("payee-ace".to_string(), Money::from_cents(50_000));

    let sandryne = Budgeter::new("Sandryne", "payee-salary", vec!["payee-ace".to_string()])
        .compute_salary(
            &[schedule(
                "payee-salary",
                200_000,
                NaiveDate::from_ymd_opt(2026, 3, 15).unwrap(),
            )],
            &fallback,
            month_start,
            month_end,
            BudgetMode::Actual,
        );

    assert_eq!(sandryne.salary_month(), Money::from_cents(250_000));
    assert_eq!(sandryne.salary_occurrence(), Money::from_cents(200_000));
    assert_eq!(sandryne.allocations_total(), Money::from_cents(50_000));

    let summary = BudgetSummary::build(vec![sandryne], &[]);
    assert_eq!(
        summary.total.salary_occurrence(),
        Money::from_cents(200_000)
    );
    assert_eq!(summary.total.allocations_total(), Money::from_cents(50_000));
}

#[test]
fn payees_without_schedule_excludes_scheduled_payees() {
    let schedules = [schedule(
        "payee-jeremy",
        400_000,
        NaiveDate::from_ymd_opt(2026, 3, 15).unwrap(),
    )];

    let missing =
        IncomeSchedule::payees_without_schedule(&schedules, ["payee-jeremy", "payee-sandryne"]);

    assert_eq!(missing, vec!["payee-sandryne"]);
}

#[test]
fn resolve_projected_amount_prefers_projection_then_budgeted_then_zero() {
    assert_eq!(
        ExpenseCategory::resolve_projected_amount(Some(100), Some(200)),
        Money::from_cents(100)
    );
    assert_eq!(
        ExpenseCategory::resolve_projected_amount(None, Some(200)),
        Money::from_cents(200)
    );
    assert_eq!(
        ExpenseCategory::resolve_projected_amount(None, None),
        Money::ZERO
    );
}
