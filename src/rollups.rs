use chrono::{DateTime, Datelike, Duration, Local, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::models::TimeEntry;
use crate::rounding::{RoundingConfig, round_seconds};

#[derive(Debug, Clone)]
pub struct DailyTotal {
    pub date: NaiveDate,
    pub seconds: i64,
}

#[derive(Debug, Clone)]
pub struct PeriodRollup {
    pub label: String,
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub days: usize,
    pub seconds: i64,
}

#[derive(Debug, Clone, Default)]
pub struct Rollups {
    pub daily: Vec<DailyTotal>,
    pub weekly: Vec<PeriodRollup>,
    pub monthly: Vec<PeriodRollup>,
    pub yearly: Vec<PeriodRollup>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum WeekStart {
    #[default]
    Monday,
    Sunday,
}

pub fn build_rollups(
    entries: &[TimeEntry],
    start: NaiveDate,
    end: NaiveDate,
    rounding: Option<&RoundingConfig>,
    week_start: WeekStart,
) -> Rollups {
    let mut totals: HashMap<NaiveDate, i64> = HashMap::new();

    for entry in entries {
        let Some(date) = parse_entry_date(entry) else {
            continue;
        };
        if date < start || date > end {
            continue;
        }
        let duration = rounding
            .map(|cfg| round_seconds(entry.duration, cfg))
            .unwrap_or(entry.duration);
        *totals.entry(date).or_insert(0) += duration;
    }

    let daily = build_daily_totals(&totals, start, end);
    let weekly = build_weekly_rollups(&daily, week_start);
    let monthly = build_monthly_rollups(&daily);
    let yearly = build_yearly_rollups(&daily);

    Rollups {
        daily,
        weekly,
        monthly,
        yearly,
    }
}

fn parse_entry_date(entry: &TimeEntry) -> Option<NaiveDate> {
    DateTime::parse_from_rfc3339(&entry.start)
        .ok()
        .map(|dt| dt.with_timezone(&Local))
        .map(|dt| dt.date_naive())
}

fn build_daily_totals(
    totals: &HashMap<NaiveDate, i64>,
    start: NaiveDate,
    end: NaiveDate,
) -> Vec<DailyTotal> {
    let mut daily = Vec::new();
    let mut current = start;
    while current <= end {
        let seconds = *totals.get(&current).unwrap_or(&0);
        daily.push(DailyTotal {
            date: current,
            seconds,
        });
        current = current.succ_opt().unwrap_or(current + Duration::days(1));
    }
    daily
}

fn build_weekly_rollups(daily: &[DailyTotal], week_start: WeekStart) -> Vec<PeriodRollup> {
    let mut rollups = Vec::new();
    let mut current_key: Option<NaiveDate> = None;
    let mut current_rollup: Option<PeriodRollup> = None;

    for day in daily {
        let key = start_of_week(day.date, week_start);
        if current_key.map(|value| value != key).unwrap_or(true) {
            if let Some(rollup) = current_rollup.take() {
                rollups.push(rollup);
            }
            let week = key.iso_week();
            let label = format!(
                "W{:02} {} ({} → {})",
                week.week(),
                week.year(),
                day.date.format("%Y-%m-%d"),
                day.date.format("%Y-%m-%d")
            );
            current_key = Some(key);
            current_rollup = Some(PeriodRollup {
                label,
                start: day.date,
                end: day.date,
                days: 0,
                seconds: 0,
            });
        }

        if let Some(rollup) = current_rollup.as_mut() {
            rollup.end = day.date;
            rollup.days += 1;
            rollup.seconds += day.seconds;
            let week = start_of_week(rollup.start, week_start).iso_week();
            rollup.label = format!(
                "W{:02} {} ({} → {})",
                week.week(),
                week.year(),
                rollup.start.format("%Y-%m-%d"),
                rollup.end.format("%Y-%m-%d")
            );
        }
    }

    if let Some(rollup) = current_rollup {
        rollups.push(rollup);
    }

    rollups
}

fn start_of_week(date: NaiveDate, week_start: WeekStart) -> NaiveDate {
    let offset = match week_start {
        WeekStart::Monday => date.weekday().num_days_from_monday() as i64,
        WeekStart::Sunday => date.weekday().num_days_from_sunday() as i64,
    };
    date - Duration::days(offset)
}

fn build_monthly_rollups(daily: &[DailyTotal]) -> Vec<PeriodRollup> {
    let mut rollups = Vec::new();
    let mut current_key: Option<(i32, u32)> = None;
    let mut current_rollup: Option<PeriodRollup> = None;

    for day in daily {
        let key = (day.date.year(), day.date.month());
        if current_key.map(|value| value != key).unwrap_or(true) {
            if let Some(rollup) = current_rollup.take() {
                rollups.push(rollup);
            }
            let label = day.date.format("%b %Y").to_string();
            current_key = Some(key);
            current_rollup = Some(PeriodRollup {
                label,
                start: day.date,
                end: day.date,
                days: 0,
                seconds: 0,
            });
        }

        if let Some(rollup) = current_rollup.as_mut() {
            rollup.end = day.date;
            rollup.days += 1;
            rollup.seconds += day.seconds;
            rollup.label = day.date.format("%b %Y").to_string();
        }
    }

    if let Some(rollup) = current_rollup {
        rollups.push(rollup);
    }

    rollups
}

fn build_yearly_rollups(daily: &[DailyTotal]) -> Vec<PeriodRollup> {
    let mut rollups = Vec::new();
    let mut current_key: Option<i32> = None;
    let mut current_rollup: Option<PeriodRollup> = None;

    for day in daily {
        let key = day.date.year();
        if current_key.map(|value| value != key).unwrap_or(true) {
            if let Some(rollup) = current_rollup.take() {
                rollups.push(rollup);
            }
            current_key = Some(key);
            current_rollup = Some(PeriodRollup {
                label: key.to_string(),
                start: day.date,
                end: day.date,
                days: 0,
                seconds: 0,
            });
        }

        if let Some(rollup) = current_rollup.as_mut() {
            rollup.end = day.date;
            rollup.days += 1;
            rollup.seconds += day.seconds;
            rollup.label = day.date.year().to_string();
        }
    }

    if let Some(rollup) = current_rollup {
        rollups.push(rollup);
    }

    rollups
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rounding::{RoundingConfig, RoundingMode};

    fn entry(start: &str, duration: i64) -> TimeEntry {
        TimeEntry {
            id: 1,
            description: Some("Test".to_string()),
            duration,
            start: start.to_string(),
            stop: Some(start.to_string()),
            project_id: None,
        }
    }

    #[test]
    fn build_rollups_includes_empty_days() {
        let entries = vec![
            entry("2026-02-03T10:00:00Z", 3600),
            entry("2026-02-04T10:00:00Z", 1800),
        ];
        let start = NaiveDate::from_ymd_opt(2026, 2, 3).unwrap();
        let end = NaiveDate::from_ymd_opt(2026, 2, 5).unwrap();

        let rollups = build_rollups(&entries, start, end, None, WeekStart::Monday);

        assert_eq!(rollups.daily.len(), 3);
        assert_eq!(rollups.daily[0].seconds, 3600);
        assert_eq!(rollups.daily[1].seconds, 1800);
        assert_eq!(rollups.daily[2].seconds, 0);
        assert_eq!(rollups.weekly.len(), 1);
        assert_eq!(rollups.weekly[0].days, 3);
        assert_eq!(rollups.weekly[0].seconds, 5400);
        assert_eq!(rollups.monthly.len(), 1);
        assert_eq!(rollups.yearly.len(), 1);
    }

    #[test]
    fn build_rollups_respects_rounding() {
        let entries = vec![
            entry("2026-02-03T10:00:00Z", 14 * 60),
            entry("2026-02-03T11:00:00Z", 14 * 60),
        ];
        let start = NaiveDate::from_ymd_opt(2026, 2, 3).unwrap();
        let end = NaiveDate::from_ymd_opt(2026, 2, 3).unwrap();
        let rounding = RoundingConfig {
            increment_minutes: 15,
            mode: RoundingMode::Closest,
        };

        let rollups = build_rollups(&entries, start, end, Some(&rounding), WeekStart::Monday);

        assert_eq!(rollups.daily.len(), 1);
        assert_eq!(rollups.daily[0].seconds, 30 * 60);
    }

    #[test]
    fn weekly_rollups_respect_sunday_start() {
        let entries = vec![entry("2026-02-01T10:00:00Z", 3600)];
        let start = NaiveDate::from_ymd_opt(2026, 2, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(2026, 2, 2).unwrap();

        let monday = build_rollups(&entries, start, end, None, WeekStart::Monday);
        let sunday = build_rollups(&entries, start, end, None, WeekStart::Sunday);

        assert_eq!(monday.weekly.len(), 2);
        assert_eq!(sunday.weekly.len(), 1);
    }

    #[test]
    fn yearly_rollups_group_by_year() {
        let entries = vec![
            entry("2025-12-31T10:00:00Z", 1800),
            entry("2026-01-01T10:00:00Z", 3600),
        ];
        let start = NaiveDate::from_ymd_opt(2025, 12, 31).unwrap();
        let end = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();

        let rollups = build_rollups(&entries, start, end, None, WeekStart::Monday);

        assert_eq!(rollups.yearly.len(), 2);
        assert_eq!(rollups.yearly[0].label, "2025");
        assert_eq!(rollups.yearly[1].label, "2026");
    }
}
