//! Read-time derived-metric series assembly — M28.
//!
//! These functions join per-period normalized inputs from the repositories
//! and apply the pure formulas in [`super`] to produce a metric series. They
//! are **read-time** rather than persisted: a stale input that has since been
//! superseded would otherwise leave a persisted sum out of sync with its
//! inputs (architecture §6.2). The same strategy is used for `total_debt`,
//! `gross_profit`, and the `capital_expenditures` PP&E roll-forward.
//!
//! The functions are parameterized over the [`NormalizedFactRepo`] and
//! [`DerivedMetricRepo`] traits (not the concrete `AppState`) so the IPC
//! handlers and the production-mode integration tests exercise the exact same
//! code path.

use std::collections::{BTreeMap, HashSet};

use serde::Serialize;

use crate::domain::{Cik, Metric, Period, PeriodKind};
use crate::errors::AppError;
use crate::repos::derived_metric::DerivedMetricRepo;
use crate::repos::normalized_fact::NormalizedFactRepo;

/// One point in a metric's time series, as returned over IPC. For monetary
/// metrics `value` is micro-units (USD × 1e6); for ratio metrics
/// (`operating_margin`) it is the decimal ratio × 1e6 (see architecture §6.2).
#[derive(Debug, Clone, Serialize)]
pub struct MetricSeriesPoint {
    pub period: Period,
    pub value: i64,
    pub source_kind: String,
    /// `-1` is a sentinel for derived values that have no single underlying
    /// `normalized_fact` row (the lineage drawer skips the single-fact walk).
    pub normalized_fact_id: i64,
}

/// `current_series` with read-time derivations layered on top. Other metrics
/// pass through unchanged. Result is sorted by `period.end_date` (the true
/// period close; monotonic for both annual and quarterly series even when a
/// derived quarter's `start_date` is the fiscal-year start).
///
/// Derivations:
/// - **Revenue**: fills missing periods from `bank_revenue_v1` derived rows.
/// - **TotalDebt**: `LongTermDebt + CurrentDebt` per period.
/// - **GrossProfit**: `Revenue − CostOfRevenue` when no direct value exists.
/// - **CapitalExpenditures**: PP&E roll-forward fallback when not directly tagged.
/// - **FreeCashFlow**: `NetIncome + DepreciationAmortization − CapitalExpenditures`.
/// - **OperatingMargin**: `OperatingIncome ÷ Revenue` (decimal ratio × 1e6).
pub async fn revenue_aware_series(
    nf: &dyn NormalizedFactRepo,
    dm: &dyn DerivedMetricRepo,
    cik: &Cik,
    metric: Metric,
    kind: PeriodKind,
) -> Result<Vec<MetricSeriesPoint>, AppError> {
    match metric {
        Metric::TotalDebt => return total_debt_series(nf, cik, kind).await,
        Metric::GrossProfit => return gross_profit_series(nf, dm, cik, kind).await,
        Metric::CapitalExpenditures => return capital_expenditures_series(nf, cik, kind).await,
        Metric::FreeCashFlow => return free_cash_flow_series(nf, dm, cik, kind).await,
        Metric::OperatingMargin => return operating_margin_series(nf, dm, cik, kind).await,
        _ => {}
    }

    let direct = nf.current_series(cik, metric, kind.clone()).await?;
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
        let derived = dm.series(cik, "bank_revenue_v1", kind).await?;
        for (period, d) in derived {
            if covered.contains(&period.id) {
                continue;
            }
            if let Some(value) = d.value {
                out.push(MetricSeriesPoint {
                    period,
                    value,
                    source_kind: "derived".into(),
                    // Sentinel: derived rows have no underlying single
                    // normalized_fact id. See followup.md for richer
                    // derivation lineage.
                    normalized_fact_id: -1,
                });
            }
        }
        out.sort_by(|a, b| a.period.end_date.cmp(&b.period.end_date));
    }
    Ok(out)
}

/// Gross profit is `Revenue − CostOfRevenue` per period, but only when no
/// directly-filed `GrossProfit` fact is present. Companies that file
/// GrossProfit directly keep their authoritative value; those that don't
/// (banks, some service-only filers) get the derived value when both Revenue
/// and CostOfRevenue are available.
async fn gross_profit_series(
    nf: &dyn NormalizedFactRepo,
    dm: &dyn DerivedMetricRepo,
    cik: &Cik,
    kind: PeriodKind,
) -> Result<Vec<MetricSeriesPoint>, AppError> {
    // Direct GrossProfit facts win when present.
    let direct = nf.current_series(cik, Metric::GrossProfit, kind.clone()).await?;
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

    // For periods without a direct fact: derive Revenue − CostOfRevenue.
    // Box::pin the recursive call so the future has a known size.
    let rev = Box::pin(revenue_aware_series(nf, dm, cik, Metric::Revenue, kind.clone())).await?;
    let cor = nf.current_series(cik, Metric::CostOfRevenue, kind).await?;

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
            out.push(MetricSeriesPoint {
                period,
                value: r.saturating_sub(c),
                source_kind: "derived".into(),
                normalized_fact_id: -1,
            });
        }
    }
    out.sort_by(|a, b| a.period.end_date.cmp(&b.period.end_date));
    Ok(out)
}

/// Total debt is `LongTermDebt + CurrentDebt`, per-period. We join the two
/// component series by `period.id`; if a period has at least one component, we
/// emit a row (treating the missing one as 0). If neither component is present,
/// no row is emitted.
async fn total_debt_series(
    nf: &dyn NormalizedFactRepo,
    cik: &Cik,
    kind: PeriodKind,
) -> Result<Vec<MetricSeriesPoint>, AppError> {
    let lt = nf.current_series(cik, Metric::LongTermDebt, kind.clone()).await?;
    let cd = nf.current_series(cik, Metric::CurrentDebt, kind).await?;

    // Key by period.id so we can sum components without losing the period record.
    let mut by_pid: BTreeMap<i64, (Period, Option<i64>, Option<i64>)> = BTreeMap::new();
    for (p, n) in lt {
        by_pid.entry(p.id).or_insert((p, None, None)).1 = Some(n.value);
    }
    for (p, n) in cd {
        by_pid.entry(p.id).or_insert((p, None, None)).2 = Some(n.value);
    }

    let mut out: Vec<MetricSeriesPoint> = by_pid
        .into_values()
        .map(|(period, lt_v, cd_v)| MetricSeriesPoint {
            period,
            value: lt_v.unwrap_or(0).saturating_add(cd_v.unwrap_or(0)),
            source_kind: "derived".into(),
            normalized_fact_id: -1,
        })
        .collect();
    out.sort_by(|a, b| a.period.end_date.cmp(&b.period.end_date));
    Ok(out)
}

/// Capital expenditures, with read-time derivation for periods where the filer
/// didn't report `PaymentsToAcquirePropertyPlantAndEquipment`.
///
/// Formula:
///   `CapEx(t) ≈ PP&E_Net(end of t) − PP&E_Net(end of prior period) + DepreciationAndAmortization(t)`
///
/// This is the standard reconstruction from the balance roll-forward identity
///   `PP&E_Net(end) = PP&E_Net(begin) + Additions − Depreciation − Disposals`
/// solving for Additions and ignoring disposals (unobservable from these
/// inputs). For filers with significant asset disposals the derived value is
/// an upper bound on capital expenditures.
///
/// Behavior:
/// - Periods with a directly-filed value pass through unchanged.
/// - Periods missing a direct value are derived if all three inputs are
///   available: PP&E_Net at the period's end_date, PP&E_Net at the most recent
///   earlier observation, and DepreciationAndAmortization for the period.
/// - Non-positive derivations (more disposals than purchases — implausible for
///   a positive-only metric) are skipped per the project accuracy rule.
async fn capital_expenditures_series(
    nf: &dyn NormalizedFactRepo,
    cik: &Cik,
    kind: PeriodKind,
) -> Result<Vec<MetricSeriesPoint>, AppError> {
    // Direct values win.
    let direct = nf.current_series(cik, Metric::CapitalExpenditures, kind.clone()).await?;
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

    // PP&E net is an instant. Q4 / FY observations attach to the annual period;
    // Q1–Q3 attach to quarterly periods. To find the right "prior" balance for
    // any period, gather every observation across both kinds and key by end_date.
    let ppne_a = nf
        .current_series(cik, Metric::PropertyPlantAndEquipmentNet, PeriodKind::Annual)
        .await?;
    let ppne_q = nf
        .current_series(cik, Metric::PropertyPlantAndEquipmentNet, PeriodKind::Quarterly)
        .await?;
    let mut ppne_by_end: BTreeMap<chrono::NaiveDate, i64> = BTreeMap::new();
    for (p, n) in ppne_a.iter().chain(ppne_q.iter()) {
        ppne_by_end.insert(p.end_date, n.value);
    }

    // DepreciationAndAmortization for the requested kind drives candidate
    // periods: we can only derive a period that has a depreciation value and a
    // closing PP&E observation matching its end_date.
    let da = nf.current_series(cik, Metric::DepreciationAmortization, kind).await?;

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
        // Capital expenditures is positive-only; skip implausible results.
        if value <= 0 {
            continue;
        }
        out.push(MetricSeriesPoint {
            period,
            value,
            source_kind: "derived".into(),
            normalized_fact_id: -1,
        });
    }
    out.sort_by(|a, b| a.period.end_date.cmp(&b.period.end_date));
    Ok(out)
}

/// Free cash flow `= NetIncome + DepreciationAmortization − CapitalExpenditures`
/// per period (architecture §6.2 / PRD FR-032). Capital expenditures uses the
/// full read-time series, so the PP&E roll-forward fallback flows through for
/// filers that don't tag cash-flow CapEx directly.
///
/// All three inputs are required for a period: free cash flow has three terms,
/// and substituting a missing term with zero would silently misstate it, so an
/// incomplete period is omitted rather than reported inaccurately.
async fn free_cash_flow_series(
    nf: &dyn NormalizedFactRepo,
    dm: &dyn DerivedMetricRepo,
    cik: &Cik,
    kind: PeriodKind,
) -> Result<Vec<MetricSeriesPoint>, AppError> {
    let net_income = nf.current_series(cik, Metric::NetIncome, kind.clone()).await?;
    let dep_amort = nf
        .current_series(cik, Metric::DepreciationAmortization, kind.clone())
        .await?;
    // CapEx via the read-time series so the bank/PP&E fallback applies.
    let capex =
        Box::pin(revenue_aware_series(nf, dm, cik, Metric::CapitalExpenditures, kind)).await?;

    // (period, net_income?, dep_amort?, capex?)
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
            (Some(ni), Some(da), Some(cx)) => Some(MetricSeriesPoint {
                period,
                value: super::free_cash_flow(ni, da, cx),
                source_kind: "derived".into(),
                normalized_fact_id: -1,
            }),
            _ => None,
        })
        .collect();
    out.sort_by(|a, b| a.period.end_date.cmp(&b.period.end_date));
    Ok(out)
}

/// Operating margin `= OperatingIncome ÷ Revenue`, per period, returned as a
/// decimal ratio × 1e6 (architecture §6.2). Revenue uses the read-time series
/// so the bank-revenue fallback applies. Both inputs are required; periods with
/// non-positive revenue are omitted (margin undefined — see
/// [`super::operating_margin_micro`]).
async fn operating_margin_series(
    nf: &dyn NormalizedFactRepo,
    dm: &dyn DerivedMetricRepo,
    cik: &Cik,
    kind: PeriodKind,
) -> Result<Vec<MetricSeriesPoint>, AppError> {
    let op_inc = nf.current_series(cik, Metric::OperatingIncome, kind.clone()).await?;
    let revenue = Box::pin(revenue_aware_series(nf, dm, cik, Metric::Revenue, kind)).await?;

    // (period, operating_income?, revenue?)
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
            (Some(oi), Some(rev)) => super::operating_margin_micro(oi, rev).map(|value| {
                MetricSeriesPoint {
                    period,
                    value,
                    source_kind: "derived".into(),
                    normalized_fact_id: -1,
                }
            }),
            _ => None,
        })
        .collect();
    out.sort_by(|a, b| a.period.end_date.cmp(&b.period.end_date));
    Ok(out)
}
