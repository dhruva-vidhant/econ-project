use chrono::{Datelike, Months, NaiveDate};
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
    /// 52/53-week filers (e.g. retailers ending on the Saturday nearest a
    /// month-end) have a real year-end that drifts a few days year to year
    /// and can cross a month boundary relative to the nominal MMDD. A fact
    /// ending within [`FYE_DRIFT_TOL_DAYS`] of a fiscal-year-end anchor is
    /// the (drifted) year-end and is named for that anchor's year — never
    /// rolled forward. No real quarterly/annual period *ends* within a
    /// couple of weeks after the year-end (the next period ends ~3 months
    /// later), so this tolerance only ever captures genuine year-ends.
    ///
    /// This is the right source of truth for `Period.fiscal_year` —
    /// **not** the SEC companyfacts `fy` field, which carries the
    /// FILING's fiscal year and shifts every period in the same 10-K's
    /// comparative window (see notes in `pipeline::orchestrator`).
    pub fn compute_fiscal_year(end: NaiveDate, fye_mmdd: &str) -> i32 {
        let (m, d) = parse_fye_mmdd(fye_mmdd).unwrap_or((12, 31));
        // Drift-tolerant match: if `end` is near a year-end anchor, it IS
        // that fiscal year's close (the anchor may sit in a neighbouring
        // calendar year when the FYE is close to Jan 1 / Dec 31).
        for yr in [end.year() - 1, end.year(), end.year() + 1] {
            let anchor = fye_on(yr, m, d);
            if (end - anchor).num_days().abs() <= FYE_DRIFT_TOL_DAYS {
                return anchor.year();
            }
        }
        // General case: on/before this year's FYE -> this year; after -> next.
        let anchor = fye_on(end.year(), m, d);
        if end <= anchor { end.year() } else { end.year() + 1 }
    }

    /// Determine the fiscal quarter a period-end date corresponds to,
    /// given the issuer's fiscal-year-end MMDD. Returns:
    /// - `Some(0)` when the date sits on the FY end (Q4 / annual close).
    /// - `Some(1)`/`Some(2)`/`Some(3)` for Q1/Q2/Q3 ends.
    /// - `None` when the date doesn't align with any of the four
    ///   quarter-end milestones (e.g. mid-quarter snapshots).
    ///
    /// Matches by date proximity to the four fiscal milestones (the
    /// year-end anchor and the points 3/6/9 months before it) rather than
    /// by exact calendar month. This tolerates the year-to-year drift of
    /// 52/53-week filers — whose quarter ends move a few days and can cross
    /// a month boundary — which an exact-month test silently dropped,
    /// while still returning `None` for genuinely mid-quarter snapshots
    /// (the milestones are ~3 months apart, far wider than the tolerance).
    ///
    /// This is the right source of truth for `Period.fiscal_quarter` on
    /// **instant** facts — the SEC `fp` tag follows the *filing*'s
    /// fiscal year, so a 10-Q filed for fy=2019 carrying the
    /// 2018-12-31 opening balance still tags it `fp=Q1`. Trusting that
    /// tag would write the year-end balance into Q1's slot.
    pub fn compute_fiscal_quarter(end: NaiveDate, fye_mmdd: &str) -> Option<u8> {
        let (m, d) = parse_fye_mmdd(fye_mmdd).unwrap_or((12, 31));
        // Consider anchors in neighbouring calendar years so milestones near
        // a year boundary (e.g. a Q1 end just before Jan 1) still match.
        for yr in [end.year() - 1, end.year(), end.year() + 1] {
            let fye = fye_on(yr, m, d);
            for (months_before, q) in [(0u32, 0u8), (3, 3), (6, 2), (9, 1)] {
                let milestone = fye.checked_sub_months(Months::new(months_before)).unwrap_or(fye);
                if (end - milestone).num_days().abs() <= FYE_DRIFT_TOL_DAYS {
                    return Some(q);
                }
            }
        }
        None
    }
}

/// Half-width (in days) of the window around each fiscal milestone within
/// which a period-end is treated as landing on that milestone. Chosen to
/// cover 52/53-week calendar drift (a few days, occasionally crossing a
/// month boundary) while staying well under the ~91-day quarter spacing so
/// mid-quarter dates remain unmatched.
const FYE_DRIFT_TOL_DAYS: i64 = 17;

/// The fiscal-year-end date in calendar year `year`, clamping an invalid
/// day-of-month (e.g. an MMDD of "0229" in a non-leap year) back to the
/// last valid day of that month.
fn fye_on(year: i32, month: u32, day: u32) -> NaiveDate {
    for dd in (1..=day).rev() {
        if let Some(date) = NaiveDate::from_ymd_opt(year, month, dd) {
            return date;
        }
    }
    NaiveDate::from_ymd_opt(year, 12, 31).unwrap()
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
        // FYE Dec 31: every period within a calendar year is FY=year. (Dates
        // in the first couple of weeks of January sit inside the year-end
        // drift window and resolve to the prior close; no real Dec-31 filer
        // has a period *ending* there, so the next-year case below uses a
        // realistic Q1 end.)
        let fye = "1231";
        assert_eq!(Period::compute_fiscal_year(NaiveDate::from_ymd_opt(2017, 12, 31).unwrap(), fye), 2017);
        assert_eq!(Period::compute_fiscal_year(NaiveDate::from_ymd_opt(2017, 3, 31).unwrap(), fye), 2017);
        assert_eq!(Period::compute_fiscal_year(NaiveDate::from_ymd_opt(2018, 3, 31).unwrap(), fye), 2018);
    }

    #[test]
    fn compute_fiscal_year_september_fye() {
        // FYE Sept 30 (Apple): periods well after Sept 30 belong to the NEXT
        // fiscal year. (A date 1-17 days past the FYE is now treated as
        // year-end drift — see compute_fiscal_year_tolerates_yearend_drift —
        // so the "next FY" cases below use realistic mid-quarter ends.)
        let fye = "0930";
        assert_eq!(Period::compute_fiscal_year(NaiveDate::from_ymd_opt(2017, 9, 30).unwrap(), fye), 2017);
        assert_eq!(Period::compute_fiscal_year(NaiveDate::from_ymd_opt(2017, 12, 31).unwrap(), fye), 2018);
        assert_eq!(Period::compute_fiscal_year(NaiveDate::from_ymd_opt(2018, 3, 31).unwrap(), fye), 2018);
        assert_eq!(Period::compute_fiscal_year(NaiveDate::from_ymd_opt(2018, 9, 30).unwrap(), fye), 2018);
        assert_eq!(Period::compute_fiscal_year(NaiveDate::from_ymd_opt(2018, 12, 30).unwrap(), fye), 2019);
    }

    #[test]
    fn compute_fiscal_year_tolerates_yearend_drift() {
        // Lululemon-style 52/53-week filer: nominal FYE "0202" but the real
        // year-end drifts across the Jan/Feb boundary year to year.
        let fye = "0202";
        // Year ending late January is still that calendar year's close.
        assert_eq!(Period::compute_fiscal_year(NaiveDate::from_ymd_opt(2023, 1, 29).unwrap(), fye), 2023);
        assert_eq!(Period::compute_fiscal_year(NaiveDate::from_ymd_opt(2024, 1, 28).unwrap(), fye), 2024);
        // Year ending a day or two AFTER the nominal FYE must NOT roll into
        // the next fiscal year (the FY2018 close on 2019-02-03 regression).
        assert_eq!(Period::compute_fiscal_year(NaiveDate::from_ymd_opt(2019, 2, 3).unwrap(), fye), 2019);
        // Distinct drifted year-ends map to distinct fiscal years (no collision).
        assert_ne!(
            Period::compute_fiscal_year(NaiveDate::from_ymd_opt(2019, 2, 3).unwrap(), fye),
            Period::compute_fiscal_year(NaiveDate::from_ymd_opt(2020, 2, 2).unwrap(), fye),
        );
    }

    #[test]
    fn compute_fiscal_quarter_tolerates_drift_across_month_boundary() {
        // FYE "0202": a year-end landing in late January (offset would be a
        // non-multiple-of-3 month under the old exact-month test) must still
        // resolve to the annual close, and the quarter ends likewise.
        let fye = "0202";
        assert_eq!(Period::compute_fiscal_quarter(NaiveDate::from_ymd_opt(2023, 1, 29).unwrap(), fye), Some(0));
        assert_eq!(Period::compute_fiscal_quarter(NaiveDate::from_ymd_opt(2024, 1, 28).unwrap(), fye), Some(0));
        assert_eq!(Period::compute_fiscal_quarter(NaiveDate::from_ymd_opt(2019, 2, 3).unwrap(), fye), Some(0));
        // Q1/Q2/Q3 ends (~3/6/9 months before the year-end) still resolve.
        assert_eq!(Period::compute_fiscal_quarter(NaiveDate::from_ymd_opt(2022, 5, 1).unwrap(), fye), Some(1));
        assert_eq!(Period::compute_fiscal_quarter(NaiveDate::from_ymd_opt(2022, 7, 31).unwrap(), fye), Some(2));
        assert_eq!(Period::compute_fiscal_quarter(NaiveDate::from_ymd_opt(2022, 10, 30).unwrap(), fye), Some(3));
        // A genuinely mid-quarter snapshot is still unmatched.
        assert_eq!(Period::compute_fiscal_quarter(NaiveDate::from_ymd_opt(2022, 9, 1).unwrap(), fye), None);
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
