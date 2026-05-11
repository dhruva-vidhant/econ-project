//! Tauri commands. These are the V1 IPC surface.

use serde::Serialize;
use tauri::State;

use crate::domain::{Cik, Company, IngestionEvent, Metric, NormalizedFact, Period, PeriodKind, Ticker};
use crate::errors::AppError;
use crate::pipeline::{ingest_company, IngestionSummary};
use crate::repos::company::CompanyRepo;
use crate::repos::ingestion_event::IngestionEventRepo;
use crate::repos::normalized_fact::NormalizedFactRepo;

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
    let series = state.normalized_facts.current_series(&cik, metric, kind).await?;
    Ok(series
        .into_iter()
        .map(|(p, n)| MetricSeriesPoint {
            period: p,
            value: n.value,
            source_kind: n.source_kind.as_str().into(),
            normalized_fact_id: n.id,
        })
        .collect())
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
        let series = state
            .normalized_facts
            .current_series(&cik, *metric, PeriodKind::Annual)
            .await?;
        if series.is_empty() { continue; }
        let history: Vec<(String, i64)> = series
            .iter()
            .map(|(p, n)| (format!("FY{}", p.fiscal_year), n.value))
            .collect();
        let last = series.last().unwrap();
        widgets.push(DashboardWidget {
            metric: metric.as_str().into(),
            period_label: format!("FY{}", last.0.fiscal_year),
            value_micro: last.1.value,
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
