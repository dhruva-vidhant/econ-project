//! Period resolution — turns a user-supplied period token + granularity into
//! a `period_id` against a company's known `Period` rows. See techspec §5.

use chrono::NaiveDate;

use crate::domain::{Period, PeriodKind};

/// Tolerance (days) for matching a bare date to a period's `end_date` when
/// it falls outside the period's own `[start_date, end_date]` range —
/// covers filing/announcement dates reported a little after period close.
pub const TOL: i64 = 45;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveError(pub &'static str);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    Matched(i64),
    Unaligned,
    Error(ResolveError),
}

/// Resolve a period token (fiscal label or bare date) against `periods`,
/// using `granularity` as the default when the token itself doesn't imply
/// one (a token encoding a quarter always forces `Quarterly`; a bare year
/// always forces `Annual`).
pub fn resolve_period(token: &str, granularity: PeriodKind, periods: &[Period]) -> Resolution {
    let token = token.trim();

    if let Some((fy, fq)) = parse_fiscal_token(token) {
        let kind = if fq == 0 { PeriodKind::Annual } else { PeriodKind::Quarterly };
        return match_unique(periods, |p| {
            p.kind == kind && p.fiscal_year == fy && p.fiscal_quarter == fq
        });
    }

    if let Some(date) = parse_date(token) {
        // 1. Exact end_date match.
        let exact = match_unique(periods, |p| p.kind == granularity && p.end_date == date);
        if !matches!(exact, Resolution::Unaligned) {
            return exact;
        }
        // 2. Date within [start_date, end_date].
        let in_range = match_unique(periods, |p| {
            p.kind == granularity && p.start_date <= date && date <= p.end_date
        });
        if !matches!(in_range, Resolution::Unaligned) {
            return in_range;
        }
        // 3. Within ±TOL days of end_date.
        let within_tol = match_unique(periods, |p| {
            p.kind == granularity && (date - p.end_date).num_days().abs() <= TOL
        });
        return within_tol;
    }

    Resolution::Error(ResolveError("bad_period"))
}

fn match_unique(periods: &[Period], pred: impl Fn(&Period) -> bool) -> Resolution {
    let mut matches = periods.iter().filter(|p| pred(p));
    match (matches.next(), matches.next()) {
        (None, _) => Resolution::Unaligned,
        (Some(p), None) => Resolution::Matched(p.id),
        (Some(_), Some(_)) => Resolution::Error(ResolveError("ambiguous_period")),
    }
}

/// Parses fiscal-label tokens: `"FY2024"`, `"2024"`, `"FY2024 Q2"`,
/// `"2024-Q2"`. Returns `(fiscal_year, fiscal_quarter)` with `fiscal_quarter
/// = 0` for annual tokens.
fn parse_fiscal_token(token: &str) -> Option<(i32, u8)> {
    let upper = token.to_ascii_uppercase();
    let rest = upper.strip_prefix("FY").unwrap_or(&upper);

    let (year_str, quarter_str) = if let Some(idx) = rest.find(" Q") {
        (&rest[..idx], Some(&rest[idx + 2..]))
    } else if let Some(idx) = rest.find('-') {
        let (y, q) = rest.split_at(idx);
        let q = q.trim_start_matches('-');
        if q.starts_with('Q') {
            (y, Some(&q[1..]))
        } else {
            return None;
        }
    } else {
        (rest, None)
    };

    if year_str.len() != 4 || !year_str.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let year: i32 = year_str.parse().ok()?;

    match quarter_str {
        None => Some((year, 0)),
        Some(q) => {
            let q: u8 = q.trim().parse().ok()?;
            if (1..=4).contains(&q) { Some((year, q)) } else { None }
        }
    }
}

fn parse_date(token: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(token, "%Y-%m-%d").ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Cik;

    fn annual(id: i64, fy: i32, start: (i32, u32, u32), end: (i32, u32, u32)) -> Period {
        Period {
            id,
            cik: Cik("0000000001".into()),
            fiscal_year: fy,
            fiscal_quarter: 0,
            fiscal_year_end: "1231".into(),
            start_date: NaiveDate::from_ymd_opt(start.0, start.1, start.2).unwrap(),
            end_date: NaiveDate::from_ymd_opt(end.0, end.1, end.2).unwrap(),
            kind: PeriodKind::Annual,
            is_53_week: false,
        }
    }

    fn quarter(id: i64, fy: i32, fq: u8, start: (i32, u32, u32), end: (i32, u32, u32)) -> Period {
        Period {
            id,
            cik: Cik("0000000001".into()),
            fiscal_year: fy,
            fiscal_quarter: fq,
            fiscal_year_end: "1231".into(),
            start_date: NaiveDate::from_ymd_opt(start.0, start.1, start.2).unwrap(),
            end_date: NaiveDate::from_ymd_opt(end.0, end.1, end.2).unwrap(),
            kind: PeriodKind::Quarterly,
            is_53_week: false,
        }
    }

    fn fixture_periods() -> Vec<Period> {
        vec![
            annual(1, 2022, (2022, 1, 1), (2022, 12, 31)),
            annual(2, 2023, (2023, 1, 1), (2023, 12, 31)),
            annual(3, 2024, (2024, 1, 1), (2024, 12, 31)),
            quarter(4, 2024, 1, (2024, 1, 1), (2024, 3, 31)),
            quarter(5, 2024, 2, (2024, 4, 1), (2024, 6, 30)),
            quarter(6, 2024, 3, (2024, 7, 1), (2024, 9, 30)),
            quarter(7, 2024, 4, (2024, 10, 1), (2024, 12, 31)),
        ]
    }

    #[test]
    fn resolve_fiscal_annual() {
        let periods = fixture_periods();
        assert_eq!(resolve_period("FY2024", PeriodKind::Annual, &periods), Resolution::Matched(3));
    }

    #[test]
    fn resolve_bare_year() {
        let periods = fixture_periods();
        assert_eq!(resolve_period("2023", PeriodKind::Annual, &periods), Resolution::Matched(2));
    }

    #[test]
    fn resolve_fiscal_quarter() {
        let periods = fixture_periods();
        // Q overrides the declared (annual) granularity.
        assert_eq!(resolve_period("FY2024 Q2", PeriodKind::Annual, &periods), Resolution::Matched(5));
    }

    #[test]
    fn resolve_date_exact_end() {
        let periods = fixture_periods();
        assert_eq!(resolve_period("2024-12-31", PeriodKind::Annual, &periods), Resolution::Matched(3));
    }

    #[test]
    fn resolve_date_in_range() {
        let periods = fixture_periods();
        assert_eq!(resolve_period("2024-07-15", PeriodKind::Annual, &periods), Resolution::Matched(3));
    }

    #[test]
    fn resolve_date_within_tol() {
        let periods = fixture_periods();
        // 20 days after FY2024's 2024-12-31 close, within TOL=45.
        assert_eq!(resolve_period("2025-01-20", PeriodKind::Annual, &periods), Resolution::Matched(3));
    }

    #[test]
    fn resolve_date_unmatched() {
        let periods = fixture_periods();
        assert_eq!(resolve_period("2019-06-01", PeriodKind::Annual, &periods), Resolution::Unaligned);
    }

    #[test]
    fn resolve_garbage() {
        let periods = fixture_periods();
        assert_eq!(
            resolve_period("Q9-foo", PeriodKind::Annual, &periods),
            Resolution::Error(ResolveError("bad_period"))
        );
    }
}
