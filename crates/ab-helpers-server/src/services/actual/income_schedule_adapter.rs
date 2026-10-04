//! Translates Actual's `Schedule`/`RecurringConfig` wire shapes into
//! [`ab_helpers_domain`]'s schedule-agnostic [`IncomeSchedule`] model.

use ab_helpers_domain::{
    DayOfMonth, IncomeSchedule, Money, NthOccurrence, RecurringConfig as DomainRecurringConfig,
    RecurringEnd, RecurringFrequency as DomainRecurringFrequency,
    RecurringPattern as DomainRecurringPattern, ScheduleTiming,
    WeekendSolveMode as DomainWeekendSolveMode,
};
use actual::{
    RecurringConfig, RecurringEndMode, RecurringFrequency, RecurringPattern, RecurringPatternType,
    Schedule, ScheduleDate, WeekendSolveMode,
};
use chrono::NaiveDate;

pub fn to_income_schedule(schedule: Schedule) -> Option<IncomeSchedule> {
    let payee_id = schedule.payee_id?;
    let amount = Money::from_cents(schedule.amount.estimate());
    let timing = match schedule.date {
        ScheduleDate::Recurring(config) => {
            ScheduleTiming::Recurring(to_domain_recurring_config(config)?)
        }
        ScheduleDate::Fixed(date_str) => {
            ScheduleTiming::OneOff(NaiveDate::parse_from_str(&date_str, "%Y-%m-%d").ok()?)
        }
    };
    Some(IncomeSchedule {
        payee_id,
        amount,
        timing,
    })
}

fn to_domain_recurring_config(config: RecurringConfig) -> Option<DomainRecurringConfig> {
    let start = NaiveDate::parse_from_str(&config.start, "%Y-%m-%d").ok()?;

    let end = match config.end_mode {
        Some(RecurringEndMode::AfterOccurrences) => config
            .end_occurrences
            .map(RecurringEnd::AfterOccurrences)
            .unwrap_or(RecurringEnd::Never),
        Some(RecurringEndMode::OnDate) => config
            .end_date
            .as_deref()
            .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
            .map(RecurringEnd::OnDate)
            .unwrap_or(RecurringEnd::Never),
        _ => RecurringEnd::Never,
    };

    let frequency = match config.frequency {
        RecurringFrequency::Daily => DomainRecurringFrequency::Daily,
        RecurringFrequency::Weekly => DomainRecurringFrequency::Weekly,
        RecurringFrequency::Monthly => DomainRecurringFrequency::Monthly,
        RecurringFrequency::Yearly => DomainRecurringFrequency::Yearly,
    };

    let patterns = config
        .patterns
        .unwrap_or_default()
        .into_iter()
        .filter_map(to_domain_pattern)
        .collect();

    let weekend_solve_mode = match config.weekend_solve_mode {
        Some(WeekendSolveMode::Before) => DomainWeekendSolveMode::Before,
        _ => DomainWeekendSolveMode::After,
    };

    Some(DomainRecurringConfig {
        frequency,
        interval: config.interval.unwrap_or(1),
        patterns,
        skip_weekend: config.skip_weekend.unwrap_or(false),
        weekend_solve_mode,
        start,
        end,
    })
}

fn to_domain_pattern(pattern: RecurringPattern) -> Option<DomainRecurringPattern> {
    let raw = pattern.value.max(0) as u32;
    if pattern.pattern_type == RecurringPatternType::Day {
        return DayOfMonth::try_from(raw)
            .ok()
            .map(DomainRecurringPattern::DayOfMonth);
    }
    let weekday = match pattern.pattern_type {
        RecurringPatternType::Day => unreachable!("handled above"),
        RecurringPatternType::Su => chrono::Weekday::Sun,
        RecurringPatternType::Mo => chrono::Weekday::Mon,
        RecurringPatternType::Tu => chrono::Weekday::Tue,
        RecurringPatternType::We => chrono::Weekday::Wed,
        RecurringPatternType::Th => chrono::Weekday::Thu,
        RecurringPatternType::Fr => chrono::Weekday::Fri,
        RecurringPatternType::Sa => chrono::Weekday::Sat,
    };
    NthOccurrence::try_from(raw)
        .ok()
        .map(|nth| DomainRecurringPattern::NthWeekday { weekday, nth })
}
