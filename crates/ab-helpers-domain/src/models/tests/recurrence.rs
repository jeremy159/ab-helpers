use chrono::{NaiveDate, Weekday};

use crate::{
    NthOccurrence, RecurringConfig, RecurringEnd, RecurringFrequency, RecurringPattern, ScheduleTiming,
    WeekendSolveMode,
};

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

fn base(frequency: RecurringFrequency, start: NaiveDate) -> RecurringConfig {
    RecurringConfig {
        frequency,
        interval: 1,
        patterns: Vec::new(),
        skip_weekend: false,
        weekend_solve_mode: WeekendSolveMode::After,
        start,
        end: RecurringEnd::Never,
    }
}

fn occurrences_of(config: RecurringConfig, range_start: NaiveDate, range_end: NaiveDate) -> Vec<NaiveDate> {
    ScheduleTiming::Recurring(config).expand_occurrences(range_start, range_end)
}

#[test]
fn weekly_occurrences_within_a_month() {
    // Every Friday, starting Fri 2026-01-02.
    let config = base(RecurringFrequency::Weekly, d(2026, 1, 2));
    let occurrences = occurrences_of(config, d(2026, 3, 1), d(2026, 3, 31));
    assert_eq!(
        occurrences,
        vec![d(2026, 3, 6), d(2026, 3, 13), d(2026, 3, 20), d(2026, 3, 27)]
    );
}

#[test]
fn biweekly_interval_skips_alternating_weeks() {
    let mut config = base(RecurringFrequency::Weekly, d(2026, 1, 2));
    config.interval = 2;
    let occurrences = occurrences_of(config, d(2026, 3, 1), d(2026, 3, 31));
    // Fridays every 2 weeks from 2026-01-02: ... 2026-02-27, 2026-03-13, 2026-03-27
    assert_eq!(occurrences, vec![d(2026, 3, 13), d(2026, 3, 27)]);
}

#[test]
fn monthly_day_of_month_clamped_to_short_months() {
    // 31st of each month, clamped in February.
    let config = base(RecurringFrequency::Monthly, d(2026, 1, 31));
    let occurrences = occurrences_of(config, d(2026, 2, 1), d(2026, 2, 28));
    assert_eq!(occurrences, vec![d(2026, 2, 28)]);
}

#[test]
fn monthly_nth_weekday_pattern() {
    let mut config = base(RecurringFrequency::Monthly, d(2026, 1, 1));
    config.patterns = vec![RecurringPattern::NthWeekday {
        weekday: Weekday::Tue,
        nth: NthOccurrence::try_from(2).unwrap(),
    }];
    // 2nd Tuesday of March 2026 is 2026-03-10.
    let occurrences = occurrences_of(config, d(2026, 3, 1), d(2026, 3, 31));
    assert_eq!(occurrences, vec![d(2026, 3, 10)]);
}

#[test]
fn respects_end_after_n_occurrences() {
    let mut config = base(RecurringFrequency::Monthly, d(2026, 1, 15));
    config.end = RecurringEnd::AfterOccurrences(2);
    // Occurrences: 2026-01-15, 2026-02-15 - none left by April.
    let occurrences = occurrences_of(config, d(2026, 3, 1), d(2026, 4, 30));
    assert!(occurrences.is_empty());
}

#[test]
fn skip_weekend_shifts_to_monday() {
    // 2026-03-07 is a Saturday.
    let mut config = base(RecurringFrequency::Monthly, d(2026, 3, 7));
    config.skip_weekend = true;
    config.weekend_solve_mode = WeekendSolveMode::After;
    let occurrences = occurrences_of(config, d(2026, 3, 1), d(2026, 3, 31));
    assert_eq!(occurrences, vec![d(2026, 3, 9)]);
}

#[test]
fn one_off_schedule_only_matches_its_exact_date() {
    let timing = ScheduleTiming::OneOff(d(2026, 3, 15));
    assert_eq!(
        timing.expand_occurrences(d(2026, 3, 1), d(2026, 3, 31)),
        vec![d(2026, 3, 15)]
    );
    assert!(timing.expand_occurrences(d(2026, 4, 1), d(2026, 4, 30)).is_empty());
}
