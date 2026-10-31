//! Pipeline orchestrator. Wires Discover → Download → Parse → Normalize → Persist.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{NaiveDate, Utc};
use serde::Serialize;

use crate::domain::{
    AccessionNo, Cik, Company, DerivedMetric, Filing, FormType, IngestionEvent, Metric,
    NormalizedFact, Period, PeriodKind, RawFact, Severity, Ticker,
};
use crate::errors::{PipelineError, RepoError, SourceError};
use crate::import::resolve::{resolve_period, Resolution};
use crate::normalize::{concept_map, fiscal_calendar::FiscalCalendar, periods::reconcile_quarters};
use crate::repos::company::CompanyRepo;
use crate::repos::current_price::CurrentPriceRepo;
use crate::repos::custom_metric::CustomMetricRepo;
use crate::repos::derived_metric::DerivedMetricRepo;
use crate::repos::filing::FilingRepo;
use crate::repos::historical_price::HistoricalPriceRepo;
use crate::repos::ingestion_event::IngestionEventRepo;
use crate::repos::normalized_fact::NormalizedFactRepo;
use crate::repos::period::PeriodRepo;
use crate::repos::raw_fact::RawFactRepo;
use crate::sources::market_data::MarketDataAdapter;
use crate::sources::{
    companyfacts, legacy_html, sec_client::SecClient, submissions, tickers::TickerMap,
};

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
    pub market_data: Arc<dyn MarketDataAdapter>,
    pub companies: Arc<dyn CompanyRepo>,
    pub filings: Arc<dyn FilingRepo>,
    pub periods: Arc<dyn PeriodRepo>,
    pub raw_facts: Arc<dyn RawFactRepo>,
    pub normalized_facts: Arc<dyn NormalizedFactRepo>,
    pub derived_metrics: Arc<dyn DerivedMetricRepo>,
    pub prices: Arc<dyn HistoricalPriceRepo>,
    pub current_prices: Arc<dyn CurrentPriceRepo>,
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
    let mut raw_facts = companyfacts::to_raw_facts(&cik, &cf);
    let legacy_filings =
        legacy_html::fetch_configured_history(&deps.sec, &cik, &subs.filings.files).await?;
    if !legacy_filings.is_empty() {
        let mut legacy_count = 0;
        for (filing, legacy_facts) in legacy_filings {
            legacy_count += legacy_facts.len();
            raw_facts.extend(legacy_facts);
            deps.filings.upsert(&filing).await?;
            filings_ingested += 1;
            by_accn.insert(filing.accession_no.clone(), filing);
        }
        record_event(
            deps,
            &cik,
            None,
            "download",
            Severity::Info,
            false,
            format!("Retrieved {legacy_count} facts from configured pre-XBRL public filings."),
            &mut events,
        )
        .await?;
    }

    let mut raw_to_insert: Vec<RawFact> = Vec::with_capacity(raw_facts.len());
    let mut placeholders_made = 0usize;
    for f in raw_facts {
        if !by_accn.contains_key(&f.accession_no) {
            let filing = Filing {
                accession_no: f.accession_no.clone(),
                cik: cik.clone(),
                form_type: FormType::Other("unknown".into()),
                filed_at: f
                    .filed
                    .unwrap_or(NaiveDate::from_ymd_opt(1970, 1, 1).unwrap()),
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
    let calendar = FiscalCalendar::infer(&all_raw, &resolved_fye);
    if calendar.segments().len() > 1 {
        let regimes = calendar
            .segments()
            .iter()
            .map(|segment| match segment.effective_from {
                Some(date) => format!("{} from {date}", segment.fiscal_year_end),
                None => format!("{} historically", segment.fiscal_year_end),
            })
            .collect::<Vec<_>>()
            .join(", ");
        record_event(
            deps,
            &cik,
            None,
            "normalize",
            Severity::Info,
            false,
            format!("Detected fiscal-calendar history: {regimes}."),
            &mut events,
        )
        .await?;
    }

    let existing_periods = deps.periods.list_for_cik(&cik, None).await?;
    let classification_changed = existing_periods.iter().any(|period| {
        let fiscal = calendar.period_for(period.end_date);
        fiscal.fiscal_year != period.fiscal_year
            || fiscal.fiscal_year_end != period.fiscal_year_end
            || (period.fiscal_quarter < 4 && fiscal.fiscal_quarter != Some(period.fiscal_quarter))
    });
    if classification_changed {
        deps.periods
            .reset_normalized_for_reclassification(&cik)
            .await?;
        record_event(
            deps,
            &cik,
            None,
            "normalize",
            Severity::Info,
            false,
            "Rebuilt normalized periods after fiscal-calendar classification changed.".into(),
            &mut events,
        )
        .await?;
    }

    // (1) Quarterly facts via period reconciler.
    let (quarter_values, _fy_facts_from_quarters) = reconcile_quarters(&all_raw, &calendar);
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
        let period = Period {
            id: 0,
            cik: cik.clone(),
            fiscal_year: q.fy,
            fiscal_quarter: q.fq,
            fiscal_year_end: q.fiscal_year_end.clone(),
            start_date: q.period_start,
            end_date: q.period_end,
            kind: PeriodKind::Quarterly,
            is_53_week: false,
        };
        let period_id = deps.periods.upsert_returning_id(&period).await?;
        let value = apply_sign(q.metric, q.value);
        let nf = NormalizedFact {
            id: 0,
            cik: cik.clone(),
            metric: q.metric,
            period_id,
            value,
            unit: "USD".into(),
            source_fact_id: q.source_fact_id,
            source_kind: q.source_kind,
            is_primary: true,
            original_value: None,
            original_unit: None,
            fx_rate_micro: None,
            fx_rate_source: None,
            fx_rate_date: None,
            superseded_by: None,
        };
        if deps
            .normalized_facts
            .insert_primary_with_supersession(&nf)
            .await
            .is_ok()
        {
            normalized_count += 1;
            if q.derived {
                record_event(
                    deps,
                    &cik,
                    None,
                    "normalize",
                    Severity::Info,
                    false,
                    format!(
                        "Derived single-quarter {} for FY{} Q{} from YTD difference.",
                        q.metric.as_str(),
                        q.fy,
                        q.fq
                    ),
                    &mut events,
                )
                .await?;
            }
        }
    }

    // (2) Annual + instant facts: pick the most-recently-filed per
    // (metric, derived_fy) — approximates §8.1 amendment-priority and
    // also collapses duplicate facts for the same calendar year that
    // appear under different SEC `fy` tags (the comparative-data case
    // described above). For amendments, insert_primary_with_supersession
    // updates the previous primary so the chain is preserved.
    type AnnualKey = (Metric, usize, i32);
    let mut best_annual: HashMap<AnnualKey, &RawFact> = HashMap::new();
    type InstantKey = (Metric, usize, i32, u8);
    let mut best_instant: HashMap<InstantKey, &RawFact> = HashMap::new();

    for f in &all_raw {
        let metric = match concept_map::metric_for(&f.taxonomy, &f.concept) {
            Some(m) => m,
            None => continue,
        };
        let fiscal = calendar.period_for(f.period_end);
        if metric.is_instant() {
            // For instants we ignore both `fy` and `fp` — the SEC tags
            // both follow the filing's fiscal year, so a 2019-Q1 filing
            // emits a 2018-12-31 opening balance tagged `fp=Q1, fy=2019`,
            // which would land in our Q1-2018 bucket if we trusted it.
            // Period::compute_fiscal_quarter aligns on period_end month
            // against the issuer's fiscal calendar.
            let Some(fq) = fiscal.fiscal_quarter else {
                continue;
            };
            let key = (metric, fiscal.segment_id, fiscal.fiscal_year, fq);
            let take = match best_instant.get(&key) {
                Some(prev) => f.filed > prev.filed,
                None => true,
            };
            if take {
                best_instant.insert(key, f);
            }
            continue;
        }
        // Duration facts: only the FY (annual) row goes through this
        // path; quarterly durations were handled by reconcile_quarters.
        // Use period span (~365 days, ending on the issuer's FYE) rather
        // than the SEC `fp` tag, which carries the filing's year and
        // would let an embedded comparative 10-K row sneak through.
        let Some(start) = f.period_start else {
            continue;
        };
        let span_days = (f.period_end - start).num_days();
        let is_full_year = span_days >= 340 && span_days <= 380;
        let ends_on_fye = fiscal.fiscal_quarter == Some(0);
        if !(is_full_year && ends_on_fye) {
            continue;
        }
        let key = (metric, fiscal.segment_id, fiscal.fiscal_year);
        let take = match best_annual.get(&key) {
            Some(prev) => f.filed > prev.filed,
            None => true,
        };
        if take {
            best_annual.insert(key, f);
        }
    }

    // Persist annual rows.
    for ((metric, _segment_id, derived_fy), f) in best_annual {
        let start = f.period_start.unwrap_or(f.period_end);
        let fiscal = calendar.period_for(f.period_end);
        let period = Period {
            id: 0,
            cik: cik.clone(),
            fiscal_year: derived_fy,
            fiscal_quarter: 0,
            fiscal_year_end: fiscal.fiscal_year_end,
            start_date: start,
            end_date: f.period_end,
            kind: PeriodKind::Annual,
            is_53_week: Period::detect_53_week(start, f.period_end),
        };
        let period_id = deps.periods.upsert_returning_id(&period).await?;
        let value = apply_sign(metric, f.value_numeric);
        let nf = NormalizedFact {
            id: 0,
            cik: cik.clone(),
            metric,
            period_id,
            value,
            unit: f.unit.clone(),
            source_fact_id: f.id,
            source_kind: f.source_kind,
            is_primary: true,
            original_value: None,
            original_unit: None,
            fx_rate_micro: None,
            fx_rate_source: None,
            fx_rate_date: None,
            superseded_by: None,
        };
        if deps
            .normalized_facts
            .insert_primary_with_supersession(&nf)
            .await
            .is_ok()
        {
            normalized_count += 1;
        }
    }

    // Persist instant rows. Try to attach to an existing period; create a
    // new annual stub if none.
    for ((metric, _segment_id, derived_fy, fq), f) in best_instant {
        let fiscal = calendar.period_for(f.period_end);
        let pid = match deps.periods.get_id(&cik, derived_fy, fq).await? {
            Some(id) => id,
            None => {
                // Create a placeholder period anchored at the instant date.
                let p = Period {
                    id: 0,
                    cik: cik.clone(),
                    fiscal_year: derived_fy,
                    fiscal_quarter: fq,
                    fiscal_year_end: fiscal.fiscal_year_end,
                    start_date: f.period_end,
                    end_date: f.period_end,
                    kind: if fq == 0 {
                        PeriodKind::Annual
                    } else {
                        PeriodKind::Quarterly
                    },
                    is_53_week: false,
                };
                deps.periods.upsert_returning_id(&p).await?
            }
        };
        let value = apply_sign(metric, f.value_numeric);
        let nf = NormalizedFact {
            id: 0,
            cik: cik.clone(),
            metric,
            period_id: pid,
            value,
            unit: f.unit.clone(),
            source_fact_id: f.id,
            source_kind: f.source_kind,
            is_primary: true,
            original_value: None,
            original_unit: None,
            fx_rate_micro: None,
            fx_rate_source: None,
            fx_rate_date: None,
            superseded_by: None,
        };
        if deps
            .normalized_facts
            .insert_primary_with_supersession(&nf)
            .await
            .is_ok()
        {
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

    // ── Market-data enrichment ──────────────────────────────────────────
    // Fetch end-of-day closes for each period end-date so market cap and
    // FCF yield can be derived at read time. Best-effort: a price-source
    // failure is recorded but does not fail ingestion.
    let prices_ingested = fetch_prices(deps, &cik, ticker, &mut events).await?;

    record_event(deps, &cik, None, "persist", Severity::Info, false,
        format!("Ingestion complete: {filings_ingested} filings, {raw_facts_ingested} raw facts, {normalized_count} normalized facts, {derived_revenue_count} bank-revenue derivations, {prices_ingested} price points."),
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

/// Re-resolve a company's unaligned custom-metric points against its
/// (possibly newly ingested) periods. See techspec §5. Called from the IPC
/// layer after `ingest_company` returns, rather than threaded through
/// `IngestionDeps`, so the authoritative pipeline struct and its existing
/// integration tests stay untouched — this is a satellite step, not part of
/// the fact-persistence chain itself.
///
/// A stored point carries no `granularity` column (see
/// `db/migrations/V2__custom_metrics.sql`), so it is inferred by trying
/// `Annual` first and falling back to `Quarterly`. Fiscal-label tokens
/// (`"FY2024 Q2"`) self-disambiguate via `resolve_period`'s quarter
/// detection regardless of which granularity is passed in; only bare dates
/// depend on this ordering, and annual is the more common case for custom
/// KPIs.
pub async fn reresolve_custom_points(
    periods_repo: &dyn PeriodRepo,
    custom_metrics: &dyn CustomMetricRepo,
    cik: &Cik,
) -> Result<usize, RepoError> {
    let periods = periods_repo.list_for_cik(cik, None).await?;
    let unaligned = custom_metrics.points_unaligned(cik).await?;
    let mut realigned = 0usize;
    for point in unaligned {
        let Some(token) = point.observed_date.as_deref() else {
            continue;
        };
        let resolution = match resolve_period(token, PeriodKind::Annual, &periods) {
            Resolution::Unaligned => resolve_period(token, PeriodKind::Quarterly, &periods),
            other => other,
        };
        if let Resolution::Matched(period_id) = resolution {
            custom_metrics.realign_point(point.id, period_id).await?;
            realigned += 1;
        }
    }
    Ok(realigned)
}

/// Fetch end-of-day closes for each distinct period end-date and persist one
/// `historical_price` row per date, resolved to the nearest prior trading day
/// (so a fiscal close that lands on a weekend or holiday still gets a price).
///
/// Price enrichment is **best-effort**: a market-data failure (offline,
/// rate-limited, non-USD listing, unknown symbol) is recorded as a
/// user-visible warning and returns 0 — financial facts are already
/// persisted, and market cap / FCF yield simply remain absent until a later
/// refresh succeeds.
async fn fetch_prices(
    deps: &IngestionDeps,
    cik: &Cik,
    ticker: &Ticker,
    events: &mut usize,
) -> Result<usize, PipelineError> {
    use std::collections::BTreeMap;

    let periods = deps.periods.list_for_cik(cik, None).await?;
    let mut end_dates: Vec<NaiveDate> = periods.iter().map(|p| p.end_date).collect();
    end_dates.sort_unstable();
    end_dates.dedup();
    let (Some(&first), Some(&last)) = (end_dates.first(), end_dates.last()) else {
        return Ok(0);
    };
    // Reach back a week so the earliest period end can resolve to a prior
    // trading day even if it fell on a weekend/holiday.
    let from = first - chrono::Duration::days(7);

    let daily = match deps.market_data.historical_prices(ticker, from, last).await {
        Ok(d) => d,
        Err(e) => {
            record_event(
                deps,
                cik,
                None,
                "prices",
                Severity::Warn,
                true,
                format!(
                    "Price history unavailable ({e}); market cap and FCF yield will be absent."
                ),
                events,
            )
            .await?;
            return Ok(0);
        }
    };
    let by_date: BTreeMap<NaiveDate, i64> = daily.into_iter().collect();
    if by_date.is_empty() {
        return Ok(0);
    }

    let mut written = 0usize;
    for end in end_dates {
        // Nearest close on-or-before the period end (weekend/holiday-safe).
        if let Some((_, &close)) = by_date.range(..=end).next_back() {
            deps.prices
                .upsert(cik, end, &ticker.0, close, "yahoo")
                .await?;
            written += 1;
        }
    }

    // Also fetch and persist the spot price for current valuation metrics.
    match deps.market_data.current_price(ticker).await {
        Ok(spot_micro) => {
            deps.current_prices
                .upsert(cik, &ticker.0, spot_micro, chrono::Utc::now(), "yahoo")
                .await?;
        }
        Err(e) => {
            record_event(
                deps,
                cik,
                None,
                "prices",
                Severity::Warn,
                true,
                format!("Spot price unavailable ({e}); current market cap and current FCF yield will be absent."),
                events,
            )
            .await?;
        }
    }

    record_event(
        deps,
        cik,
        None,
        "prices",
        Severity::Info,
        false,
        format!("Fetched {written} end-of-day closes for market-cap derivation."),
        events,
    )
    .await?;
    Ok(written)
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
        let nii = deps
            .normalized_facts
            .current_value(cik, Metric::NetInterestIncome, p.id)
            .await?;
        let noni = deps
            .normalized_facts
            .current_value(cik, Metric::NoninterestIncome, p.id)
            .await?;
        let iio = deps
            .normalized_facts
            .current_value(cik, Metric::InterestIncomeOperating, p.id)
            .await?;
        let ie = deps
            .normalized_facts
            .current_value(cik, Metric::InterestExpense, p.id)
            .await?;

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
                    deps,
                    cik,
                    None,
                    "normalize",
                    Severity::Warn,
                    false,
                    format!(
                        "Skipped bank-revenue derivation for FY{} Q{} ({}): value {} ≤ 0",
                        p.fiscal_year, p.fiscal_quarter, step, value,
                    ),
                    events,
                )
                .await?;
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
        assert_eq!(
            apply_sign(Metric::CapitalExpenditures, -1_000_000),
            1_000_000
        );
        assert_eq!(
            apply_sign(Metric::CapitalExpenditures, 1_000_000),
            1_000_000
        );
    }

    #[test]
    fn other_metrics_keep_sign() {
        assert_eq!(apply_sign(Metric::Revenue, 1_500_000), 1_500_000);
        assert_eq!(apply_sign(Metric::NetIncome, -100_000), -100_000);
    }

    #[tokio::test]
    async fn reresolution_on_refresh() {
        use crate::db::Pool;
        use crate::domain::{Company, NewCustomMetric, Ticker, UnitKind};
        use crate::repos::company::{CompanyRepo, SqliteCompanyRepo};
        use crate::repos::custom_metric::SqliteCustomMetricRepo;
        use crate::repos::period::SqlitePeriodRepo;
        use chrono::{NaiveDate, TimeZone, Utc};

        let dir = tempfile::tempdir().unwrap();
        let pool = std::sync::Arc::new(Pool::open(&dir.path().join("test.sqlite")).unwrap());

        let cik = Cik("0000000001".into());
        let companies = SqliteCompanyRepo::new(pool.clone());
        companies
            .upsert(&Company {
                cik: cik.clone(),
                ticker: Ticker("TEST".into()),
                name: "Test Co".into(),
                exchange: None,
                sic: None,
                fiscal_year_end: Some("1231".into()),
                added_at: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
                last_refreshed: None,
            })
            .await
            .unwrap();

        let periods = SqlitePeriodRepo::new(pool.clone());
        let custom_metrics = SqliteCustomMetricRepo::new(pool.clone());
        let metric = custom_metrics
            .create(
                &cik,
                &NewCustomMetric {
                    key: "tpv".into(),
                    label: "Total Payment Volume".into(),
                    unit_kind: UnitKind::Currency,
                    currency: Some("USD".into()),
                    decimals: Some(2),
                    description: None,
                },
            )
            .await
            .unwrap();

        // Import a point dated in a not-yet-ingested period: unaligned.
        let pt = custom_metrics
            .upsert_point(
                metric.id,
                None,
                Some("2024-06-01"),
                1_000_000,
                Some("manual"),
            )
            .await
            .unwrap();
        assert_eq!(
            custom_metrics.points_unaligned(&cik).await.unwrap().len(),
            1
        );

        let n = reresolve_custom_points(&periods, &custom_metrics, &cik)
            .await
            .unwrap();
        assert_eq!(n, 0, "still unaligned — no matching period ingested yet");

        // Add the period (simulating a refresh that ingests FY2024) and rerun the hook.
        let period_id = periods
            .upsert_returning_id(&Period {
                id: 0,
                cik: cik.clone(),
                fiscal_year: 2024,
                fiscal_quarter: 0,
                fiscal_year_end: "1231".into(),
                start_date: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
                end_date: NaiveDate::from_ymd_opt(2024, 12, 31).unwrap(),
                kind: PeriodKind::Annual,
                is_53_week: false,
            })
            .await
            .unwrap();

        let n = reresolve_custom_points(&periods, &custom_metrics, &cik)
            .await
            .unwrap();
        assert_eq!(n, 1);
        assert_eq!(
            custom_metrics.points_unaligned(&cik).await.unwrap().len(),
            0
        );

        let aligned = custom_metrics
            .points_aligned(metric.id, PeriodKind::Annual)
            .await
            .unwrap();
        assert_eq!(aligned.len(), 1);
        assert_eq!(aligned[0].1.id, pt.id);
        assert_eq!(aligned[0].0.id, period_id);
    }

    #[tokio::test]
    async fn aligned_points_survive_period_rebuild() {
        use crate::db::Pool;
        use crate::domain::{Company, NewCustomMetric, Ticker, UnitKind};
        use crate::repos::company::{CompanyRepo, SqliteCompanyRepo};
        use crate::repos::custom_metric::SqliteCustomMetricRepo;
        use crate::repos::period::SqlitePeriodRepo;
        use chrono::{NaiveDate, TimeZone, Utc};

        let dir = tempfile::tempdir().unwrap();
        let pool = std::sync::Arc::new(Pool::open(&dir.path().join("test.sqlite")).unwrap());
        let cik = Cik("0000000002".into());
        SqliteCompanyRepo::new(pool.clone())
            .upsert(&Company {
                cik: cik.clone(),
                ticker: Ticker("TEST".into()),
                name: "Test Co".into(),
                exchange: None,
                sic: None,
                fiscal_year_end: Some("1231".into()),
                added_at: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
                last_refreshed: None,
            })
            .await
            .unwrap();
        let periods = SqlitePeriodRepo::new(pool.clone());
        let custom_metrics = SqliteCustomMetricRepo::new(pool.clone());
        let metric = custom_metrics
            .create(
                &cik,
                &NewCustomMetric {
                    key: "margin".into(),
                    label: "Margin".into(),
                    unit_kind: UnitKind::Ratio,
                    currency: None,
                    decimals: Some(2),
                    description: None,
                },
            )
            .await
            .unwrap();
        let q3 = Period {
            id: 0,
            cik: cik.clone(),
            fiscal_year: 2026,
            fiscal_quarter: 3,
            fiscal_year_end: "1231".into(),
            start_date: NaiveDate::from_ymd_opt(2026, 7, 1).unwrap(),
            end_date: NaiveDate::from_ymd_opt(2026, 9, 30).unwrap(),
            kind: PeriodKind::Quarterly,
            is_53_week: false,
        };
        let pid = periods.upsert_returning_id(&q3).await.unwrap();
        // Aligned point imported without an explicit date.
        let pt = custom_metrics
            .upsert_point(metric.id, Some(pid), None, 460_000, Some("csv import"))
            .await
            .unwrap();
        assert_eq!(pt.observed_date.as_deref(), Some("FY2026 Q3"));

        use crate::repos::period::PeriodRepo;
        periods.reset_normalized_for_reclassification(&cik).await.unwrap();
        assert_eq!(custom_metrics.points_unaligned(&cik).await.unwrap().len(), 1);

        let new_pid = periods.upsert_returning_id(&q3).await.unwrap();
        let n = reresolve_custom_points(&periods, &custom_metrics, &cik).await.unwrap();
        assert_eq!(n, 1, "label lets the point re-attach after a rebuild");
        let aligned = custom_metrics
            .points_aligned(metric.id, PeriodKind::Quarterly)
            .await
            .unwrap();
        assert_eq!(aligned.len(), 1);
        assert_eq!(aligned[0].0.id, new_pid);
    }
}
