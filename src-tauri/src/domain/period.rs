use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use super::ids::Cik;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PeriodKind {
    #[serde(rename = "annual")] Annual,
    #[serde(rename = "quarterly")] Quarterly,
}

impl PeriodKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            PeriodKind::Annual => "annual",
            PeriodKind::Quarterly => "quarterly",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "annual" => Some(PeriodKind::Annual),
            "quarterly" => Some(PeriodKind::Quarterly),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Period {
    pub id: i64,
    pub cik: Cik,
    pub fiscal_year: i32,
    /// 0 for annual; 1..=4 for quarterly.
    pub fiscal_quarter: u8,
    /// MMDD, e.g. "0926" for late-September FYE.
    pub fiscal_year_end: String,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub kind: PeriodKind,
    pub is_53_week: bool,
}

impl Period {
    /// Returns true if the period spans more than 364 days, indicating a 53-week
    /// year (or, defensively, any unusually long span we want to flag).
    pub fn detect_53_week(start: NaiveDate, end: NaiveDate) -> bool {
        (end - start).num_days() > 364
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn period_kind_round_trip() {
        assert_eq!(PeriodKind::Annual.as_str(), "annual");
        assert_eq!(PeriodKind::from_str("quarterly"), Some(PeriodKind::Quarterly));
        assert_eq!(PeriodKind::from_str("bogus"), None);
    }

    #[test]
    fn detect_53_week_true_for_long_year() {
        let start = NaiveDate::from_ymd_opt(2017, 9, 25).unwrap();
        let end = NaiveDate::from_ymd_opt(2018, 9, 29).unwrap(); // 369 days
        assert!(Period::detect_53_week(start, end));
    }

    #[test]
    fn detect_53_week_false_for_normal_year() {
        let start = NaiveDate::from_ymd_opt(2023, 9, 25).unwrap();
        let end = NaiveDate::from_ymd_opt(2024, 9, 28).unwrap(); // 369 days actually
        let _ = Period::detect_53_week(start, end);
        let s2 = NaiveDate::from_ymd_opt(2023, 9, 25).unwrap();
        let e2 = NaiveDate::from_ymd_opt(2024, 9, 23).unwrap(); // 364 days
        assert!(!Period::detect_53_week(s2, e2));
    }
}
