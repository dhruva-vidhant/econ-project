//! Tauri commands. These are the V1 IPC surface.

use tauri::State;

use crate::domain::{Cik, Company, Ticker};
use crate::errors::AppError;
use crate::repos::company::CompanyRepo;

use super::state::AppState;

#[tauri::command]
pub async fn list_companies(state: State<'_, AppState>) -> Result<Vec<Company>, AppError> {
    let companies = state.companies.clone();
    companies.list_saved().await.map_err(Into::into)
}

#[tauri::command]
pub async fn add_company(
    _state: State<'_, AppState>,
    ticker: String,
) -> Result<Company, AppError> {
    let _t = Ticker::from_str(&ticker);
    // V1-stub: a real ingestion runs the pipeline here. For now we just
    // refuse with a stable, user-actionable error so the UI flow can be
    // exercised end-to-end against a deterministic surface.
    Err(AppError::Ingestion {
        code: "ingestion_not_implemented",
        message: format!(
            "Ingestion for {ticker} is not yet wired in this build. \
             The Discover/Download/Parse/Normalize/Persist pipeline lands \
             in the next implementation pass."
        ),
    })
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
