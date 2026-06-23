//! Application state shared across Tauri commands.

use std::sync::Arc;

use tauri::{Manager, Runtime};

use crate::db::Pool;
use crate::pipeline::IngestionDeps;
use crate::repos::company::SqliteCompanyRepo;
use crate::repos::current_price::SqliteCurrentPriceRepo;
use crate::repos::derived_metric::SqliteDerivedMetricRepo;
use crate::repos::filing::SqliteFilingRepo;
use crate::repos::ingestion_event::SqliteIngestionEventRepo;
use crate::repos::historical_price::SqliteHistoricalPriceRepo;
use crate::repos::normalized_fact::SqliteNormalizedFactRepo;
use crate::repos::period::SqlitePeriodRepo;
use crate::repos::raw_fact::SqliteRawFactRepo;
use crate::sources::market_data::YahooMarketData;
use crate::sources::sec_client::SecClient;

pub struct AppState {
    pub pool: Arc<Pool>,
    pub companies: Arc<SqliteCompanyRepo>,
    pub filings: Arc<SqliteFilingRepo>,
    pub periods: Arc<SqlitePeriodRepo>,
    pub raw_facts: Arc<SqliteRawFactRepo>,
    pub normalized_facts: Arc<SqliteNormalizedFactRepo>,
    pub derived_metrics: Arc<SqliteDerivedMetricRepo>,
    pub prices: Arc<SqliteHistoricalPriceRepo>,
    pub current_prices: Arc<SqliteCurrentPriceRepo>,
    pub events: Arc<SqliteIngestionEventRepo>,
    pub sec: Arc<SecClient>,
    pub market_data: Arc<YahooMarketData>,
}

impl AppState {
    pub fn initialize<R: Runtime>(handle: &tauri::AppHandle<R>) -> Result<Self, Box<dyn std::error::Error>> {
        let path = data_dir(handle)?.join("data.sqlite");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let pool = Arc::new(Pool::open(&path)?);
        // SEC's Fair Access policy (https://www.sec.gov/os/accessing-edgar-data)
        // requires a User-Agent of the form "Identifier name@host.tld".
        // The email must be syntactically valid (have a TLD); SEC returns
        // 403 for UAs lacking a real email format. The `contact@local`
        // we used previously failed that check.
        let user_agent = format!(
            "EconProject/{} contact@econproject.example",
            env!("CARGO_PKG_VERSION"),
        );
        let sec = Arc::new(SecClient::new(user_agent, 5)?);
        let market_data = Arc::new(YahooMarketData::new()?);
        Ok(AppState {
            companies: Arc::new(SqliteCompanyRepo::new(pool.clone())),
            filings: Arc::new(SqliteFilingRepo::new(pool.clone())),
            periods: Arc::new(SqlitePeriodRepo::new(pool.clone())),
            raw_facts: Arc::new(SqliteRawFactRepo::new(pool.clone())),
            normalized_facts: Arc::new(SqliteNormalizedFactRepo::new(pool.clone())),
            derived_metrics: Arc::new(SqliteDerivedMetricRepo::new(pool.clone())),
            prices: Arc::new(SqliteHistoricalPriceRepo::new(pool.clone())),
            current_prices: Arc::new(SqliteCurrentPriceRepo::new(pool.clone())),
            events: Arc::new(SqliteIngestionEventRepo::new(pool.clone())),
            sec,
            market_data,
            pool,
        })
    }

    /// Borrowed repository bundle for read-time derived-metric series.
    pub fn read_ctx(&self) -> crate::derived::series::ReadCtx<'_> {
        crate::derived::series::ReadCtx {
            normalized_facts: self.normalized_facts.as_ref(),
            derived_metrics: self.derived_metrics.as_ref(),
            prices: self.prices.as_ref(),
            current_prices: self.current_prices.as_ref(),
        }
    }

    pub fn pipeline_deps(&self) -> IngestionDeps {
        IngestionDeps {
            sec: self.sec.clone(),
            market_data: self.market_data.clone(),
            companies: self.companies.clone(),
            filings: self.filings.clone(),
            periods: self.periods.clone(),
            raw_facts: self.raw_facts.clone(),
            normalized_facts: self.normalized_facts.clone(),
            derived_metrics: self.derived_metrics.clone(),
            prices: self.prices.clone(),
            current_prices: self.current_prices.clone(),
            events: self.events.clone(),
        }
    }
}

fn data_dir<R: Runtime>(handle: &tauri::AppHandle<R>) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("could not resolve app data dir: {e}").into())
}
