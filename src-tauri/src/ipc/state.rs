//! Application state shared across Tauri commands.

use std::sync::Arc;

use tauri::{Manager, Runtime};

use crate::db::Pool;
use crate::repos::company::SqliteCompanyRepo;
use crate::repos::derived_metric::SqliteDerivedMetricRepo;
use crate::repos::filing::SqliteFilingRepo;
use crate::repos::ingestion_event::SqliteIngestionEventRepo;
use crate::repos::normalized_fact::SqliteNormalizedFactRepo;
use crate::repos::period::SqlitePeriodRepo;
use crate::repos::raw_fact::SqliteRawFactRepo;

pub struct AppState {
    pub pool: Arc<Pool>,
    pub companies: Arc<SqliteCompanyRepo>,
    pub filings: Arc<SqliteFilingRepo>,
    pub periods: Arc<SqlitePeriodRepo>,
    pub raw_facts: Arc<SqliteRawFactRepo>,
    pub normalized_facts: Arc<SqliteNormalizedFactRepo>,
    pub derived_metrics: Arc<SqliteDerivedMetricRepo>,
    pub events: Arc<SqliteIngestionEventRepo>,
}

impl AppState {
    pub fn initialize<R: Runtime>(handle: &tauri::AppHandle<R>) -> Result<Self, Box<dyn std::error::Error>> {
        let path = data_dir(handle)?.join("data.sqlite");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let pool = Arc::new(Pool::open(&path)?);
        Ok(AppState {
            companies: Arc::new(SqliteCompanyRepo::new(pool.clone())),
            filings: Arc::new(SqliteFilingRepo::new(pool.clone())),
            periods: Arc::new(SqlitePeriodRepo::new(pool.clone())),
            raw_facts: Arc::new(SqliteRawFactRepo::new(pool.clone())),
            normalized_facts: Arc::new(SqliteNormalizedFactRepo::new(pool.clone())),
            derived_metrics: Arc::new(SqliteDerivedMetricRepo::new(pool.clone())),
            events: Arc::new(SqliteIngestionEventRepo::new(pool.clone())),
            pool,
        })
    }
}

fn data_dir<R: Runtime>(handle: &tauri::AppHandle<R>) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("could not resolve app data dir: {e}").into())
}
