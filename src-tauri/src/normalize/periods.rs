//! Period reconciliation — M19. YTD-to-quarter derivation.
//!
//! Companies report quarterly figures in two styles:
//! - Single-quarter: each Qn fact spans ~90 days.
//! - YTD: Q1 spans 90d, Q2 spans 180d (cumulative), Q3 spans 270d, FY spans 365d.
//!
//! Per architecture §8.2 and the user's accuracy rule, V1 must always
//! produce single-quarter values. When a single-quarter fact is missing,
//! derive: Q2_single = H1_ytd − Q1, Q3_single = 9M_ytd − H1_ytd, etc.

use std::collections::HashMap;

use chrono::NaiveDate;

use crate::domain::{Cik, Metric, RawFact};

/// A single-quarter fact (either reported directly or derived).
#[derive(Clone, Debug)]
pub struct QuarterValue {
    pub metric: Metric,
    pub cik: Cik,
    pub fy: i32,
    pub fq: u8,                                  // 1..=4
    pub period_start: NaiveDate,
    pub period_end: NaiveDate,
    /// Source raw fact id when reported directly; the contributing facts'
    /// ids when derived (longest first — the "minuend" filing's id wins
    /// for source attribution).
    pub source_fact_id: i64,
    pub value: i64,
    pub source_kind: crate::domain::SourceKind,
    pub derived: bool,
}

/// Group raw facts by (metric, fy, fp) for a single CIK and produce
/// single-quarter values, deriving where needed. Annual ("FY") and
/// instant facts are passed through unchanged via `annuals` / caller.
///
/// Returns `(quarterlies, fy_facts)` — the FY facts are returned alongside
/// so the caller can persist them on the same pass.
pub fn reconcile_quarters(
    raw: &[RawFact],
) -> (Vec<QuarterValue>, Vec<RawFact>) {
    // Index by (metric, fy, fp) — keep the most-recently-filed per slot.
    let mut by_key: HashMap<(Metric, i32, String), RawFact> = HashMap::new();
    let mut fy_out: Vec<RawFact> = Vec::new();
    for f in raw {
        let metric = match crate::normalize::concept_map::metric_for(&f.taxonomy, &f.concept) {
            Some(m) => m,
            None => continue,
        };
        if metric.is_instant() {
            // Instants don't go through quarterly derivation; caller handles.
            continue;
        }
        let fy = match f.fy { Some(x) => x, None => continue };
        let fp = match &f.fp { Some(s) => s.clone(), None => continue };
        let key = (metric, fy, fp.clone());
        let take = match by_key.get(&key) {
            Some(prev) => f.filed > prev.filed,
            None => true,
        };
        if take {
            by_key.insert(key, f.clone());
        }
    }

    // Pull out FY rows.
    for ((_, _, fp), v) in by_key.iter() {
        if fp == "FY" { fy_out.push(v.clone()); }
    }

    // Helper to read a span fact (period_start..period_end) for a slot.
    let get = |m: Metric, fy: i32, fp: &str| -> Option<&RawFact> {
        by_key.get(&(m, fy, fp.into()))
    };

    let mut out: Vec<QuarterValue> = Vec::new();
    // For each (metric, fy), see what we have and produce Q1..Q4.
    let mut keys: std::collections::HashSet<(Metric, i32)> = by_key
        .keys()
        .filter(|(_, _, fp)| fp != "FY")
        .map(|(m, fy, _)| (*m, *fy))
        .collect();
    // Also iterate metrics that may only have FY but might pair with quarter facts elsewhere.
    for k in by_key.keys() { keys.insert((k.0, k.1)); }

    for (metric, fy) in keys {
        let q1 = get(metric, fy, "Q1");
        let q2 = get(metric, fy, "Q2");
        let q3 = get(metric, fy, "Q3");
        let q4 = get(metric, fy, "Q4");
        let fy_fact = get(metric, fy, "FY");

        // Classify each Qn as single-quarter (S) or cumulative (Y).
        let classify = |f: &RawFact| -> Option<bool> {
            // Returns Some(true) for single-quarter, Some(false) for YTD-cumulative.
            let start = f.period_start?;
            let days = (f.period_end - start).num_days();
            // Single-quarter typically spans 80-100 days.
            // Cumulative quarterly spans roughly 80*n days.
            Some(days <= 110)
        };

        // Q1: if single-quarter, use it directly. (YTD-Q1 is the same as
        // single-quarter Q1 by definition, so no derivation needed.)
        if let Some(f) = q1 {
            // Q1 is always single-quarter (cumulative-Q1 == single-Q1).
            push_single(&mut out, metric, f, 1);
        }

        // Q2: derive from H1 - Q1 if Q2 row is YTD; or use Q2 directly if single-quarter.
        if let Some(f) = q2 {
            match classify(f) {
                Some(true) => push_single(&mut out, metric, f, 2),
                Some(false) => {
                    // YTD H1; need Q1 to derive single-Q2.
                    if let Some(q1f) = q1 {
                        let value = f.value_numeric.saturating_sub(q1f.value_numeric);
                        push_derived(&mut out, metric, f, q1f, 2, value);
                    }
                }
                None => {}
            }
        }

        if let Some(f) = q3 {
            match classify(f) {
                Some(true) => push_single(&mut out, metric, f, 3),
                Some(false) => {
                    // YTD 9M; need H1 (q2 must be YTD or sum of Q1+Q2 single).
                    let h1_value = sum_h1(q1, q2);
                    if let Some(h1) = h1_value {
                        let value = f.value_numeric.saturating_sub(h1);
                        push_derived_single_input(&mut out, metric, f, 3, value);
                    }
                }
                None => {}
            }
        }

        // Q4: typically not reported as a single quarter. Derive: Q4 = FY - 9M.
        if let Some(f) = q4 {
            match classify(f) {
                Some(true) => push_single(&mut out, metric, f, 4),
                _ => {}
            }
        } else if let (Some(fy_f), Some(_q3f)) = (fy_fact, q3) {
            // Compute 9M: if Q3 is YTD it IS the 9M; otherwise sum.
            let nine_m = match q3.and_then(|f| classify(f).map(|s| (s, f))) {
                Some((true, _)) => sum_h1_plus_q3(q1, q2, q3),
                Some((false, q3_ytd)) => Some(q3_ytd.value_numeric),
                None => None,
            };
            if let Some(nm) = nine_m {
                let value = fy_f.value_numeric.saturating_sub(nm);
                push_derived_single_input(&mut out, metric, fy_f, 4, value);
            }
        }
    }

    (out, fy_out)
}

fn sum_h1(q1: Option<&RawFact>, q2: Option<&RawFact>) -> Option<i64> {
    // If q2 is YTD, that's H1. If both are single, sum them.
    let q2 = q2?;
    let q1 = q1?;
    let q2_days = q2.period_start.map(|s| (q2.period_end - s).num_days()).unwrap_or(0);
    if q2_days > 110 {
        Some(q2.value_numeric)
    } else {
        Some(q1.value_numeric.saturating_add(q2.value_numeric))
    }
}

fn sum_h1_plus_q3(
    q1: Option<&RawFact>,
    q2: Option<&RawFact>,
    q3: Option<&RawFact>,
) -> Option<i64> {
    let h1 = sum_h1(q1, q2)?;
    let q3 = q3?;
    Some(h1.saturating_add(q3.value_numeric))
}

fn push_single(out: &mut Vec<QuarterValue>, metric: Metric, f: &RawFact, fq: u8) {
    let start = f.period_start.unwrap_or(f.period_end);
    out.push(QuarterValue {
        metric,
        cik: f.cik.clone(),
        fy: f.fy.unwrap_or(0),
        fq,
        period_start: start,
        period_end: f.period_end,
        source_fact_id: f.id,
        value: f.value_numeric,
        source_kind: f.source_kind,
        derived: false,
    });
}

fn push_derived(
    out: &mut Vec<QuarterValue>,
    metric: Metric,
    minuend: &RawFact,
    _subtrahend: &RawFact,
    fq: u8,
    value: i64,
) {
    let start = minuend.period_start.unwrap_or(minuend.period_end);
    out.push(QuarterValue {
        metric,
        cik: minuend.cik.clone(),
        fy: minuend.fy.unwrap_or(0),
        fq,
        period_start: start,
        period_end: minuend.period_end,
        source_fact_id: minuend.id,
        value,
        source_kind: minuend.source_kind,
        derived: true,
    });
}

fn push_derived_single_input(
    out: &mut Vec<QuarterValue>,
    metric: Metric,
    minuend: &RawFact,
    fq: u8,
    value: i64,
) {
    let start = minuend.period_start.unwrap_or(minuend.period_end);
    out.push(QuarterValue {
        metric,
        cik: minuend.cik.clone(),
        fy: minuend.fy.unwrap_or(0),
        fq,
        period_start: start,
        period_end: minuend.period_end,
        source_fact_id: minuend.id,
        value,
        source_kind: minuend.source_kind,
        derived: true,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{AccessionNo, SourceKind};
    use chrono::Utc;

    fn rf(metric_concept: (&str, &str), fy: i32, fp: &str, days: i64, val: i64) -> RawFact {
        let end = NaiveDate::from_ymd_opt(2024, 9, 30).unwrap();
        let start = end - chrono::Duration::days(days);
        RawFact {
            id: 0,
            cik: Cik("0000320193".into()),
            accession_no: AccessionNo("X".into()),
            taxonomy: metric_concept.0.into(),
            concept: metric_concept.1.into(),
            unit: "USD".into(),
            value_numeric: val,
            period_start: Some(start),
            period_end: end,
            is_instant: false,
            fy: Some(fy),
            fp: Some(fp.into()),
            filed: Some(end),
            source_kind: SourceKind::XbrlApi,
            ingested_at: Utc::now(),
        }
    }

    #[test]
    fn passes_through_single_quarter_q1() {
        let raw = vec![rf(("us-gaap", "Revenues"), 2024, "Q1", 90, 100)];
        let (q, _) = reconcile_quarters(&raw);
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].fq, 1);
        assert_eq!(q[0].value, 100);
        assert!(!q[0].derived);
    }

    #[test]
    fn derives_q2_from_ytd_h1_minus_q1() {
        let raw = vec![
            rf(("us-gaap", "Revenues"), 2024, "Q1", 90, 100),
            rf(("us-gaap", "Revenues"), 2024, "Q2", 180, 250), // H1 cumulative
        ];
        let (q, _) = reconcile_quarters(&raw);
        let q2 = q.iter().find(|x| x.fq == 2).unwrap();
        assert_eq!(q2.value, 150); // 250 - 100
        assert!(q2.derived);
    }

    #[test]
    fn passes_through_single_quarter_q2() {
        let raw = vec![
            rf(("us-gaap", "Revenues"), 2024, "Q1", 90, 100),
            rf(("us-gaap", "Revenues"), 2024, "Q2", 90, 150), // single-quarter
        ];
        let (q, _) = reconcile_quarters(&raw);
        let q2 = q.iter().find(|x| x.fq == 2).unwrap();
        assert_eq!(q2.value, 150);
        assert!(!q2.derived);
    }

    #[test]
    fn derives_q4_from_fy_minus_9m_when_q3_is_ytd() {
        let raw = vec![
            rf(("us-gaap", "Revenues"), 2024, "Q3", 270, 300), // 9M YTD
            rf(("us-gaap", "Revenues"), 2024, "FY", 365, 400), // full year
        ];
        let (q, _) = reconcile_quarters(&raw);
        let q4 = q.iter().find(|x| x.fq == 4).unwrap();
        assert_eq!(q4.value, 100); // 400 - 300
        assert!(q4.derived);
    }
}
