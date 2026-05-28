//! Tauri commands. These are the V1 IPC surface.

use serde::Serialize;
use tauri::State;

use std::collections::HashSet;

use crate::domain::{Cik, Company, IngestionEvent, Metric, NormalizedFact, Period, PeriodKind, Ticker};
use crate::errors::AppError;
use crate::pipeline::{ingest_company, IngestionSummary};
use crate::repos::company::CompanyRepo;
use crate::repos::derived_metric::DerivedMetricRepo;
use crate::repos::filing::FilingRepo;
use crate::repos::ingestion_event::IngestionEventRepo;
use crate::repos::normalized_fact::NormalizedFactRepo;
use crate::repos::raw_fact::RawFactRepo;

use super::state::AppState;

#[tauri::command]
pub async fn list_companies(state: State<'_, AppState>) -> Result<Vec<Company>, AppError> {
    state.companies.list_saved().await.map_err(Into::into)
}

#[derive(Debug, Clone, Serialize)]
pub struct AddCompanyResponse {
    pub company: Company,
    pub summary: IngestionSummary,
}

#[tauri::command]
pub async fn add_company(
    state: State<'_, AppState>,
    ticker: String,
) -> Result<AddCompanyResponse, AppError> {
    let t = Ticker::from_str(&ticker);
    if t.0.is_empty() {
        return Err(AppError::invalid("ticker cannot be empty"));
    }
    let deps = state.pipeline_deps();
    let (company, summary) = ingest_company(&deps, &t).await?;
    Ok(AddCompanyResponse { company, summary })
}

#[tauri::command]
pub async fn remove_company(
    state: State<'_, AppState>,
    cik: String,
    drop_cache: bool,
) -> Result<(), AppError> {
    let cik = Cik::from_any(&cik).map_err(AppError::invalid)?;
    state.companies.remove(&cik, drop_cache).await.map_err(Into::into)
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricSeriesPoint {
    pub period: Period,
    pub value: i64,
    pub source_kind: String,
    pub normalized_fact_id: i64,
}

#[tauri::command]
pub async fn get_metric_history(
    state: State<'_, AppState>,
    cik: String,
    metric: String,
    kind: String,
) -> Result<Vec<MetricSeriesPoint>, AppError> {
    let cik = Cik::from_any(&cik).map_err(AppError::invalid)?;
    let metric = Metric::from_str(&metric)
        .ok_or_else(|| AppError::invalid(format!("unknown metric: {metric}")))?;
    let kind = PeriodKind::from_str(&kind)
        .ok_or_else(|| AppError::invalid(format!("unknown period kind: {kind}")))?;
    revenue_aware_series(state.inner(), &cik, metric, kind).await
}

/// Like `current_series` but with read-time derivations layered on top:
///
/// - **Revenue**: fills missing periods from `derived_metric` rows with
///   `formula_id = "bank_revenue_v1"` (see pipeline::orchestrator).
/// - **TotalDebt**: computed as `LongTermDebt + CurrentDebt` per period.
///   Missing input treated as 0; row omitted only if both inputs missing.
/// - **GrossProfit**: when no directly-filed value exists, computed as
///   `Revenue - CostOfRevenue` per period. Both inputs required.
///
/// Other metrics pass through unchanged. Result is sorted by
/// `period.start_date`.
async fn revenue_aware_series(
    state: &AppState,
    cik: &Cik,
    metric: Metric,
    kind: PeriodKind,
) -> Result<Vec<MetricSeriesPoint>, AppError> {
    if matches!(metric, Metric::TotalDebt) {
        return total_debt_series(state, cik, kind).await;
    }
    if matches!(metric, Metric::GrossProfit) {
        return gross_profit_series(state, cik, kind).await;
    }

    let direct = state.normalized_facts.current_series(cik, metric, kind.clone()).await?;
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
        let derived = state.derived_metrics.series(cik, "bank_revenue_v1", kind).await?;
        for (period, dm) in derived {
            if covered.contains(&period.id) { continue; }
            if let Some(value) = dm.value {
                out.push(MetricSeriesPoint {
                    period,
                    value,
                    source_kind: "derived".into(),
                    // Sentinel: derived rows have no underlying single
                    // normalized_fact id. The lineage drawer treats this
                    // value as a derivation and skips the single-fact
                    // walk; see followup.md for richer derivation lineage.
                    normalized_fact_id: -1,
                });
            }
        }
        out.sort_by(|a, b| a.period.start_date.cmp(&b.period.start_date));
    }
    Ok(out)
}

/// Gross profit is `Revenue - CostOfRevenue` per period, but only when
/// no directly-filed `GrossProfit` fact is present. Companies that file
/// GrossProfit directly (e.g., most retailers, manufacturers) keep their
/// authoritative value; companies that don't (banks, some service-only
/// filers) get the derived value when both Revenue and CostOfRevenue
/// are available.
async fn gross_profit_series(
    state: &AppState,
    cik: &Cik,
    kind: PeriodKind,
) -> Result<Vec<MetricSeriesPoint>, AppError> {
    use std::collections::{BTreeMap, HashSet};

    // Direct GrossProfit facts win when present.
    let direct = state
        .normalized_facts
        .current_series(cik, Metric::GrossProfit, kind.clone())
        .await?;
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

    // For periods without a direct fact: derive Revenue - CostOfRevenue.
    // Box the recursive call (via Box::pin) so the future has a known size.
    let rev = Box::pin(revenue_aware_series(state, cik, Metric::Revenue, kind.clone())).await?;
    let cor = state
        .normalized_facts
        .current_series(cik, Metric::CostOfRevenue, kind)
        .await?;

    let mut by_pid: BTreeMap<i64, (Period, Option<i64>, Option<i64>)> = BTreeMap::new();
    for p in rev {
        if direct_periods.contains(&p.period.id) { continue; }
        by_pid.entry(p.period.id).or_insert((p.period.clone(), None, None)).1 = Some(p.value);
    }
    for (p, n) in cor {
        if direct_periods.contains(&p.id) { continue; }
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
    out.sort_by(|a, b| a.period.start_date.cmp(&b.period.start_date));
    Ok(out)
}

/// Total debt is `LongTermDebt + CurrentDebt`, per-period. We join the
/// two component series by `period.id`; if a period has at least one
/// component, we emit a row (treating the missing one as 0). If neither
/// component is present, no row is emitted.
async fn total_debt_series(
    state: &AppState,
    cik: &Cik,
    kind: PeriodKind,
) -> Result<Vec<MetricSeriesPoint>, AppError> {
    use std::collections::BTreeMap;

    let lt = state
        .normalized_facts
        .current_series(cik, Metric::LongTermDebt, kind.clone())
        .await?;
    let cd = state
        .normalized_facts
        .current_series(cik, Metric::CurrentDebt, kind)
        .await?;

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
    out.sort_by(|a, b| a.period.start_date.cmp(&b.period.start_date));
    Ok(out)
}

#[derive(Debug, Clone, Serialize)]
pub struct DashboardWidget {
    pub metric: String,
    pub period_label: String,
    pub value_micro: i64,
    pub history: Vec<(String, i64)>, // (period label, value_micro)
}

#[derive(Debug, Clone, Serialize)]
pub struct DashboardPayload {
    pub company: Company,
    pub widgets: Vec<DashboardWidget>,
}

#[tauri::command]
pub async fn get_dashboard(
    state: State<'_, AppState>,
    cik: String,
) -> Result<DashboardPayload, AppError> {
    let cik = Cik::from_any(&cik).map_err(AppError::invalid)?;
    let company = state
        .companies
        .get_by_cik(&cik)
        .await?
        .ok_or_else(|| AppError::not_found(format!("company {cik} not found")))?;
    let mut widgets = Vec::new();
    for metric in &[
        Metric::Revenue,
        Metric::NetIncome,
        Metric::CashAndEquivalents,
        Metric::TotalAssets,
        Metric::TotalLiabilities,
    ] {
        let series = revenue_aware_series(state.inner(), &cik, *metric, PeriodKind::Annual).await?;
        if series.is_empty() { continue; }
        let history: Vec<(String, i64)> = series
            .iter()
            .map(|p| (format!("FY{}", p.period.fiscal_year), p.value))
            .collect();
        let last = series.last().unwrap();
        widgets.push(DashboardWidget {
            metric: metric.as_str().into(),
            period_label: format!("FY{}", last.period.fiscal_year),
            value_micro: last.value,
            history,
        });
    }
    Ok(DashboardPayload { company, widgets })
}

#[tauri::command]
pub async fn get_ingestion_events(
    state: State<'_, AppState>,
    cik: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<IngestionEvent>, AppError> {
    let cik_opt = match cik {
        Some(s) if !s.is_empty() => Some(Cik::from_any(&s).map_err(AppError::invalid)?),
        _ => None,
    };
    let lim = limit.unwrap_or(200);
    state.events.recent(cik_opt.as_ref(), lim).await.map_err(Into::into)
}

#[tauri::command]
pub async fn get_supersession_chain(
    state: State<'_, AppState>,
    normalized_fact_id: i64,
) -> Result<Vec<NormalizedFact>, AppError> {
    state
        .normalized_facts
        .supersession_chain(normalized_fact_id)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn refresh_company(
    state: State<'_, AppState>,
    cik: String,
) -> Result<AddCompanyResponse, AppError> {
    let cik = Cik::from_any(&cik).map_err(AppError::invalid)?;
    let company = state
        .companies
        .get_by_cik(&cik)
        .await?
        .ok_or_else(|| AppError::not_found(format!("company {cik} not found")))?;
    let deps = state.pipeline_deps();
    let (company2, summary) = ingest_company(&deps, &company.ticker).await?;
    Ok(AddCompanyResponse { company: company2, summary })
}

#[derive(Debug, Clone, Serialize)]
pub struct LineagePayload {
    pub primary: NormalizedFact,
    pub raw_fact: crate::domain::RawFact,
    pub filing: crate::domain::Filing,
    pub supersession_chain: Vec<NormalizedFact>,
}

#[tauri::command]
pub async fn get_lineage(
    state: State<'_, AppState>,
    normalized_fact_id: i64,
) -> Result<LineagePayload, AppError> {
    // Fetch the normalized_fact directly via a small inline query.
    let g = state.pool.read().map_err(|e| AppError::Storage {
        code: "storage", message: e.to_string(),
    })?;
    let nf: NormalizedFact = g.conn().query_row(
        "SELECT id, cik, metric, period_id, value, unit, source_fact_id, source_kind, is_primary,
                original_value, original_unit, fx_rate_micro, fx_rate_source, fx_rate_date, superseded_by
         FROM normalized_fact WHERE id = ?1",
        rusqlite::params![normalized_fact_id],
        |r| {
            Ok(NormalizedFact {
                id: r.get(0)?,
                cik: Cik(r.get(1)?),
                metric: Metric::from_str(&r.get::<_, String>(2)?).unwrap_or(Metric::Revenue),
                period_id: r.get(3)?,
                value: r.get(4)?,
                unit: r.get(5)?,
                source_fact_id: r.get(6)?,
                source_kind: crate::domain::SourceKind::from_str(&r.get::<_, String>(7)?).unwrap_or(crate::domain::SourceKind::XbrlApi),
                is_primary: r.get::<_, i64>(8)? != 0,
                original_value: r.get(9)?,
                original_unit: r.get(10)?,
                fx_rate_micro: r.get(11)?,
                fx_rate_source: r.get(12)?,
                fx_rate_date: r.get(13)?,
                superseded_by: r.get(14)?,
            })
        },
    ).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => AppError::not_found(format!("normalized_fact {normalized_fact_id} not found")),
        other => AppError::Storage { code: "storage", message: other.to_string() },
    })?;
    drop(g);
    let raw = state.raw_facts.get(nf.source_fact_id).await?
        .ok_or_else(|| AppError::not_found("source raw_fact missing"))?;
    let filing = state.filings.get(&raw.accession_no).await?
        .ok_or_else(|| AppError::not_found("source filing missing"))?;
    let chain = state.normalized_facts.supersession_chain(nf.id).await?;
    Ok(LineagePayload { primary: nf, raw_fact: raw, filing, supersession_chain: chain })
}
