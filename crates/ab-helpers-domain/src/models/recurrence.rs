use chrono::{Datelike, Duration, NaiveDate, Weekday};

/// A day-of-month, guaranteed to be in `1..=31` (clamped to the month's
/// actual length when turned into a date).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DayOfMonth(u32);

impl DayOfMonth {
    pub fn get(self) -> u32 {
        self.0
    }
}

impl TryFrom<u32> for DayOfMonth {
    type Error = RecurrenceRangeError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        if (1..=31).contains(&value) {
            Ok(Self(value))
        } else {
            Err(RecurrenceRangeError::DayOfMonth(value))
        }
    }
}

/// The nth occurrence of a weekday within a month, guaranteed to be in
/// `1..=5`. If that occurrence doesn't exist in a given month (e.g. a 5th
/// Friday), that month simply contributes no date for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NthOccurrence(u32);

impl NthOccurrence {
    pub fn get(self) -> u32 {
        self.0
    }
}

impl TryFrom<u32> for NthOccurrence {
    type Error = RecurrenceRangeError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        if (1..=5).contains(&value) {
            Ok(Self(value))
        } else {
            Err(RecurrenceRangeError::NthOccurrence(value))
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RecurrenceRangeError {
    #[error("day-of-month must be between 1 and 31, got {0}")]
    DayOfMonth(u32),
    #[error("nth occurrence must be between 1 and 5, got {0}")]
    NthOccurrence(u32),
}

/// A single pattern entry within a [`RecurringConfig`]'s `patterns` (monthly
/// frequency only - Actual's own `recurConfigToRSchedule` ignores `patterns`
/// for daily/weekly/yearly).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecurringPattern {
    DayOfMonth(DayOfMonth),
    NthWeekday { weekday: Weekday, nth: NthOccurrence },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeekendSolveMode {
    Before,
    After,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecurringEnd {
    Never,
    AfterOccurrences(u32),
    OnDate(NaiveDate),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecurringFrequency {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

/// A recurring schedule's timing, mirroring Actual's `RecurringConfig`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecurringConfig {
    pub frequency: RecurringFrequency,
    /// Step between periods; `0` is treated as `1`.
    pub interval: u32,
    pub patterns: Vec<RecurringPattern>,
    pub skip_weekend: bool,
    pub weekend_solve_mode: WeekendSolveMode,
    pub start: NaiveDate,
    pub end: RecurringEnd,
}

/// Either a recurring schedule, or a single fixed date (a schedule with a
/// plain, non-recurring `date` condition).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScheduleTiming {
    Recurring(RecurringConfig),
    OneOff(NaiveDate),
}

/// Safety cap on periods walked from `config.start`, so a malformed or
/// very-long-lived daily schedule can't loop indefinitely (20_000 daily
/// periods is ~54 years).
const MAX_PERIODS: i64 = 20_000;

impl ScheduleTiming {
    /// Expands `self` into the concrete dates it falls on within
    /// `[range_start, range_end]` (inclusive).
    pub fn expand_occurrences(&self, range_start: NaiveDate, range_end: NaiveDate) -> Vec<NaiveDate> {
        if range_end < range_start {
            return Vec::new();
        }

        match self {
            ScheduleTiming::OneOff(date) => {
                if *date >= range_start && *date <= range_end {
                    vec![*date]
                } else {
                    Vec::new()
                }
            }
            ScheduleTiming::Recurring(config) => expand_recurring(config, range_start, range_end),
        }
    }
}

fn expand_recurring(config: &RecurringConfig, range_start: NaiveDate, range_end: NaiveDate) -> Vec<NaiveDate> {
    let interval = config.interval.max(1);
    let mut results = Vec::new();
    let mut occurrence_number: u32 = 0;

    'periods: for period_index in 0..=MAX_PERIODS {
        let period_dates = raw_dates_for_period(config, interval, period_index);
        if period_dates.is_empty() {
            continue;
        }

        let period_min = *period_dates.iter().min().expect("checked non-empty above");
        if period_min > range_end + Duration::days(3) {
            break;
        }

        for raw_date in period_dates {
            occurrence_number += 1;

            if let RecurringEnd::OnDate(end) = config.end
                && raw_date > end
            {
                break 'periods;
            }
            if let RecurringEnd::AfterOccurrences(n) = config.end
                && occurrence_number > n
            {
                break 'periods;
            }

            let adjusted = apply_weekend_adjustment(config, raw_date);
            if adjusted >= range_start && adjusted <= range_end {
                results.push(adjusted);
            }
        }
    }

    results.sort();
    results.dedup();
    results
}

/// Raw (pre-weekend-adjustment) occurrence date(s) for the `period_index`th
/// period (0-based, counted from `config.start`) of a recurrence stepping by
/// `interval` per the config's frequency.
fn raw_dates_for_period(config: &RecurringConfig, interval: u32, period_index: i64) -> Vec<NaiveDate> {
    match config.frequency {
        RecurringFrequency::Daily => {
            vec![config.start + Duration::days(period_index * interval as i64)]
        }
        RecurringFrequency::Weekly => {
            vec![config.start + Duration::days(period_index * interval as i64 * 7)]
        }
        RecurringFrequency::Monthly => {
            let start_month_index = config.start.year() as i64 * 12 + (config.start.month() as i64 - 1);
            let month_index = start_month_index + period_index * interval as i64;
            let year = month_index.div_euclid(12) as i32;
            let month = (month_index.rem_euclid(12) + 1) as u32;

            if config.patterns.is_empty() {
                vec![clamp_day(year, month, config.start.day())]
            } else {
                let mut dates: Vec<NaiveDate> = config
                    .patterns
                    .iter()
                    .filter_map(|p| match p {
                        RecurringPattern::DayOfMonth(day) => Some(clamp_day(year, month, day.get())),
                        RecurringPattern::NthWeekday { weekday, nth } => {
                            nth_weekday_of_month(year, month, *weekday, *nth)
                        }
                    })
                    .collect();
                dates.sort();
                dates.dedup();
                dates
            }
        }
        RecurringFrequency::Yearly => {
            let year = config.start.year() + (period_index * interval as i64) as i32;
            vec![clamp_month_day(year, config.start.month(), config.start.day())]
        }
    }
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let next_month_first = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    }
    .expect("valid next-month-first date");
    let this_month_first = NaiveDate::from_ymd_opt(year, month, 1).expect("valid month-first date");
    (next_month_first - this_month_first).num_days() as u32
}

fn clamp_day(year: i32, month: u32, day: u32) -> NaiveDate {
    let day = day.clamp(1, days_in_month(year, month));
    NaiveDate::from_ymd_opt(year, month, day).expect("clamped day is valid")
}

fn clamp_month_day(year: i32, month: u32, day: u32) -> NaiveDate {
    clamp_day(year, month, day)
}

fn nth_weekday_of_month(year: i32, month: u32, weekday: Weekday, nth: NthOccurrence) -> Option<NaiveDate> {
    let first_of_month = NaiveDate::from_ymd_opt(year, month, 1)?;
    let first_weekday_offset =
        (7 + weekday.num_days_from_monday() as i64 - first_of_month.weekday().num_days_from_monday() as i64) % 7;
    let day = 1 + first_weekday_offset + (nth.get() as i64 - 1) * 7;
    if day > days_in_month(year, month) as i64 {
        return None;
    }
    NaiveDate::from_ymd_opt(year, month, day as u32)
}

fn apply_weekend_adjustment(config: &RecurringConfig, date: NaiveDate) -> NaiveDate {
    if !config.skip_weekend {
        return date;
    }
    match date.weekday() {
        Weekday::Sat => match config.weekend_solve_mode {
            WeekendSolveMode::Before => date - Duration::days(1),
            WeekendSolveMode::After => date + Duration::days(2),
        },
        Weekday::Sun => match config.weekend_solve_mode {
            WeekendSolveMode::Before => date - Duration::days(2),
            WeekendSolveMode::After => date + Duration::days(1),
        },
        _ => date,
    }
}

