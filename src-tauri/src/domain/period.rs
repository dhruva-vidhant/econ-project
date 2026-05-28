use chrono::{Datelike, NaiveDate};
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

    /// Compute the fiscal year a period belongs to, given the company's
    /// fiscal-year-end (MMDD, e.g. "1231" for calendar-year filers, "0926"
    /// for late-September FYE like Apple).
    ///
    /// Convention: a fiscal year is named for the calendar year in which
    /// it ENDS. A period ending on or before that year's FYE belongs to
    /// fiscal year `end.year()`; a period ending after the FYE belongs to
    /// fiscal year `end.year() + 1`.
    ///
    /// This is the right source of truth for `Period.fiscal_year` —
    /// **not** the SEC companyfacts `fy` field, which carries the
    /// FILING's fiscal year and shifts every period in the same 10-K's
    /// comparative window (see notes in `pipeline::orchestrator`).
    pub fn compute_fiscal_year(end: NaiveDate, fye_mmdd: &str) -> i32 {
        let (m, d) = parse_fye_mmdd(fye_mmdd).unwrap_or((12, 31));
        let fye_for_end_year =
            NaiveDate::from_ymd_opt(end.year(), m, d).unwrap_or_else(|| {
                // Defensive fallback: invalid MMDD -> calendar year
                NaiveDate::from_ymd_opt(end.year(), 12, 31).unwrap()
            });
        if end <= fye_for_end_year {
            end.year()
        } else {
            end.year() + 1
        }
    }

    /// Determine the fiscal quarter a period-end date corresponds to,
    /// given the issuer's fiscal-year-end MMDD. Returns:
    /// - `Some(0)` when the date sits on the FY end (Q4 / annual close).
    /// - `Some(1)`/`Some(2)`/`Some(3)` for Q1/Q2/Q3 ends.
    /// - `None` when the date doesn't align with any of the four
    ///   quarter-end milestones (e.g. mid-quarter snapshots).
    ///
    /// This is the right source of truth for `Period.fiscal_quarter` on
    /// **instant** facts — the SEC `fp` tag follows the *filing*'s
    /// fiscal year, so a 10-Q filed for fy=2019 carrying the
    /// 2018-12-31 opening balance still tags it `fp=Q1`. Trusting that
    /// tag would write the year-end balance into Q1's slot.
    pub fn compute_fiscal_quarter(end: NaiveDate, fye_mmdd: &str) -> Option<u8> {
        let (fye_m, _fye_d) = parse_fye_mmdd(fye_mmdd).unwrap_or((12, 31));
        let offset = (fye_m as i32 - end.month() as i32).rem_euclid(12);
        match offset {
            0 => Some(0),
            3 => Some(3),
            6 => Some(2),
            9 => Some(1),
            _ => None,
        }
    }
}

fn parse_fye_mmdd(s: &str) -> Option<(u32, u32)> {
    if s.len() != 4 { return None; }
    let m: u32 = s[0..2].parse().ok()?;
    let d: u32 = s[2..4].parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) { return None; }
    Some((m, d))
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
    fn compute_fiscal_year_calendar_filer() {
        // FYE Dec 31: every period within a calendar year is FY=year
        let fye = "1231";
        assert_eq!(Period::compute_fiscal_year(NaiveDate::from_ymd_opt(2017, 12, 31).unwrap(), fye), 2017);
        assert_eq!(Period::compute_fiscal_year(NaiveDate::from_ymd_opt(2017, 3, 31).unwrap(), fye), 2017);
        assert_eq!(Period::compute_fiscal_year(NaiveDate::from_ymd_opt(2018, 1, 1).unwrap(), fye), 2018);
    }

    #[test]
    fn compute_fiscal_year_september_fye() {
        // FYE Sept 30 (Apple): periods after Sept 30 belong to NEXT fiscal year
        let fye = "0930";
        assert_eq!(Period::compute_fiscal_year(NaiveDate::from_ymd_opt(2017, 9, 30).unwrap(), fye), 2017);
        assert_eq!(Period::compute_fiscal_year(NaiveDate::from_ymd_opt(2017, 12, 31).unwrap(), fye), 2018);
        assert_eq!(Period::compute_fiscal_year(NaiveDate::from_ymd_opt(2018, 3, 31).unwrap(), fye), 2018);
        assert_eq!(Period::compute_fiscal_year(NaiveDate::from_ymd_opt(2018, 9, 30).unwrap(), fye), 2018);
        assert_eq!(Period::compute_fiscal_year(NaiveDate::from_ymd_opt(2018, 10, 1).unwrap(), fye), 2019);
    }

    #[test]
    fn compute_fiscal_year_handles_invalid_fye() {
        // Garbage MMDD -> treat as Dec 31 calendar-year filer.
        assert_eq!(Period::compute_fiscal_year(NaiveDate::from_ymd_opt(2017, 6, 30).unwrap(), "x"), 2017);
        assert_eq!(Period::compute_fiscal_year(NaiveDate::from_ymd_opt(2017, 6, 30).unwrap(), ""), 2017);
    }

    #[test]
    fn compute_fiscal_quarter_calendar_filer() {
        let fye = "1231";
        assert_eq!(Period::compute_fiscal_quarter(NaiveDate::from_ymd_opt(2018, 3, 31).unwrap(), fye), Some(1));
        assert_eq!(Period::compute_fiscal_quarter(NaiveDate::from_ymd_opt(2018, 6, 30).unwrap(), fye), Some(2));
        assert_eq!(Period::compute_fiscal_quarter(NaiveDate::from_ymd_opt(2018, 9, 30).unwrap(), fye), Some(3));
        assert_eq!(Period::compute_fiscal_quarter(NaiveDate::from_ymd_opt(2018, 12, 31).unwrap(), fye), Some(0));
        assert_eq!(Period::compute_fiscal_quarter(NaiveDate::from_ymd_opt(2018, 5, 15).unwrap(), fye), None);
    }

    #[test]
    fn compute_fiscal_quarter_september_fye() {
        // Apple-style FYE 09-26: Q1 ends Dec, Q2 Mar, Q3 Jun, FY Sep.
        let fye = "0930";
        assert_eq!(Period::compute_fiscal_quarter(NaiveDate::from_ymd_opt(2017, 12, 30).unwrap(), fye), Some(1));
        assert_eq!(Period::compute_fiscal_quarter(NaiveDate::from_ymd_opt(2018, 3, 31).unwrap(), fye), Some(2));
        assert_eq!(Period::compute_fiscal_quarter(NaiveDate::from_ymd_opt(2018, 6, 30).unwrap(), fye), Some(3));
        assert_eq!(Period::compute_fiscal_quarter(NaiveDate::from_ymd_opt(2018, 9, 30).unwrap(), fye), Some(0));
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
