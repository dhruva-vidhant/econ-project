//! Pipeline orchestrator. Wires Discover → Download → Parse → Normalize → Persist.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{NaiveDate, Utc};
use serde::Serialize;

use crate::domain::{
    AccessionNo, Cik, Company, DerivedMetric, Filing, FormType, IngestionEvent, Metric,
    NormalizedFact, Period, PeriodKind, RawFact, Severity, Ticker,
};
use crate::errors::{PipelineError, SourceError};
use crate::normalize::{concept_map, periods::reconcile_quarters};
use crate::repos::company::CompanyRepo;
use crate::repos::derived_metric::DerivedMetricRepo;
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

pub struct IngestionDeps {
    pub sec: Arc<SecClient>,
    pub companies: Arc<dyn CompanyRepo>,
    pub filings: Arc<dyn FilingRepo>,
    pub periods: Arc<dyn PeriodRepo>,
    pub raw_facts: Arc<dyn RawFactRepo>,
    pub normalized_facts: Arc<dyn NormalizedFactRepo>,
    pub derived_metrics: Arc<dyn DerivedMetricRepo>,
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
    // Foreign private issuers (e.g., BABA, NVO) get `fiscalYearEnd: null`
    // from `data.sec.gov/submissions/`. Derive it from the most recent
    // annual filing's `reportDate` so period reconciliation works for
    // any issuer.
    let resolved_fye = subs.resolved_fiscal_year_end();

    // Wire `amends` field: an amendment's accession_no is structurally
    // related to the original by SEC convention but we can't infer it
    // without parsing per-filing metadata. For now, leave amends=None;
    // supersession is selected by filed_at recency (resolution rule §8.1).
    // Future: read each amendment's filing index XML to extract `original-of`.

    let company = Company {
        cik: cik.clone(),
        ticker: ticker.clone(),
        name: name.clone(),
        exchange: subs.exchanges.first().cloned(),
        sic: None,
        fiscal_year_end: Some(resolved_fye.clone()),
        added_at: now,
        last_refreshed: Some(now),
    };
    deps.companies.upsert(&company).await?;

    let mut filings_ingested = 0usize;
    let mut by_accn: HashMap<AccessionNo, Filing> = HashMap::new();
    for f in &filings {
        deps.filings.upsert(f).await?;
        filings_ingested += 1;
        by_accn.insert(f.accession_no.clone(), f.clone());
    }
    drop(filings);

    // ─── Download + Parse ───────────────────────────────────────────────
    let cf = companyfacts::fetch_companyfacts(&deps.sec, &cik).await?;
    let raw_facts = companyfacts::to_raw_facts(&cik, &cf);

    let mut raw_to_insert: Vec<RawFact> = Vec::with_capacity(raw_facts.len());
    let mut placeholders_made = 0usize;
    for f in raw_facts {
        if !by_accn.contains_key(&f.accession_no) {
            let filing = Filing {
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
            format!("Created {placeholders_made} filing placeholders for accessions outside submissions.recent."),
            &mut events).await?;
    }
    let raw_facts_ingested = deps.raw_facts.upsert_many(&raw_to_insert).await?;

    // Re-load raw_facts grouped by accession so we have ids assigned.
    let mut all_raw: Vec<RawFact> = Vec::new();
    for accn in by_accn.keys() {
        let v = deps.raw_facts.list_for_filing(accn).await?;
        all_raw.extend(v);
    }

    // ─── Normalize ───────────────────────────────────────────────────────
    // The SEC companyfacts JSON tags every fact with `fy` / `fp` —
    // BUT those reflect the FILING's fiscal year, not the period the
    // fact actually represents. A 10-K for FY2025 typically embeds
    // 3 years of comparative data (e.g., 2023 + 2024 + 2025 numbers
    // tagged `fy:2025, fp:FY`), so trusting the SEC tag for our
    // Period.fiscal_year would systematically mis-label every period
    // by however many years the issuer's most recent 10-K spans.
    //
    // Source of truth: the period's own `end_date`, projected through
    // the issuer's fiscal-year-end calendar. See
    // Period::compute_fiscal_year.
    let fye = resolved_fye.as_str();
    let derive_fy = |end: NaiveDate| Period::compute_fiscal_year(end, fye);

    // (1) Quarterly facts via period reconciler.
    let (quarter_values, _fy_facts_from_quarters) = reconcile_quarters(&all_raw, fye);
    let mut normalized_count = 0usize;

    for q in quarter_values {
        // Guard: a derived value computed by subtracting two facts that
        // straddle a restatement can come out negative for a positive-only
        // metric. Per the project's accuracy rule we'd rather have an
        // explicit gap than a wrong number. Skip + diagnostic.
        if q.derived && q.value < 0 && is_positive_only(q.metric) {
            record_event(deps, &cik, None, "normalize", Severity::Warn, false,
                format!("Skipped derived {} for FY{} Q{}: result {} would be negative for a positive-only metric (likely cross-restatement input mismatch).",
                    q.metric.as_str(), q.fy, q.fq, q.value),
                &mut events).await?;
            continue;
        }
        let derived_fy = derive_fy(q.period_end);
        let period = Period {
            id: 0,
            cik: cik.clone(),
            fiscal_year: derived_fy,
            fiscal_quarter: q.fq,
            fiscal_year_end: resolved_fye.clone(),
            start_date: q.period_start,
            end_date: q.period_end,
            kind: PeriodKind::Quarterly,
            is_53_week: false,
        };
        let period_id = deps.periods.upsert_returning_id(&period).await?;
        let value = apply_sign(q.metric, q.value);
        let nf = NormalizedFact {
            id: 0, cik: cik.clone(), metric: q.metric, period_id,
            value, unit: "USD".into(),
            source_fact_id: q.source_fact_id, source_kind: q.source_kind,
            is_primary: true,
            original_value: None, original_unit: None,
            fx_rate_micro: None, fx_rate_source: None, fx_rate_date: None,
            superseded_by: None,
        };
        if deps.normalized_facts.insert_primary_with_supersession(&nf).await.is_ok() {
            normalized_count += 1;
            if q.derived {
                record_event(deps, &cik, None, "normalize", Severity::Info, false,
                    format!("Derived single-quarter {} for FY{} Q{} from YTD difference.",
                        q.metric.as_str(), derived_fy, q.fq),
                    &mut events).await?;
            }
        }
    }

    // (2) Annual + instant facts: pick the most-recently-filed per
    // (metric, derived_fy) — approximates §8.1 amendment-priority and
    // also collapses duplicate facts for the same calendar year that
    // appear under different SEC `fy` tags (the comparative-data case
    // described above). For amendments, insert_primary_with_supersession
    // updates the previous primary so the chain is preserved.
    type AnnualKey = (Metric, i32);
    let mut best_annual: HashMap<AnnualKey, &RawFact> = HashMap::new();
    type InstantKey = (Metric, i32, u8);
    let mut best_instant: HashMap<InstantKey, &RawFact> = HashMap::new();

    for f in &all_raw {
        let metric = match concept_map::metric_for(&f.taxonomy, &f.concept) {
            Some(m) => m,
            None => continue,
        };
        let derived_fy = derive_fy(f.period_end);
        if metric.is_instant() {
            // For instants we ignore both `fy` and `fp` — the SEC tags
            // both follow the filing's fiscal year, so a 2019-Q1 filing
            // emits a 2018-12-31 opening balance tagged `fp=Q1, fy=2019`,
            // which would land in our Q1-2018 bucket if we trusted it.
            // Period::compute_fiscal_quarter aligns on period_end month
            // against the issuer's fiscal calendar.
            let Some(fq) = Period::compute_fiscal_quarter(f.period_end, fye) else {
                continue;
            };
            let key = (metric, derived_fy, fq);
            let take = match best_instant.get(&key) {
                Some(prev) => f.filed > prev.filed,
                None => true,
            };
            if take { best_instant.insert(key, f); }
            continue;
        }
        // Duration facts: only the FY (annual) row goes through this
        // path; quarterly durations were handled by reconcile_quarters.
        // Use period span (~365 days, ending on the issuer's FYE) rather
        // than the SEC `fp` tag, which carries the filing's year and
        // would let an embedded comparative 10-K row sneak through.
        let Some(start) = f.period_start else { continue };
        let span_days = (f.period_end - start).num_days();
        let is_full_year = span_days >= 340 && span_days <= 380;
        let ends_on_fye =
            Period::compute_fiscal_quarter(f.period_end, fye) == Some(0);
        if !(is_full_year && ends_on_fye) { continue; }
        let key = (metric, derived_fy);
        let take = match best_annual.get(&key) {
            Some(prev) => f.filed > prev.filed,
            None => true,
        };
        if take { best_annual.insert(key, f); }
    }

    // Persist annual rows.
    for ((metric, derived_fy), f) in best_annual {
        let start = f.period_start.unwrap_or(f.period_end);
        let period = Period {
            id: 0, cik: cik.clone(),
            fiscal_year: derived_fy, fiscal_quarter: 0,
            fiscal_year_end: resolved_fye.clone(),
            start_date: start, end_date: f.period_end,
            kind: PeriodKind::Annual,
            is_53_week: Period::detect_53_week(start, f.period_end),
        };
        let period_id = deps.periods.upsert_returning_id(&period).await?;
        let value = apply_sign(metric, f.value_numeric);
        let nf = NormalizedFact {
            id: 0, cik: cik.clone(), metric, period_id,
            value, unit: f.unit.clone(),
            source_fact_id: f.id, source_kind: f.source_kind,
            is_primary: true,
            original_value: None, original_unit: None,
            fx_rate_micro: None, fx_rate_source: None, fx_rate_date: None,
            superseded_by: None,
        };
        if deps.normalized_facts.insert_primary_with_supersession(&nf).await.is_ok() {
            normalized_count += 1;
        }
    }

    // Persist instant rows. Try to attach to an existing period; create a
    // new annual stub if none.
    for ((metric, derived_fy, fq), f) in best_instant {
        let pid = match deps.periods.get_id(&cik, derived_fy, fq).await? {
            Some(id) => id,
            None => {
                // Create a placeholder period anchored at the instant date.
                let p = Period {
                    id: 0, cik: cik.clone(),
                    fiscal_year: derived_fy, fiscal_quarter: fq,
                    fiscal_year_end: resolved_fye.clone(),
                    start_date: f.period_end, end_date: f.period_end,
                    kind: if fq == 0 { PeriodKind::Annual } else { PeriodKind::Quarterly },
                    is_53_week: false,
                };
                deps.periods.upsert_returning_id(&p).await?
            }
        };
        let value = apply_sign(metric, f.value_numeric);
        let nf = NormalizedFact {
            id: 0, cik: cik.clone(), metric, period_id: pid,
            value, unit: f.unit.clone(),
            source_fact_id: f.id, source_kind: f.source_kind,
            is_primary: true,
            original_value: None, original_unit: None,
            fx_rate_micro: None, fx_rate_source: None, fx_rate_date: None,
            superseded_by: None,
        };
        if deps.normalized_facts.insert_primary_with_supersession(&nf).await.is_ok() {
            normalized_count += 1;
        }
    }

    // ── Bank revenue derivation ─────────────────────────────────────────
    // For periods where the canonical Revenue concept-map didn't yield
    // anything but bank-input metrics are present, derive Revenue per
    // the resolution order:
    //   2. us-gaap:Revenues               (handled by normal pipeline)
    //   3. NetInterestIncome + NoninterestIncome
    //   4. (InterestIncomeOperating - InterestExpense) + NoninterestIncome
    let derived_revenue_count = derive_bank_revenue(deps, &cik, &mut events).await?;

    record_event(deps, &cik, None, "persist", Severity::Info, false,
        format!("Ingestion complete: {filings_ingested} filings, {raw_facts_ingested} raw facts, {normalized_count} normalized facts, {derived_revenue_count} bank-revenue derivations."),
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

fn apply_sign(metric: Metric, value: i64) -> i64 {
    match metric {
        Metric::CapitalExpenditures => value.abs(),
        _ => value,
    }
}

/// Whether a metric should never legitimately be negative. Used to guard
/// derived (subtraction-based) quarter values against cross-restatement
/// inconsistencies — a negative result for these metrics indicates the
/// inputs came from different restate states; skipping is more accurate
/// than persisting a known-wrong value.
fn is_positive_only(metric: Metric) -> bool {
    matches!(
        metric,
        Metric::Revenue
        | Metric::CostOfRevenue
        | Metric::GrossProfit
        | Metric::SharesOutstandingBasic
        | Metric::SharesOutstandingDiluted
        | Metric::CashAndEquivalents
        | Metric::LongTermDebt
        | Metric::CurrentDebt
        | Metric::TotalDebt
        | Metric::TotalAssets
        | Metric::TotalLiabilities
        | Metric::CapitalExpenditures
        | Metric::DepreciationAmortization
        | Metric::HistoricalMarketCap
        | Metric::CurrentMarketCap
    )
}

/// Per the bank-revenue resolution order:
///   1. (deferred — explicit bank total revenue concept; see followup.md)
///   2. `us-gaap:Revenues` (already handled by the canonical concept map)
///   3. `NetInterestIncome + NoninterestIncome`
///   4. `(InterestIncomeOperating - InterestExpense) + NoninterestIncome`
///
/// Steps 3 and 4 fire only when no direct Revenue normalized_fact exists
/// for a (cik, period_id). Results are persisted to `derived_metric` with
/// `formula_id = "bank_revenue_v1"` and merged into Revenue queries at
/// read time (see ipc::commands::get_metric_history /
/// ipc::commands::get_dashboard).
async fn derive_bank_revenue(
    deps: &IngestionDeps,
    cik: &Cik,
    events: &mut usize,
) -> Result<usize, PipelineError> {
    let mut count = 0usize;
    let periods = deps.periods.list_for_cik(cik, None).await?;
    for p in periods {
        // Skip periods that already have a direct Revenue value.
        if deps
            .normalized_facts
            .current_value(cik, Metric::Revenue, p.id)
            .await?
            .is_some()
        {
            continue;
        }
        // Look up the bank inputs.
        let nii = deps.normalized_facts.current_value(cik, Metric::NetInterestIncome, p.id).await?;
        let noni = deps.normalized_facts.current_value(cik, Metric::NoninterestIncome, p.id).await?;
        let iio = deps.normalized_facts.current_value(cik, Metric::InterestIncomeOperating, p.id).await?;
        let ie = deps.normalized_facts.current_value(cik, Metric::InterestExpense, p.id).await?;

        let derived = match (nii.as_ref(), noni.as_ref(), iio.as_ref(), ie.as_ref()) {
            // Resolution order step 3: NetInterestIncome + NoninterestIncome
            (Some(nii), Some(noni), _, _) => Some((nii.value.saturating_add(noni.value), "step3")),
            // Step 4: (InterestIncomeOperating - InterestExpense) + NoninterestIncome
            (None, Some(noni), Some(iio), Some(ie)) => {
                let net = iio.value.saturating_sub(ie.value);
                Some((net.saturating_add(noni.value), "step4"))
            }
            _ => None,
        };

        if let Some((value, step)) = derived {
            // Sanity guard: bank revenue is always positive. Skip + log if not.
            if value <= 0 {
                record_event(
                    deps, cik, None, "normalize", Severity::Warn, false,
                    format!(
                        "Skipped bank-revenue derivation for FY{} Q{} ({}): value {} ≤ 0",
                        p.fiscal_year, p.fiscal_quarter, step, value,
                    ),
                    events,
                ).await?;
                continue;
            }
            let dm = DerivedMetric {
                id: 0,
                cik: cik.clone(),
                formula_id: "bank_revenue_v1".to_string(),
                period_id: p.id,
                value: Some(value),
                is_complete: true,
            };
            deps.derived_metrics.upsert(&dm).await?;
            count += 1;
        }
    }
    Ok(count)
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
        assert_eq!(apply_sign(Metric::CapitalExpenditures, -1_000_000), 1_000_000);
        assert_eq!(apply_sign(Metric::CapitalExpenditures, 1_000_000), 1_000_000);
    }

    #[test]
    fn other_metrics_keep_sign() {
        assert_eq!(apply_sign(Metric::Revenue, 1_500_000), 1_500_000);
        assert_eq!(apply_sign(Metric::NetIncome, -100_000), -100_000);
    }
}
