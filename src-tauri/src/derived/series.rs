//! Read-time derived-metric series assembly — M28.
//!
//! These functions join per-period normalized inputs from the repositories
//! and apply the pure formulas in [`super`] to produce a metric series. They
//! are **read-time** rather than persisted: a stale input that has since been
//! superseded would otherwise leave a persisted sum out of sync with its
//! inputs (architecture §6.2). The same strategy is used for `total_debt`,
//! `gross_profit`, the `capital_expenditures` PP&E roll-forward, free cash
//! flow, operating margin, market cap, and free-cash-flow yield.
//!
//! The functions take a [`ReadCtx`] of repository trait objects (not the
//! concrete `AppState`) so the IPC handlers and the production-mode
//! integration tests exercise the exact same code path.

use std::collections::{BTreeMap, HashSet};

use serde::Serialize;

use crate::domain::{Cik, Metric, Period, PeriodKind};
use crate::errors::AppError;
use crate::repos::current_price::CurrentPriceRepo;
use crate::repos::derived_metric::DerivedMetricRepo;
use crate::repos::historical_price::HistoricalPriceRepo;
use crate::repos::normalized_fact::NormalizedFactRepo;

/// Repositories the read-time derivations draw on. Borrowed trait objects so
/// callers pass `&SqliteX` (which coerces) and tests can substitute fakes.
pub struct ReadCtx<'a> {
    pub normalized_facts: &'a dyn NormalizedFactRepo,
    pub derived_metrics: &'a dyn DerivedMetricRepo,
    pub prices: &'a dyn HistoricalPriceRepo,
    pub current_prices: &'a dyn CurrentPriceRepo,
}

/// One point in a metric's time series, as returned over IPC. For monetary
/// metrics `value` is micro-units (USD × 1e6); for ratio metrics
/// (`operating_margin`, `free_cash_flow_yield`) it is the decimal ratio × 1e6
/// (see architecture §6.2).
#[derive(Debug, Clone, Serialize)]
pub struct MetricSeriesPoint {
    pub period: Period,
    pub value: i64,
    pub source_kind: String,
    /// `-1` is a sentinel for derived values that have no single underlying
    /// `normalized_fact` row (the lineage drawer skips the single-fact walk).
    pub normalized_fact_id: i64,
}

fn derived_point(period: Period, value: i64) -> MetricSeriesPoint {
    MetricSeriesPoint { period, value, source_kind: "derived".into(), normalized_fact_id: -1 }
}

/// `current_series` with read-time derivations layered on top. Other metrics
/// pass through unchanged. Result is sorted by `period.end_date` (the true
/// period close; monotonic for both annual and quarterly series even when a
/// derived quarter's `start_date` is the fiscal-year start).
///
/// Derivations: Revenue (bank fallback), TotalDebt, GrossProfit,
/// CapitalExpenditures, FreeCashFlow, OperatingMargin, FreeCashFlowTtm,
/// HistoricalMarketCap, FreeCashFlowYield.
pub async fn revenue_aware_series(
    ctx: &ReadCtx<'_>,
    cik: &Cik,
    metric: Metric,
    kind: PeriodKind,
) -> Result<Vec<MetricSeriesPoint>, AppError> {
    match metric {
        // Live scalar metrics served by current_valuation, not series.
        Metric::CurrentMarketCap | Metric::CurrentFreeCashFlowYield => return Ok(Vec::new()),
        Metric::TotalDebt => return total_debt_series(ctx, cik, kind).await,
        Metric::GrossProfit => return gross_profit_series(ctx, cik, kind).await,
        Metric::CapitalExpenditures => return capital_expenditures_series(ctx, cik, kind).await,
        Metric::FreeCashFlow => return free_cash_flow_series(ctx, cik, kind).await,
        Metric::OperatingMargin => return operating_margin_series(ctx, cik, kind).await,
        Metric::FreeCashFlowTtm => return free_cash_flow_ttm_series(ctx, cik, kind).await,
        Metric::HistoricalMarketCap => return historical_market_cap_series(ctx, cik, kind).await,
        Metric::FreeCashFlowYield => return free_cash_flow_yield_series(ctx, cik, kind).await,
        _ => {}
    }

    let direct = ctx.normalized_facts.current_series(cik, metric, kind.clone()).await?;
    let mut out: Vec<MetricSeriesPoint> = direct
        .into_iter()
        .map(|(p, n)| MetricSeriesPoint {
            period: p,
            value: n.value,
            source_kind: n.source_kind.as_str().into(),
            normalized_fact_id: n.id,
        })
        .collect();

    if matches!(metric, Metric::Revenue) {
        let covered: HashSet<i64> = out.iter().map(|p| p.period.id).collect();
        let derived = ctx.derived_metrics.series(cik, "bank_revenue_v1", kind).await?;
        for (period, d) in derived {
            if covered.contains(&period.id) {
                continue;
            }
            if let Some(value) = d.value {
                // Sentinel id: derived rows have no single normalized_fact.
                out.push(derived_point(period, value));
            }
        }
        out.sort_by(|a, b| a.period.end_date.cmp(&b.period.end_date));
    }
    Ok(out)
}

/// Gross profit is `Revenue − CostOfRevenue` per period, but only when no
/// directly-filed `GrossProfit` fact is present.
async fn gross_profit_series(
    ctx: &ReadCtx<'_>,
    cik: &Cik,
    kind: PeriodKind,
) -> Result<Vec<MetricSeriesPoint>, AppError> {
    let direct = ctx.normalized_facts.current_series(cik, Metric::GrossProfit, kind.clone()).await?;
    let mut out: Vec<MetricSeriesPoint> = direct
        .into_iter()
        .map(|(p, n)| MetricSeriesPoint {
            period: p,
            value: n.value,
            source_kind: n.source_kind.as_str().into(),
            normalized_fact_id: n.id,
        })
        .collect();
    let direct_periods: HashSet<i64> = out.iter().map(|p| p.period.id).collect();

    // Box::pin the recursive call so the future has a known size.
    let rev = Box::pin(revenue_aware_series(ctx, cik, Metric::Revenue, kind.clone())).await?;
    let cor = ctx.normalized_facts.current_series(cik, Metric::CostOfRevenue, kind).await?;

    let mut by_pid: BTreeMap<i64, (Period, Option<i64>, Option<i64>)> = BTreeMap::new();
    for p in rev {
        if direct_periods.contains(&p.period.id) {
            continue;
        }
        by_pid.entry(p.period.id).or_insert((p.period.clone(), None, None)).1 = Some(p.value);
    }
    for (p, n) in cor {
        if direct_periods.contains(&p.id) {
            continue;
        }
        by_pid.entry(p.id).or_insert((p, None, None)).2 = Some(n.value);
    }

    for (_, (period, rev_v, cor_v)) in by_pid {
        if let (Some(r), Some(c)) = (rev_v, cor_v) {
            out.push(derived_point(period, r.saturating_sub(c)));
        }
    }
    out.sort_by(|a, b| a.period.end_date.cmp(&b.period.end_date));
    Ok(out)
}

/// Total debt is `LongTermDebt + CurrentDebt`, per-period. A period with at
/// least one component emits a row (missing component treated as 0); a period
/// with neither emits nothing.
async fn total_debt_series(
    ctx: &ReadCtx<'_>,
    cik: &Cik,
    kind: PeriodKind,
) -> Result<Vec<MetricSeriesPoint>, AppError> {
    let lt = ctx.normalized_facts.current_series(cik, Metric::LongTermDebt, kind.clone()).await?;
    let cd = ctx.normalized_facts.current_series(cik, Metric::CurrentDebt, kind).await?;

    let mut by_pid: BTreeMap<i64, (Period, Option<i64>, Option<i64>)> = BTreeMap::new();
    for (p, n) in lt {
        by_pid.entry(p.id).or_insert((p, None, None)).1 = Some(n.value);
    }
    for (p, n) in cd {
        by_pid.entry(p.id).or_insert((p, None, None)).2 = Some(n.value);
    }

    let mut out: Vec<MetricSeriesPoint> = by_pid
        .into_values()
        .map(|(period, lt_v, cd_v)| {
            derived_point(period, lt_v.unwrap_or(0).saturating_add(cd_v.unwrap_or(0)))
        })
        .collect();
    out.sort_by(|a, b| a.period.end_date.cmp(&b.period.end_date));
    Ok(out)
}

/// Capital expenditures, with read-time derivation for periods where the filer
/// didn't report `PaymentsToAcquirePropertyPlantAndEquipment`.
///
/// `CapEx(t) ≈ PP&E_Net(end t) − PP&E_Net(prior) + DepreciationAndAmortization(t)`,
/// the balance roll-forward solved for additions (disposals unobservable, so
/// the derived value is an upper bound). Direct values pass through; non-positive
/// derivations are skipped (positive-only metric).
async fn capital_expenditures_series(
    ctx: &ReadCtx<'_>,
    cik: &Cik,
    kind: PeriodKind,
) -> Result<Vec<MetricSeriesPoint>, AppError> {
    let direct = ctx.normalized_facts.current_series(cik, Metric::CapitalExpenditures, kind.clone()).await?;
    let mut out: Vec<MetricSeriesPoint> = direct
        .into_iter()
        .map(|(p, n)| MetricSeriesPoint {
            period: p,
            value: n.value,
            source_kind: n.source_kind.as_str().into(),
            normalized_fact_id: n.id,
        })
        .collect();
    let direct_periods: HashSet<i64> = out.iter().map(|p| p.period.id).collect();

    // PP&E net is an instant; gather across both kinds keyed by end_date so any
    // period can find its closing and most-recent-prior balance.
    let ppne_a = ctx
        .normalized_facts
        .current_series(cik, Metric::PropertyPlantAndEquipmentNet, PeriodKind::Annual)
        .await?;
    let ppne_q = ctx
        .normalized_facts
        .current_series(cik, Metric::PropertyPlantAndEquipmentNet, PeriodKind::Quarterly)
        .await?;
    let mut ppne_by_end: BTreeMap<chrono::NaiveDate, i64> = BTreeMap::new();
    for (p, n) in ppne_a.iter().chain(ppne_q.iter()) {
        ppne_by_end.insert(p.end_date, n.value);
    }

    let da = ctx.normalized_facts.current_series(cik, Metric::DepreciationAmortization, kind).await?;

    for (period, da_n) in da {
        if direct_periods.contains(&period.id) {
            continue;
        }
        let Some(&ppne_end) = ppne_by_end.get(&period.end_date) else {
            continue;
        };
        let Some((_, &ppne_start)) = ppne_by_end.range(..period.end_date).next_back() else {
            continue;
        };
        let value = ppne_end.saturating_sub(ppne_start).saturating_add(da_n.value);
        if value <= 0 {
            continue;
        }
        out.push(derived_point(period, value));
    }
    out.sort_by(|a, b| a.period.end_date.cmp(&b.period.end_date));
    Ok(out)
}

/// Free cash flow `= NetIncome + DepreciationAmortization − CapitalExpenditures`
/// per period (PRD FR-032). Capital expenditures uses the full read-time series
/// (PP&E fallback flows through). All three inputs are required for a period.
async fn free_cash_flow_series(
    ctx: &ReadCtx<'_>,
    cik: &Cik,
    kind: PeriodKind,
) -> Result<Vec<MetricSeriesPoint>, AppError> {
    let net_income = ctx.normalized_facts.current_series(cik, Metric::NetIncome, kind.clone()).await?;
    let dep_amort = ctx
        .normalized_facts
        .current_series(cik, Metric::DepreciationAmortization, kind.clone())
        .await?;
    let capex = Box::pin(revenue_aware_series(ctx, cik, Metric::CapitalExpenditures, kind)).await?;

    let mut by_pid: BTreeMap<i64, (Period, Option<i64>, Option<i64>, Option<i64>)> = BTreeMap::new();
    for (p, n) in net_income {
        by_pid.entry(p.id).or_insert((p, None, None, None)).1 = Some(n.value);
    }
    for (p, n) in dep_amort {
        by_pid.entry(p.id).or_insert((p, None, None, None)).2 = Some(n.value);
    }
    for point in capex {
        by_pid
            .entry(point.period.id)
            .or_insert((point.period.clone(), None, None, None))
            .3 = Some(point.value);
    }

    let mut out: Vec<MetricSeriesPoint> = by_pid
        .into_values()
        .filter_map(|(period, ni, da, cx)| match (ni, da, cx) {
            (Some(ni), Some(da), Some(cx)) => {
                Some(derived_point(period, super::free_cash_flow(ni, da, cx)))
            }
            _ => None,
        })
        .collect();
    out.sort_by(|a, b| a.period.end_date.cmp(&b.period.end_date));
    Ok(out)
}

/// Operating margin `= OperatingIncome ÷ Revenue` (decimal ratio × 1e6).
/// Revenue uses the read-time series (bank fallback). Both inputs required;
/// non-positive revenue is omitted (margin undefined).
async fn operating_margin_series(
    ctx: &ReadCtx<'_>,
    cik: &Cik,
    kind: PeriodKind,
) -> Result<Vec<MetricSeriesPoint>, AppError> {
    let op_inc = ctx.normalized_facts.current_series(cik, Metric::OperatingIncome, kind.clone()).await?;
    let revenue = Box::pin(revenue_aware_series(ctx, cik, Metric::Revenue, kind)).await?;

    let mut by_pid: BTreeMap<i64, (Period, Option<i64>, Option<i64>)> = BTreeMap::new();
    for (p, n) in op_inc {
        by_pid.entry(p.id).or_insert((p, None, None)).1 = Some(n.value);
    }
    for point in revenue {
        by_pid
            .entry(point.period.id)
            .or_insert((point.period.clone(), None, None))
            .2 = Some(point.value);
    }

    let mut out: Vec<MetricSeriesPoint> = by_pid
        .into_values()
        .filter_map(|(period, oi, rev)| match (oi, rev) {
            (Some(oi), Some(rev)) => {
                super::operating_margin_micro(oi, rev).map(|v| derived_point(period, v))
            }
            _ => None,
        })
        .collect();
    out.sort_by(|a, b| a.period.end_date.cmp(&b.period.end_date));
    Ok(out)
}

/// A monotonic quarter index for continuity checks: `fiscal_year*4 + (q-1)`.
fn quarter_index(p: &Period) -> i64 {
    p.fiscal_year as i64 * 4 + (p.fiscal_quarter as i64 - 1)
}

/// Trailing-twelve-month free cash flow.
///
/// - **Annual**: a fiscal year already spans twelve months, so this is the
///   plain annual `free_cash_flow` series (relabeled).
/// - **Quarterly**: the sum of each quarter and its three predecessors, emitted
///   at the most recent quarter. A window is emitted only when the four
///   quarters are strictly consecutive (no gap and no fiscal-calendar break),
///   so an incomplete trailing year is omitted rather than understated.
async fn free_cash_flow_ttm_series(
    ctx: &ReadCtx<'_>,
    cik: &Cik,
    kind: PeriodKind,
) -> Result<Vec<MetricSeriesPoint>, AppError> {
    let fcf = Box::pin(revenue_aware_series(ctx, cik, Metric::FreeCashFlow, kind.clone())).await?;
    if matches!(kind, PeriodKind::Annual) {
        // Already a 12-month figure; just re-tag the points.
        return Ok(fcf
            .into_iter()
            .map(|p| derived_point(p.period, p.value))
            .collect());
    }

    // Quarterly: rolling 4-quarter sum over the end_date-sorted series.
    let mut out = Vec::new();
    for i in 3..fcf.len() {
        let window = &fcf[i - 3..=i];
        // Strictly consecutive fiscal quarters: newest index − oldest == 3.
        if quarter_index(&window[3].period) - quarter_index(&window[0].period) != 3 {
            continue;
        }
        let sum: i64 = window.iter().fold(0i64, |acc, p| acc.saturating_add(p.value));
        out.push(derived_point(window[3].period.clone(), sum));
    }
    out.sort_by(|a, b| a.period.end_date.cmp(&b.period.end_date));
    Ok(out)
}

/// Historical market cap `= close_price(period end) × shares_outstanding_basic`,
/// a per-period instant. The close comes from `historical_price` keyed by the
/// period's `end_date` (resolved to the nearest prior trading day at ingest);
/// shares come from the period's current basic-shares fact. Both required.
async fn historical_market_cap_series(
    ctx: &ReadCtx<'_>,
    cik: &Cik,
    kind: PeriodKind,
) -> Result<Vec<MetricSeriesPoint>, AppError> {
    let shares = ctx
        .normalized_facts
        .current_series(cik, Metric::SharesOutstandingBasic, kind)
        .await?;
    let prices = ctx.prices.map_for(cik).await?;

    let mut out: Vec<MetricSeriesPoint> = shares
        .into_iter()
        .filter_map(|(period, n)| {
            prices
                .get(&period.end_date)
                .map(|&close| derived_point(period, super::market_cap(close, n.value)))
        })
        .collect();
    out.sort_by(|a, b| a.period.end_date.cmp(&b.period.end_date));
    Ok(out)
}

/// Free cash flow yield `= FCF ÷ market cap` (decimal ratio × 1e6).
/// Annual uses annual FCF; quarterly uses trailing-twelve-month FCF. Both the
/// numerator and the market cap are required; a non-positive market cap is
/// omitted (yield undefined).
async fn free_cash_flow_yield_series(
    ctx: &ReadCtx<'_>,
    cik: &Cik,
    kind: PeriodKind,
) -> Result<Vec<MetricSeriesPoint>, AppError> {
    // Numerator: annual FCF, or TTM FCF for quarterly.
    let numerator = match kind {
        PeriodKind::Annual => {
            Box::pin(revenue_aware_series(ctx, cik, Metric::FreeCashFlow, kind.clone())).await?
        }
        PeriodKind::Quarterly => {
            Box::pin(free_cash_flow_ttm_series(ctx, cik, kind.clone())).await?
        }
    };
    let mcap = Box::pin(historical_market_cap_series(ctx, cik, kind)).await?;
    let mcap_by_pid: BTreeMap<i64, i64> =
        mcap.into_iter().map(|p| (p.period.id, p.value)).collect();

    let mut out: Vec<MetricSeriesPoint> = numerator
        .into_iter()
        .filter_map(|p| {
            let mc = mcap_by_pid.get(&p.period.id)?;
            super::fcf_yield_micro(p.value, *mc).map(|v| derived_point(p.period, v))
        })
        .collect();
    out.sort_by(|a, b| a.period.end_date.cmp(&b.period.end_date));
    Ok(out)
}

/// A helper for retrieving the most recent value from a series across both
/// quarterly AND annual periods, useful for live metrics that need the absolute
/// latest reported value (shares, FCF) regardless of which series it came from.
async fn current_series<'a>(
    ctx: &ReadCtx<'a>,
    cik: &Cik,
    metric: Metric,
) -> Result<Vec<MetricSeriesPoint>, AppError> {
    let mut annual = ctx
        .normalized_facts
        .current_series(cik, metric, PeriodKind::Annual)
        .await?
        .into_iter()
        .map(|(p, n)| MetricSeriesPoint {
            period: p,
            value: n.value,
            source_kind: n.source_kind.as_str().into(),
            normalized_fact_id: n.id,
        })
        .collect::<Vec<_>>();
    let mut quarterly = ctx
        .normalized_facts
        .current_series(cik, metric, PeriodKind::Quarterly)
        .await?
        .into_iter()
        .map(|(p, n)| MetricSeriesPoint {
            period: p,
            value: n.value,
            source_kind: n.source_kind.as_str().into(),
            normalized_fact_id: n.id,
        })
        .collect::<Vec<_>>();
    annual.append(&mut quarterly);
    annual.sort_by(|a, b| a.period.end_date.cmp(&b.period.end_date));
    Ok(annual)
}

/// Current valuation: live spot price, latest shares, market cap, TTM FCF, and
/// current FCF yield. Used for the `current_market_cap` and
/// `current_free_cash_flow_yield` metrics. Returns `None` if any required input
/// is missing or if derived market cap is non-positive.
#[derive(Debug, Clone, Serialize)]
pub struct CurrentValuation {
    pub price_micro: i64,
    pub price_as_of: chrono::DateTime<chrono::Utc>,
    pub shares: i64,
    pub shares_period_end: chrono::NaiveDate,
    pub market_cap_micro: i64,
    pub ttm_fcf_micro: i64,
    pub ttm_fcf_period_end: chrono::NaiveDate,
    pub fcf_yield_micro: i64,
}

pub async fn current_valuation(
    ctx: &ReadCtx<'_>,
    cik: &Cik,
) -> Result<Option<CurrentValuation>, AppError> {
    // 1. Fetch stored spot price.
    let Some(price) = ctx.current_prices.get(cik).await? else {
        return Ok(None);
    };

    // 2. Latest shares (across annual and quarterly, by max end_date).
    let shares_series = current_series(ctx, cik, Metric::SharesOutstandingBasic).await?;
    let Some(shares_point) = shares_series.last() else {
        return Ok(None);
    };

    // 3. Current market cap.
    let market_cap = super::market_cap(price.price_micro, shares_point.value);
    if market_cap <= 0 {
        return Ok(None);
    }

    // 4. TTM free cash flow: prefer latest quarterly TTM, fall back to latest annual FCF.
    let ttm_q = Box::pin(free_cash_flow_ttm_series(ctx, cik, PeriodKind::Quarterly)).await?;
    let fcf_point = if let Some(p) = ttm_q.last() {
        p.clone()
    } else {
        let fcf_a = Box::pin(revenue_aware_series(ctx, cik, Metric::FreeCashFlow, PeriodKind::Annual)).await?;
        match fcf_a.last() {
            Some(p) => p.clone(),
            None => return Ok(None),
        }
    };

    // 5. FCF yield.
    let Some(fcf_yield) = super::fcf_yield_micro(fcf_point.value, market_cap) else {
        return Ok(None);
    };

    Ok(Some(CurrentValuation {
        price_micro: price.price_micro,
        price_as_of: price.as_of,
        shares: shares_point.value,
        shares_period_end: shares_point.period.end_date,
        market_cap_micro: market_cap,
        ttm_fcf_micro: fcf_point.value,
        ttm_fcf_period_end: fcf_point.period.end_date,
        fcf_yield_micro: fcf_yield,
    }))
}
