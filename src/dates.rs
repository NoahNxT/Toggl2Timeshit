use chrono::{Datelike, DateTime, Local, NaiveDate, TimeZone};
use chrono::Duration;

#[derive(Debug, Clone)]
pub struct DateRange {
    start: DateTime<Local>,
    end: DateTime<Local>,
    label: String,
}

impl DateRange {
    pub fn today() -> Self {
        let today = Local::now().date_naive();
        let start = local_datetime(today, 0, 0, 0);
        let end = local_datetime(today, 23, 59, 59);
        let label = format!("Today ({})", today.format("%Y-%m-%d"));
        Self { start, end, label }
    }

    pub fn yesterday() -> Self {
        let day = Local::now().date_naive() - Duration::days(1);
        let start = local_datetime(day, 0, 0, 0);
        let end = local_datetime(day, 23, 59, 59);
        let label = format!("Yesterday ({})", day.format("%Y-%m-%d"));
        Self { start, end, label }
    }

    pub fn from_single(date: NaiveDate) -> Self {
        let start = local_datetime(date, 0, 0, 0);
        let end = local_datetime(date, 23, 59, 59);
        let label = format!("{}", date.format("%Y-%m-%d"));
        Self { start, end, label }
    }

    pub fn from_bounds(start_date: NaiveDate, end_date: NaiveDate) -> Self {
        let start = local_datetime(start_date, 0, 0, 0);
        let end = local_datetime(end_date, 23, 59, 59);
        let label = if start_date == end_date {
            format!("{}", start_date.format("%Y-%m-%d"))
        } else {
            format!(
                "{} → {}",
                start_date.format("%Y-%m-%d"),
                end_date.format("%Y-%m-%d")
            )
        };
        Self { start, end, label }
    }

    pub fn from_options(
        date: Option<NaiveDate>,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
    ) -> Result<Self, String> {
        if let Some(date) = date {
            return Ok(Self::from_single(date));
        }

        match (start_date, end_date) {
            (Some(start), Some(end)) => {
                if start > end {
                    return Err("Start date cannot be after end date.".to_string());
                }
                Ok(Self::from_bounds(start, end))
            }
            (Some(start), None) => {
                let end = Local::now().date_naive();
                if start > end {
                    return Err("Start date cannot be after end date.".to_string());
                }
                Ok(Self::from_bounds(start, end))
            }
            (None, None) => Ok(Self::today()),
            (None, Some(_)) => Err("End date requires a start date.".to_string()),
        }
    }

    pub fn as_rfc3339(&self) -> (String, String) {
        (self.start.to_rfc3339(), self.end.to_rfc3339())
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn start_date(&self) -> NaiveDate {
        self.start.date_naive()
    }

    pub fn end_date(&self) -> NaiveDate {
        self.end.date_naive()
    }
}

pub fn parse_date(value: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| "Invalid date format. Use YYYY-MM-DD.".to_string())
}

fn local_datetime(date: NaiveDate, hour: u32, minute: u32, second: u32) -> DateTime<Local> {
    let result = Local.with_ymd_and_hms(date.year(), date.month(), date.day(), hour, minute, second);
    result
        .earliest()
        .or_else(|| result.latest())
        .unwrap_or_else(Local::now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_date_valid() {
        let date = parse_date("2026-02-03").unwrap();
        assert_eq!(date.year(), 2026);
        assert_eq!(date.month(), 2);
        assert_eq!(date.day(), 3);
    }

    #[test]
    fn parse_date_invalid() {
        assert!(parse_date("02-03-2026").is_err());
    }

    #[test]
    fn range_from_bounds_label() {
        let start = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(2026, 1, 10).unwrap();
        let range = DateRange::from_bounds(start, end);
        assert!(range.label().contains("2026-01-01"));
        assert!(range.label().contains("2026-01-10"));
    }

    #[test]
    fn yesterday_label() {
        let range = DateRange::yesterday();
        assert!(range.label().starts_with("Yesterday"));
    }
}
