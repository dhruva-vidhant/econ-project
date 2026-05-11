//! Pipeline orchestrator. Wires Discover → Download → Parse → Normalize → Persist.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{NaiveDate, Utc};
use serde::Serialize;

use crate::domain::{
    AccessionNo, Cik, Company, FormType, IngestionEvent, NormalizedFact, Period, PeriodKind,
    RawFact, Severity, SourceKind, Ticker,
};
use crate::errors::{PipelineError, SourceError};
use crate::normalize::concept_map;
use crate::repos::company::CompanyRepo;
use crate::repos::filing::FilingRepo;
use crate::repos::ingestion_event::IngestionEventRepo;
use crate::repos::normalized_fact::NormalizedFactRepo;
use crate::repos::period::PeriodRepo;
use crate::repos::raw_fact::RawFactRepo;
use crate::sources::{companyfacts, sec_client::SecClient, submissions, tickers::TickerMap};

#[derive(Clone, Debug, Serialize)]
pub struct IngestionSummary {
    pub cik: String,
    pub ticker: String,
    pub name: String,
    pub filings_ingested: usize,
    pub raw_facts_ingested: usize,
    pub normalized_facts_ingested: usize,
    pub events_recorded: usize,
}

/// Repository handles needed by the pipeline.
pub struct IngestionDeps {
    pub sec: Arc<SecClient>,
    pub companies: Arc<dyn CompanyRepo>,
    pub filings: Arc<dyn FilingRepo>,
    pub periods: Arc<dyn PeriodRepo>,
    pub raw_facts: Arc<dyn RawFactRepo>,
    pub normalized_facts: Arc<dyn NormalizedFactRepo>,
    pub events: Arc<dyn IngestionEventRepo>,
}

pub async fn ingest_company(
    deps: &IngestionDeps,
    ticker: &Ticker,
) -> Result<(Company, IngestionSummary), PipelineError> {
    let mut events = 0usize;
    let now = Utc::now();

    // ─── Discover ───────────────────────────────────────────────────────
    let ticker_map = TickerMap::load(&deps.sec).await?;
    let (cik, name) = ticker_map
        .lookup(ticker)
        .ok_or_else(|| SourceError::UnknownTicker(ticker.0.clone()))?;

    let subs = submissions::fetch_submissions(&deps.sec, &cik).await?;
    let filings = submissions::to_filings(&cik, &subs);

    // Persist Company first so FKs work for everything else.
    let company = Company {
        cik: cik.clone(),
        ticker: ticker.clone(),
        name: name.clone(),
        exchange: subs.exchanges.first().cloned(),
        sic: None,
        fiscal_year_end: Some(subs.fiscal_year_end.clone()),
        added_at: now,
        last_refreshed: Some(now),
    };
    deps.companies.upsert(&company).await?;

    // Persist filings before facts (raw_fact has FK to filing.accession_no).
    let mut filings_ingested = 0usize;
    let mut by_accn: HashMap<AccessionNo, crate::domain::Filing> = HashMap::new();
    for f in &filings {
        deps.filings.upsert(f).await?;
        filings_ingested += 1;
        by_accn.insert(f.accession_no.clone(), f.clone());
    }

    // ─── Download + Parse ───────────────────────────────────────────────
    let cf = companyfacts::fetch_companyfacts(&deps.sec, &cik).await?;
    let raw_facts = companyfacts::to_raw_facts(&cik, &cf);

    // We can only insert raw_fact rows whose accession_no exists in the filing
    // table (FK). companyfacts can reference accessions older than what
    // submissions.recent returns (1000-row pagination); insert filing
    // placeholders for any unknown accession.
    let mut raw_to_insert: Vec<RawFact> = Vec::with_capacity(raw_facts.len());
    let mut placeholders_made = 0usize;
    for f in raw_facts {
        if !by_accn.contains_key(&f.accession_no) {
            // Fabricate a minimal filing row so the FK resolves. Form type "other".
            let filing = crate::domain::Filing {
                accession_no: f.accession_no.clone(),
                cik: cik.clone(),
                form_type: FormType::Other("unknown".into()),
                filed_at: f.filed.unwrap_or(NaiveDate::from_ymd_opt(1970, 1, 1).unwrap()),
                period_of_report: Some(f.period_end),
                is_amendment: false,
                amends: None,
                item_4_02_8k: false,
            };
            deps.filings.upsert(&filing).await?;
            by_accn.insert(filing.accession_no.clone(), filing.clone());
            placeholders_made += 1;
        }
        raw_to_insert.push(f);
    }
    if placeholders_made > 0 {
        record_event(deps, &cik, None, "discover", Severity::Info, false,
            format!("Created {placeholders_made} filing placeholders for accessions referenced by companyfacts beyond submissions.recent window."), &mut events).await?;
    }

    let raw_facts_ingested = deps.raw_facts.upsert_many(&raw_to_insert).await?;

    // Re-load raw_facts so we have their assigned ids per accession (we need
    // source_fact_id to link normalized_fact rows).
    let mut raw_by_accn: HashMap<AccessionNo, Vec<RawFact>> = HashMap::new();
    for accn in by_accn.keys() {
        let v = deps.raw_facts.list_for_filing(accn).await?;
        raw_by_accn.insert(accn.clone(), v);
    }

    // ─── Normalize ───────────────────────────────────────────────────────
    let mut normalized_count = 0usize;
    // Group raw facts by (metric, period boundaries) and pick a primary.
    // V1 strategy: prefer the most-recently-filed (`filed`) source for any given
    // (metric, period_end / period_start) tuple. This roughly approximates the
    // §8.1 resolution rules without yet implementing supersession-chain updates.
    type Key = (crate::domain::Metric, Option<NaiveDate>, NaiveDate);
    let mut best: HashMap<Key, RawFact> = HashMap::new();
    for facts in raw_by_accn.values() {
        for f in facts {
            if let Some(metric) = concept_map::metric_for(&f.taxonomy, &f.concept) {
                let key = (metric, f.period_start, f.period_end);
                let take = match best.get(&key) {
                    Some(existing) => f.filed > existing.filed,
                    None => true,
                };
                if take {
                    best.insert(key, f.clone());
                }
            }
        }
    }

    // Persist Period rows + NormalizedFact rows.
    let fye = subs.fiscal_year_end.clone();
    for ((metric, period_start, period_end), f) in best {
        // V1 period reconciliation: derive (fy, fq) from fp + fy when present;
        // skip when ambiguous (the diagnostic event surfaces it).
        let (fy, fq, kind) = match (f.fy, f.fp.as_deref()) {
            (Some(fy), Some("FY")) => (fy, 0u8, PeriodKind::Annual),
            (Some(fy), Some("Q1")) => (fy, 1u8, PeriodKind::Quarterly),
            (Some(fy), Some("Q2")) => (fy, 2u8, PeriodKind::Quarterly),
            (Some(fy), Some("Q3")) => (fy, 3u8, PeriodKind::Quarterly),
            (Some(fy), Some("Q4")) => (fy, 4u8, PeriodKind::Quarterly),
            _ => continue, // skip non-canonical periods (CY frames, etc.)
        };

        // Heuristic: skip YTD-style facts where period_end - period_start
        // exceeds a typical quarter window for non-FY rows. (Proper YTD
        // derivation is deferred — see `docs/followup.md`.)
        if matches!(kind, PeriodKind::Quarterly) {
            if let Some(start) = period_start {
                let days = (period_end - start).num_days();
                if days > 100 {
                    record_event(
                        deps, &cik, Some(&f.accession_no), "normalize", Severity::Info, false,
                        format!("Skipped YTD-style fact for {:?} {:?} (span {} days). Single-quarter derivation deferred to follow-up pass.", metric, f.period_start, days),
                        &mut events).await?;
                    continue;
                }
            }
        }

        let start = period_start.unwrap_or(period_end);
        let period = Period {
            id: 0,
            cik: cik.clone(),
            fiscal_year: fy,
            fiscal_quarter: fq,
            fiscal_year_end: fye.clone(),
            start_date: start,
            end_date: period_end,
            kind: kind.clone(),
            is_53_week: false,
        };
        let period_id = deps.periods.upsert_returning_id(&period).await?;

        let value = apply_sign(metric, f.value_numeric);
        let n = NormalizedFact {
            id: 0,
            cik: cik.clone(),
            metric,
            period_id,
            value,
            unit: f.unit.clone(),
            source_fact_id: f.id,
            source_kind: SourceKind::XbrlApi,
            is_primary: true,
            original_value: None,
            original_unit: None,
            fx_rate_micro: None,
            fx_rate_source: None,
            fx_rate_date: None,
            superseded_by: None,
        };
        match deps.normalized_facts.insert_primary_with_supersession(&n).await {
            Ok(_) => { normalized_count += 1; }
            Err(_) => {
                // Constraint violation (e.g., duplicate due to retry): skip.
                continue;
            }
        }
    }

    record_event(deps, &cik, None, "persist", Severity::Info, false,
        format!("Ingestion complete: {filings_ingested} filings, {raw_facts_ingested} raw facts, {normalized_count} normalized facts."),
        &mut events).await?;

    deps.companies.touch_refreshed(&cik).await?;
    let summary = IngestionSummary {
        cik: cik.0.clone(),
        ticker: ticker.0.clone(),
        name,
        filings_ingested,
        raw_facts_ingested,
        normalized_facts_ingested: normalized_count,
        events_recorded: events,
    };
    Ok((company, summary))
}

/// Apply the §6.2 sign convention (mostly identity for V1).
fn apply_sign(metric: crate::domain::Metric, value: i64) -> i64 {
    use crate::domain::Metric;
    match metric {
        // CapEx is reported as a payment (typically positive for "cash outflow")
        // or as a negative cash-flow line; normalize to positive at storage.
        Metric::CapitalExpenditures => value.abs(),
        _ => value,
    }
}

async fn record_event(
    deps: &IngestionDeps,
    cik: &Cik,
    accn: Option<&AccessionNo>,
    stage: &str,
    level: Severity,
    user_visible: bool,
    message: String,
    counter: &mut usize,
) -> Result<(), PipelineError> {
    let e = IngestionEvent {
        id: 0,
        cik: Some(cik.clone()),
        accession_no: accn.cloned(),
        stage: stage.into(),
        level,
        user_visible,
        message,
        detail_json: None,
        occurred_at: Utc::now(),
    };
    deps.events.record(&e).await?;
    *counter += 1;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capex_sign_is_normalized_positive() {
        assert_eq!(apply_sign(crate::domain::Metric::CapitalExpenditures, -1_000_000), 1_000_000);
        assert_eq!(apply_sign(crate::domain::Metric::CapitalExpenditures, 1_000_000), 1_000_000);
    }

    #[test]
    fn other_metrics_keep_sign() {
        assert_eq!(apply_sign(crate::domain::Metric::Revenue, 1_500_000), 1_500_000);
        assert_eq!(apply_sign(crate::domain::Metric::NetIncome, -100_000), -100_000);
    }
}
