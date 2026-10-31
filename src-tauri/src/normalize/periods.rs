//! Period reconciliation — M19. YTD-to-quarter derivation.
//!
//! Companies report quarterly figures in two styles:
//! - Single-quarter: each Qn fact spans ~90 days.
//! - YTD: Q1 spans 90d, Q2 spans 180d (cumulative), Q3 spans 270d, FY spans 365d.
//!
//! Per architecture §8.2 and the user's accuracy rule, V1 must always
//! produce single-quarter values. When a single-quarter fact is missing,
//! derive: Q2_single = H1_ytd − Q1, Q3_single = 9M_ytd − H1_ytd, etc.
//!
//! ## Why we don't trust the SEC `fy` tag for grouping
//!
//! Each 10-K / 10-Q embeds prior-year comparative data, and the SEC
//! companyfacts API tags every embedded fact with the *filing's*
//! fiscal year and period — not the period the fact actually
//! represents. A 10-K filed for FY2010 carries (fy=2010, fp=FY) rows
//! for fiscal 2008, 2009, *and* 2010, and (fy=2010, fp=Q3) rows for
//! the comparative Q3 of 2009 alongside Q3 2010. Grouping by the SEC
//! tag silently collapses three years' data into one slot and the
//! winner is whichever filing was indexed most recently — random.
//!
//! Reconciliation therefore derives the fiscal year from each fact's
//! own `period_end` (using the issuer's fiscal-year-end calendar) and
//! classifies each fact into a span-aware slot (single-Q1..Q4, YTD-H1,
//! YTD-9M, FY). The slot encodes both position-in-year and duration,
//! so single-quarter Q2 (~90 days) and cumulative H1 (~180 days)
//! filed under the same `fp=Q2` tag are kept distinct.
//!
//! ## Concept-consistency rule
//!
//! Several canonical metrics resolve through multiple XBRL concept
//! candidates (see `concept_map`). The fallbacks have different scopes —
//! e.g. `DepreciationAndAmortization` (annual-only filings) and
//! `DepreciationAmortizationAndAccretionNet` (quarterly, includes
//! accretion) both map to `Metric::DepreciationAmortization`. Mixing
//! concepts within a single (metric, fiscal year) breaks Q4 derivation:
//! `Q4 = FY − 9M` produces a negative if the FY value is taken from one
//! concept and the 9M YTD from another.
//!
//! Reconciliation picks one source concept per (metric, fy) and uses
//! only that concept's facts for derivation. Selection is by coverage
//! (number of distinct slots present) with ties broken by catalog
//! priority order.

use std::collections::{BTreeSet, HashMap};

use chrono::NaiveDate;

use crate::domain::{Cik, Metric, RawFact};
use crate::normalize::fiscal_calendar::FiscalCalendar;

/// A single-quarter fact (either reported directly or derived).
#[derive(Clone, Debug)]
pub struct QuarterValue {
    pub metric: Metric,
    pub cik: Cik,
    pub fy: i32,
    pub fq: u8, // 1..=4
    pub fiscal_year_end: String,
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

/// Position+duration of a fact within its fiscal year.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Slot {
    SingleQ1,
    SingleQ2,
    SingleQ3,
    SingleQ4,
    YtdH1,
    Ytd9M,
    Fy,
}

const ALL_SLOTS: &[Slot] = &[
    Slot::SingleQ1,
    Slot::SingleQ2,
    Slot::SingleQ3,
    Slot::SingleQ4,
    Slot::YtdH1,
    Slot::Ytd9M,
    Slot::Fy,
];

fn concept_key(taxonomy: &str, concept: &str) -> String {
    format!("{taxonomy}:{concept}")
}

/// Classify a duration fact into one of the canonical slots based on
/// the fiscal quarter its `period_end` falls in and the duration of
/// its period (which distinguishes single-quarter from YTD). The SEC
/// `fp` tag is only a fallback: it names the *filing's* period, so a
/// Q3 10-Q also tags its prior-quarter and prior-year comparatives
/// `Q3`, and trusting it collapses them all into one slot. Returns
/// `None` for facts that aren't usable (missing period_start, unknown
/// quarter, weirdly-sized spans).
fn classify_slot(f: &RawFact, end_quarter: Option<u8>) -> Option<Slot> {
    let start = f.period_start?;
    let days = (f.period_end - start).num_days();
    let quarter = match end_quarter {
        // `compute_fiscal_quarter` reports the year-end close as 0.
        Some(0) => 4,
        Some(q) => q,
        None => match f.fp.as_deref()? {
            "Q1" => 1,
            "Q2" => 2,
            "Q3" => 3,
            "Q4" => 4,
            "FY" => 4,
            _ => return None,
        },
    };
    Some(match (quarter, days) {
        (1, d) if d <= 110 => Slot::SingleQ1,
        (2, d) if d <= 110 => Slot::SingleQ2,
        (2, d) if (150..=210).contains(&d) => Slot::YtdH1,
        (3, d) if d <= 110 => Slot::SingleQ3,
        (3, d) if (240..=290).contains(&d) => Slot::Ytd9M,
        (4, d) if d <= 110 => Slot::SingleQ4,
        (4, d) if d >= 340 => Slot::Fy,
        _ => return None,
    })
}

/// Group raw facts by (metric, derived_fy) for a single CIK and produce
/// single-quarter values, deriving where needed. The fy is computed
/// from each fact's `period_end` against the issuer's fiscal-year-end
/// calendar — we deliberately ignore the SEC `fy` tag because it
/// reflects the *filing*'s year, not the period the fact represents.
///
/// Returns `(quarterlies, fy_facts)` — the FY facts are returned alongside
/// so the caller can persist them on the same pass.
pub fn reconcile_quarters(
    raw: &[RawFact],
    calendar: &FiscalCalendar,
) -> (Vec<QuarterValue>, Vec<RawFact>) {
    // Index by (metric, concept, derived_fy, slot) — keep the
    // most-recently-filed per slot. Tracking concept lets us avoid
    // mixing inputs from different XBRL concepts within one
    // (metric, fy) series.
    let mut by_key: HashMap<(Metric, String, usize, i32, Slot), RawFact> = HashMap::new();
    for f in raw {
        let metric = match crate::normalize::concept_map::metric_for(&f.taxonomy, &f.concept) {
            Some(m) => m,
            None => continue,
        };
        if metric.is_instant() {
            // Instants don't go through quarterly derivation; caller handles.
            continue;
        }
        let fiscal = calendar.period_for(f.period_end);
        let slot = match classify_slot(f, fiscal.fiscal_quarter) {
            Some(s) => s,
            None => continue,
        };
        let ck = concept_key(&f.taxonomy, &f.concept);
        let key = (metric, ck, fiscal.segment_id, fiscal.fiscal_year, slot);
        let take = match by_key.get(&key) {
            Some(prev) => f.filed > prev.filed,
            None => true,
        };
        if take {
            by_key.insert(key, f.clone());
        }
    }

    // Collect concepts present per (metric, derived_fy).
    let mut concepts_by_year: HashMap<(Metric, usize, i32), BTreeSet<String>> = HashMap::new();
    for (metric, concept, segment_id, fy, _slot) in by_key.keys() {
        concepts_by_year
            .entry((*metric, *segment_id, *fy))
            .or_default()
            .insert(concept.clone());
    }

    let mut out: Vec<QuarterValue> = Vec::new();
    let mut fy_out: Vec<RawFact> = Vec::new();

    for ((metric, segment_id, fy), concepts) in concepts_by_year {
        let catalog: Vec<String> = crate::normalize::concept_map::concepts_for(metric)
            .iter()
            .map(|(t, c)| concept_key(t, c))
            .collect();
        let chosen = match pick_concept(&by_key, metric, segment_id, fy, &concepts, &catalog) {
            Some(c) => c,
            None => continue,
        };

        let g = |s: Slot| -> Option<&RawFact> {
            by_key.get(&(metric, chosen.clone(), segment_id, fy, s))
        };
        let fiscal_year_end = calendar
            .period_for(
                ALL_SLOTS
                    .iter()
                    .find_map(|slot| g(*slot))
                    .expect("a reconciliation group contains a fact")
                    .period_end,
            )
            .fiscal_year_end;
        let q1 = g(Slot::SingleQ1);
        let q2_single = g(Slot::SingleQ2);
        let h1 = g(Slot::YtdH1);
        let q3_single = g(Slot::SingleQ3);
        let nine_m = g(Slot::Ytd9M);
        let q4_single = g(Slot::SingleQ4);
        let fy_fact = g(Slot::Fy);

        if let Some(f) = fy_fact {
            fy_out.push(f.clone());
        }

        if let Some(f) = q1 {
            push_single(&mut out, metric, f, fy, &fiscal_year_end, 1);
        }

        // Q2: prefer single-quarter; else derive H1 - Q1.
        if let Some(f) = q2_single {
            push_single(&mut out, metric, f, fy, &fiscal_year_end, 2);
        } else if let (Some(h1f), Some(q1f)) = (h1, q1) {
            let value = h1f.value_numeric.saturating_sub(q1f.value_numeric);
            push_derived(&mut out, metric, h1f, q1f, fy, &fiscal_year_end, 2, value);
        }

        // Q3: prefer single-quarter; else derive 9M - H1 (using YTD-H1
        // when present, falling back to Q1+SingleQ2 sum).
        if let Some(f) = q3_single {
            push_single(&mut out, metric, f, fy, &fiscal_year_end, 3);
        } else if let Some(nm) = nine_m {
            // Q3 = 9M − H1, spanning (H1_end, 9M_end]. Capture the H1 close
            // date alongside the value so the derived quarter starts the day
            // after H1, not at the fiscal-year start the 9M fact carries.
            let h1_value = match (h1, q1, q2_single) {
                (Some(h1f), _, _) => Some((h1f.value_numeric, h1f.period_end)),
                (None, Some(q1f), Some(q2f)) => Some((
                    q1f.value_numeric.saturating_add(q2f.value_numeric),
                    q2f.period_end,
                )),
                _ => None,
            };
            if let Some((h1v, h1_end)) = h1_value {
                let value = nm.value_numeric.saturating_sub(h1v);
                push_derived_single_input(
                    &mut out,
                    metric,
                    nm,
                    fy,
                    &fiscal_year_end,
                    3,
                    value,
                    next_day(h1_end),
                );
            }
        }

        // Q4: prefer single-quarter; else derive FY - 9M.
        if let Some(f) = q4_single {
            push_single(&mut out, metric, f, fy, &fiscal_year_end, 4);
        } else if let Some(fy_f) = fy_fact {
            // Q4 = FY − 9M, spanning (9M_end, FY_end]. Capture the 9M close
            // date so the derived quarter starts the day after the first
            // three quarters, not at the fiscal-year start the FY fact carries.
            let nine_m_value = match (nine_m, q3_single, h1, q1, q2_single) {
                (Some(nm), _, _, _, _) => Some((nm.value_numeric, nm.period_end)),
                (None, Some(q3f), Some(h1f), _, _) => Some((
                    h1f.value_numeric.saturating_add(q3f.value_numeric),
                    q3f.period_end,
                )),
                (None, Some(q3f), None, Some(q1f), Some(q2f)) => Some((
                    q1f.value_numeric
                        .saturating_add(q2f.value_numeric)
                        .saturating_add(q3f.value_numeric),
                    q3f.period_end,
                )),
                _ => None,
            };
            if let Some((nm, nine_m_end)) = nine_m_value {
                let value = fy_f.value_numeric.saturating_sub(nm);
                push_derived_single_input(
                    &mut out,
                    metric,
                    fy_f,
                    fy,
                    &fiscal_year_end,
                    4,
                    value,
                    next_day(nine_m_end),
                );
            }
        }
    }

    (out, fy_out)
}

/// Picks the source concept for a (metric, fy): highest slot coverage,
/// breaking ties by catalog priority order.
fn pick_concept(
    by_key: &HashMap<(Metric, String, usize, i32, Slot), RawFact>,
    metric: Metric,
    segment_id: usize,
    fy: i32,
    concepts: &BTreeSet<String>,
    catalog: &[String],
) -> Option<String> {
    let priority = |c: &str| -> usize { catalog.iter().position(|x| x == c).unwrap_or(usize::MAX) };
    let coverage = |c: &str| -> usize {
        ALL_SLOTS
            .iter()
            .filter(|s| by_key.contains_key(&(metric, c.to_string(), segment_id, fy, **s)))
            .count()
    };
    concepts
        .iter()
        .max_by(|a, b| {
            let cov_cmp = coverage(a).cmp(&coverage(b));
            if cov_cmp != std::cmp::Ordering::Equal {
                cov_cmp
            } else {
                priority(b).cmp(&priority(a))
            }
        })
        .cloned()
}

fn push_single(
    out: &mut Vec<QuarterValue>,
    metric: Metric,
    f: &RawFact,
    fy: i32,
    fye: &str,
    fq: u8,
) {
    let start = f.period_start.unwrap_or(f.period_end);
    out.push(QuarterValue {
        metric,
        cik: f.cik.clone(),
        fy,
        fq,
        fiscal_year_end: fye.to_owned(),
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
    subtrahend: &RawFact,
    fy: i32,
    fye: &str,
    fq: u8,
    value: i64,
) {
    // The derived single quarter spans (subtrahend_end, minuend_end]: e.g.
    // Q2 = H1 − Q1 covers the period after Q1 ends through the H1 close.
    // Use the day after the subtrahend's period to anchor the start, not
    // the minuend's (cumulative) start, which would be the fiscal-year start.
    let start = next_day(subtrahend.period_end);
    out.push(QuarterValue {
        metric,
        cik: minuend.cik.clone(),
        fy,
        fq,
        fiscal_year_end: fye.to_owned(),
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
    fy: i32,
    fye: &str,
    fq: u8,
    value: i64,
    // Start of the derived single quarter. The minuend is a cumulative
    // (YTD or full-year) fact whose own start is the fiscal-year start, so
    // the caller passes the boundary after the prior period instead.
    start: NaiveDate,
) {
    out.push(QuarterValue {
        metric,
        cik: minuend.cik.clone(),
        fy,
        fq,
        fiscal_year_end: fye.to_owned(),
        period_start: start,
        period_end: minuend.period_end,
        source_fact_id: minuend.id,
        value,
        source_kind: minuend.source_kind,
        derived: true,
    });
}

/// The day after `d`, saturating at `d` if `d` is the maximum representable
/// date (never expected for filing dates).
fn next_day(d: NaiveDate) -> NaiveDate {
    d.succ_opt().unwrap_or(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{AccessionNo, SourceKind};
    use chrono::Utc;

    fn calendar() -> FiscalCalendar {
        FiscalCalendar::single("1231".into())
    }

    fn rf_dates(
        metric_concept: (&str, &str),
        fp: &str,
        period_start: NaiveDate,
        period_end: NaiveDate,
        val: i64,
    ) -> RawFact {
        RawFact {
            id: 0,
            cik: Cik("0000320193".into()),
            accession_no: AccessionNo("X".into()),
            taxonomy: metric_concept.0.into(),
            concept: metric_concept.1.into(),
            unit: "USD".into(),
            value_numeric: val,
            period_start: Some(period_start),
            period_end,
            is_instant: false,
            // SEC fy/fp tags reflect the *filing*'s period, not the
            // period the fact represents — tests below use realistic
            // period_start/period_end and let derivation infer the fy.
            fy: Some(period_end.format("%Y").to_string().parse().unwrap()),
            fp: Some(fp.into()),
            filed: Some(period_end),
            source_kind: SourceKind::XbrlApi,
            ingested_at: Utc::now(),
        }
    }

    fn d(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn passes_through_single_quarter_q1() {
        let raw = vec![rf_dates(
            ("us-gaap", "Revenues"),
            "Q1",
            d(2024, 1, 1),
            d(2024, 3, 31),
            100,
        )];
        let (q, _) = reconcile_quarters(&raw, &calendar());
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].fq, 1);
        assert_eq!(q[0].value, 100);
        assert!(!q[0].derived);
    }

    #[test]
    fn derives_q2_from_ytd_h1_minus_q1() {
        let raw = vec![
            rf_dates(
                ("us-gaap", "Revenues"),
                "Q1",
                d(2024, 1, 1),
                d(2024, 3, 31),
                100,
            ),
            rf_dates(
                ("us-gaap", "Revenues"),
                "Q2",
                d(2024, 1, 1),
                d(2024, 6, 30),
                250,
            ),
        ];
        let (q, _) = reconcile_quarters(&raw, &calendar());
        let q2 = q.iter().find(|x| x.fq == 2).unwrap();
        assert_eq!(q2.value, 150);
        assert!(q2.derived);
    }

    #[test]
    fn passes_through_single_quarter_q2() {
        let raw = vec![
            rf_dates(
                ("us-gaap", "Revenues"),
                "Q1",
                d(2024, 1, 1),
                d(2024, 3, 31),
                100,
            ),
            rf_dates(
                ("us-gaap", "Revenues"),
                "Q2",
                d(2024, 4, 1),
                d(2024, 6, 30),
                150,
            ),
        ];
        let (q, _) = reconcile_quarters(&raw, &calendar());
        let q2 = q.iter().find(|x| x.fq == 2).unwrap();
        assert_eq!(q2.value, 150);
        assert!(!q2.derived);
    }

    #[test]
    fn derives_q4_from_fy_minus_9m_when_q3_is_ytd() {
        let raw = vec![
            rf_dates(
                ("us-gaap", "Revenues"),
                "Q3",
                d(2024, 1, 1),
                d(2024, 9, 30),
                300,
            ),
            rf_dates(
                ("us-gaap", "Revenues"),
                "FY",
                d(2024, 1, 1),
                d(2024, 12, 31),
                400,
            ),
        ];
        let (q, _) = reconcile_quarters(&raw, &calendar());
        let q4 = q.iter().find(|x| x.fq == 4).unwrap();
        assert_eq!(q4.value, 100);
        assert!(q4.derived);
    }

    #[test]
    fn derived_quarters_start_after_the_prior_period_not_at_fy_start() {
        // A YTD-style filer: Q1/H1/9M/FY are all cumulative from the
        // fiscal-year start. The single-quarter derivations for Q2, Q3 and
        // Q4 must each begin the day after the prior period ends, otherwise
        // every derived quarter's start collapses to the fiscal-year start
        // (which previously sorted Q4 before Q2/Q3 when ordering by start).
        let raw = vec![
            rf_dates(
                ("us-gaap", "Revenues"),
                "Q1",
                d(2024, 1, 1),
                d(2024, 3, 31),
                100,
            ),
            rf_dates(
                ("us-gaap", "Revenues"),
                "Q2",
                d(2024, 1, 1),
                d(2024, 6, 30),
                250,
            ),
            rf_dates(
                ("us-gaap", "Revenues"),
                "Q3",
                d(2024, 1, 1),
                d(2024, 9, 30),
                450,
            ),
            rf_dates(
                ("us-gaap", "Revenues"),
                "FY",
                d(2024, 1, 1),
                d(2024, 12, 31),
                700,
            ),
        ];
        let (q, _) = reconcile_quarters(&raw, &calendar());

        let q2 = q.iter().find(|x| x.fq == 2).unwrap();
        assert!(q2.derived);
        assert_eq!(q2.value, 150);
        assert_eq!(q2.period_start, d(2024, 4, 1), "Q2 starts after Q1 ends");
        assert_eq!(q2.period_end, d(2024, 6, 30));

        let q3 = q.iter().find(|x| x.fq == 3).unwrap();
        assert!(q3.derived);
        assert_eq!(q3.value, 200);
        assert_eq!(q3.period_start, d(2024, 7, 1), "Q3 starts after H1 ends");
        assert_eq!(q3.period_end, d(2024, 9, 30));

        let q4 = q.iter().find(|x| x.fq == 4).unwrap();
        assert!(q4.derived);
        assert_eq!(q4.value, 250);
        assert_eq!(q4.period_start, d(2024, 10, 1), "Q4 starts after 9M ends");
        assert_eq!(q4.period_end, d(2024, 12, 31));

        // Ordering by period_start must now match fiscal-quarter order.
        let mut by_start = q.clone();
        by_start.sort_by_key(|x| x.period_start);
        let order: Vec<u8> = by_start.iter().map(|x| x.fq).collect();
        assert_eq!(order, vec![1, 2, 3, 4]);
    }

    #[test]
    fn picks_concept_with_more_coverage_when_two_concepts_share_a_metric() {
        // Regression guard for Wells Fargo FY2010 depreciation/amortization:
        // `DepreciationAndAmortization` is filed annual-only, while
        // `DepreciationAmortizationAndAccretionNet` is filed quarterly
        // (YTD style). Both map to `Metric::DepreciationAmortization`.
        // If we mix them, Q4 = FY(D&A) − Q3-YTD(D&A&AN) goes negative
        // because the accretion-inclusive concept reports a larger value.
        // Reconciliation must pick the higher-coverage concept and use
        // only that one for derivation.
        let raw = vec![
            rf_dates(
                ("us-gaap", "DepreciationAndAmortization"),
                "FY",
                d(2010, 1, 1),
                d(2010, 12, 31),
                4_000,
            ),
            rf_dates(
                ("us-gaap", "DepreciationAmortizationAndAccretionNet"),
                "Q1",
                d(2010, 1, 1),
                d(2010, 3, 31),
                1_500,
            ),
            rf_dates(
                ("us-gaap", "DepreciationAmortizationAndAccretionNet"),
                "Q2",
                d(2010, 1, 1),
                d(2010, 6, 30),
                3_000,
            ),
            rf_dates(
                ("us-gaap", "DepreciationAmortizationAndAccretionNet"),
                "Q3",
                d(2010, 1, 1),
                d(2010, 9, 30),
                4_500,
            ),
            rf_dates(
                ("us-gaap", "DepreciationAmortizationAndAccretionNet"),
                "FY",
                d(2010, 1, 1),
                d(2010, 12, 31),
                6_000,
            ),
        ];
        let (q, _) = reconcile_quarters(&raw, &calendar());
        let q4 = q.iter().find(|x| x.fq == 4).unwrap();
        assert_eq!(q4.value, 1_500);
        assert!(q4.derived);
    }

    #[test]
    fn segregates_facts_by_period_end_year_not_sec_fy_tag() {
        // Real-world Wells Fargo case: a 10-K filed for FY2010 embeds
        // FY2008 and FY2009 comparative data, all tagged fy=2010, fp=FY
        // by the SEC companyfacts API. If we trust the SEC tag we
        // collapse them into one slot and the winner is essentially
        // random. Reconciliation must derive fy from period_end so each
        // year's data lands in its own bucket.
        let raw = vec![
            rf_dates(
                ("us-gaap", "Revenues"),
                "FY",
                d(2008, 1, 1),
                d(2008, 12, 31),
                800,
            ),
            rf_dates(
                ("us-gaap", "Revenues"),
                "FY",
                d(2009, 1, 1),
                d(2009, 12, 31),
                900,
            ),
            rf_dates(
                ("us-gaap", "Revenues"),
                "FY",
                d(2010, 1, 1),
                d(2010, 12, 31),
                1_000,
            ),
            rf_dates(
                ("us-gaap", "Revenues"),
                "Q3",
                d(2010, 1, 1),
                d(2010, 9, 30),
                700,
            ),
        ];
        let (q, _fy) = reconcile_quarters(&raw, &calendar());
        let q4_2010 = q
            .iter()
            .find(|x| x.fq == 4 && x.period_end == d(2010, 12, 31))
            .expect("Q4 2010 should derive cleanly");
        assert_eq!(q4_2010.value, 300);
        assert!(q4_2010.derived);
    }

    #[test]
    fn distinguishes_single_quarter_q2_from_ytd_h1_in_same_fp_slot() {
        // SEC companyfacts tags both single-quarter and YTD facts with
        // the same `fp` (e.g. "Q2"). The reconciler must classify them
        // by period span (single ~90 days, YTD ~180 days) and prefer
        // the single-quarter row when both are filed.
        let raw = vec![
            rf_dates(
                ("us-gaap", "Revenues"),
                "Q1",
                d(2024, 1, 1),
                d(2024, 3, 31),
                100,
            ),
            // Single-quarter Q2.
            rf_dates(
                ("us-gaap", "Revenues"),
                "Q2",
                d(2024, 4, 1),
                d(2024, 6, 30),
                175,
            ),
            // YTD H1 — same fp tag, longer span.
            rf_dates(
                ("us-gaap", "Revenues"),
                "Q2",
                d(2024, 1, 1),
                d(2024, 6, 30),
                275,
            ),
        ];
        let (q, _) = reconcile_quarters(&raw, &calendar());
        let q2 = q.iter().find(|x| x.fq == 2).unwrap();
        assert_eq!(q2.value, 175);
        assert!(!q2.derived, "single-quarter should win over derived");
    }

    #[test]
    fn ignores_filing_fp_tag_for_prior_period_comparatives() {
        // A Q3 10-Q tags its Q1/Q2 comparatives `Q3` too. Each must land
        // in the slot its period_end implies, not collapse into SingleQ3.
        let raw = vec![
            rf_dates(("us-gaap", "NetIncomeLoss"), "Q3", d(2026, 1, 1), d(2026, 3, 31), 1113),
            rf_dates(("us-gaap", "NetIncomeLoss"), "Q3", d(2026, 4, 1), d(2026, 6, 30), 1104),
            rf_dates(("us-gaap", "NetIncomeLoss"), "Q3", d(2026, 7, 1), d(2026, 9, 30), 1073),
            rf_dates(("us-gaap", "NetIncomeLoss"), "Q3", d(2026, 1, 1), d(2026, 9, 30), 3290),
        ];
        let (q, _) = reconcile_quarters(&raw, &calendar());
        let val = |fq: u8| q.iter().find(|x| x.fq == fq).unwrap().value;
        assert_eq!(val(1), 1113);
        assert_eq!(val(2), 1104);
        assert_eq!(val(3), 1073);
    }

    #[test]
    fn quarterly_reconciliation_uses_the_historical_calendar() {
        let evidence = vec![
            rf_dates(
                ("us-gaap", "Revenues"),
                "FY",
                d(2016, 1, 3),
                d(2016, 12, 31),
                400,
            ),
            rf_dates(
                ("us-gaap", "Revenues"),
                "FY",
                d(2017, 1, 1),
                d(2017, 12, 30),
                500,
            ),
            rf_dates(
                ("us-gaap", "Revenues"),
                "FY",
                d(2018, 2, 4),
                d(2019, 2, 2),
                600,
            ),
            rf_dates(
                ("us-gaap", "Revenues"),
                "FY",
                d(2019, 2, 3),
                d(2020, 2, 1),
                700,
            ),
            rf_dates(
                ("us-gaap", "Revenues"),
                "Q1",
                d(2017, 1, 1),
                d(2017, 4, 1),
                100,
            ),
            rf_dates(
                ("us-gaap", "Revenues"),
                "Q3",
                d(2017, 1, 1),
                d(2017, 9, 30),
                350,
            ),
            rf_dates(
                ("us-gaap", "Revenues"),
                "Q1",
                d(2018, 2, 4),
                d(2018, 5, 5),
                120,
            ),
            rf_dates(
                ("us-gaap", "Revenues"),
                "Q3",
                d(2018, 2, 4),
                d(2018, 11, 3),
                480,
            ),
        ];
        let calendar = FiscalCalendar::infer(&evidence, "0130");

        let (quarters, _) = reconcile_quarters(&evidence, &calendar);

        let old_q1 = quarters
            .iter()
            .find(|q| q.period_end == d(2017, 4, 1))
            .unwrap();
        assert_eq!((old_q1.fy, old_q1.fq), (2017, 1));
        assert_eq!(old_q1.fiscal_year_end, "1230");
        let new_q1 = quarters
            .iter()
            .find(|q| q.period_end == d(2018, 5, 5))
            .unwrap();
        assert_eq!((new_q1.fy, new_q1.fq), (2019, 1));
        assert_eq!(new_q1.fiscal_year_end, "0130");

        let old_q4 = quarters
            .iter()
            .find(|q| q.period_end == d(2017, 12, 30) && q.fq == 4)
            .unwrap();
        assert_eq!(old_q4.value, 150);
        let new_q4 = quarters
            .iter()
            .find(|q| q.period_end == d(2019, 2, 2) && q.fq == 4)
            .unwrap();
        assert_eq!(new_q4.value, 120);
    }
}
