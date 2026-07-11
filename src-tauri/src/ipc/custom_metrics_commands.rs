//! Custom-metric IPC commands. See techspec §7.1.

use tauri::State;

use crate::derived::custom_series;
use crate::domain::{
    parse_micro, Cik, CustomMetric, CustomMetricPatch, CustomMetricPoint, CustomSeriesPoint,
    NewCustomMetric, PeriodKind,
};
use crate::errors::AppError;
use crate::export::csv as export_csv_mod;
use crate::export::csv::{ExportReport, ExportRequest};
use crate::import::csv::{import_csv, ImportOptions, ImportReport};
use crate::repos::custom_metric::CustomMetricRepo;
use crate::repos::period::PeriodRepo;

use super::state::AppState;

fn parse_kind(kind: &str) -> Result<PeriodKind, AppError> {
    PeriodKind::from_str(kind).ok_or_else(|| AppError::invalid(format!("unknown period kind: {kind}")))
}

#[tauri::command]
pub async fn list_custom_metrics(
    state: State<'_, AppState>,
    cik: String,
) -> Result<Vec<CustomMetric>, AppError> {
    let cik = Cik::from_any(&cik).map_err(AppError::invalid)?;
    state.custom_metrics.list(&cik).await.map_err(Into::into)
}

#[tauri::command]
pub async fn create_custom_metric(
    state: State<'_, AppState>,
    cik: String,
    def: NewCustomMetric,
) -> Result<CustomMetric, AppError> {
    let cik = Cik::from_any(&cik).map_err(AppError::invalid)?;
    state.custom_metrics.create(&cik, &def).await.map_err(Into::into)
}

#[tauri::command]
pub async fn update_custom_metric(
    state: State<'_, AppState>,
    id: i64,
    patch: CustomMetricPatch,
) -> Result<CustomMetric, AppError> {
    state.custom_metrics.update(id, &patch).await.map_err(Into::into)
}

#[tauri::command]
pub async fn delete_custom_metric(state: State<'_, AppState>, id: i64) -> Result<(), AppError> {
    state.custom_metrics.delete(id).await.map_err(Into::into)
}

#[tauri::command]
pub async fn count_custom_points(state: State<'_, AppState>, id: i64) -> Result<i64, AppError> {
    state.custom_metrics.count_points(id).await.map_err(Into::into)
}

#[tauri::command]
pub async fn get_custom_metric_series(
    state: State<'_, AppState>,
    cik: String,
    metric_id: i64,
    kind: String,
) -> Result<Vec<CustomSeriesPoint>, AppError> {
    let _cik = Cik::from_any(&cik).map_err(AppError::invalid)?;
    let kind = parse_kind(&kind)?;
    custom_series::custom_metric_series(state.custom_metrics.as_ref(), metric_id, kind).await
}

#[tauri::command]
pub async fn upsert_custom_metric_point(
    state: State<'_, AppState>,
    metric_id: i64,
    period: String,
    kind: String,
    value: String,
) -> Result<CustomMetricPoint, AppError> {
    let metric = state
        .custom_metrics
        .get(metric_id)
        .await?
        .ok_or_else(|| AppError::not_found("custom metric"))?;
    let granularity = parse_kind(&kind)?;
    let periods = state.periods.list_for_cik(&metric.cik, None).await?;
    let value_micro = parse_micro(&value).map_err(|e| AppError::invalid(e.to_string()))?;

    use crate::import::resolve::{resolve_period, Resolution};
    match resolve_period(&period, granularity, &periods) {
        Resolution::Matched(period_id) => state
            .custom_metrics
            .upsert_point(metric_id, Some(period_id), None, value_micro, Some("manual"))
            .await
            .map_err(Into::into),
        Resolution::Unaligned => state
            .custom_metrics
            .upsert_point(metric_id, None, Some(&period), value_micro, Some("manual"))
            .await
            .map_err(Into::into),
        Resolution::Error(e) => Err(AppError::invalid(e.0)),
    }
}

#[tauri::command]
pub async fn delete_custom_metric_point(
    state: State<'_, AppState>,
    point_id: i64,
) -> Result<(), AppError> {
    state.custom_metrics.delete_point(point_id).await.map_err(Into::into)
}

#[tauri::command]
pub async fn import_custom_metrics_csv(
    state: State<'_, AppState>,
    cik: String,
    csv_text: String,
    options: ImportOptions,
) -> Result<ImportReport, AppError> {
    let cik = Cik::from_any(&cik).map_err(AppError::invalid)?;
    let periods = state.periods.list_for_cik(&cik, None).await?;
    import_csv(&cik, &csv_text, &options, state.custom_metrics.as_ref(), &periods)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn render_csv(
    state: State<'_, AppState>,
    request: ExportRequest,
) -> Result<String, AppError> {
    let ctx = state.read_ctx();
    export_csv_mod::render_csv(&request, &ctx, state.custom_metrics.as_ref()).await
}

#[tauri::command]
pub async fn export_csv(
    state: State<'_, AppState>,
    request: ExportRequest,
    dest_path: String,
) -> Result<ExportReport, AppError> {
    let ctx = state.read_ctx();
    export_csv_mod::export_csv(request, &dest_path, &ctx, state.custom_metrics.as_ref()).await
}
