//! One-shot re-ingest of saved companies against the production SQLite.
//!
//! Run with: cargo run --bin refresh_companies -- PYPL WEN POOL
//! Mirrors the `refresh_company` IPC command without needing the app running.

use std::sync::Arc;

use econ_project_lib::db::Pool;
use econ_project_lib::domain::Ticker;
use econ_project_lib::import::reviewed_metrics;
use econ_project_lib::pipeline::{ingest_company, reresolve_custom_points, IngestionDeps};
use econ_project_lib::repos::company::SqliteCompanyRepo;
use econ_project_lib::repos::current_price::SqliteCurrentPriceRepo;
use econ_project_lib::repos::custom_metric::SqliteCustomMetricRepo;
use econ_project_lib::repos::derived_metric::SqliteDerivedMetricRepo;
use econ_project_lib::repos::filing::SqliteFilingRepo;
use econ_project_lib::repos::historical_price::SqliteHistoricalPriceRepo;
use econ_project_lib::repos::ingestion_event::SqliteIngestionEventRepo;
use econ_project_lib::repos::normalized_fact::SqliteNormalizedFactRepo;
use econ_project_lib::repos::period::SqlitePeriodRepo;
use econ_project_lib::repos::raw_fact::SqliteRawFactRepo;
use econ_project_lib::sources::market_data::YahooMarketData;
use econ_project_lib::sources::sec_client::SecClient;

#[tokio::main]
async fn main() {
    let db_path = dirs::data_dir()
        .expect("no data dir")
        .join("com.econproject.app")
        .join("data.sqlite");
    assert!(db_path.exists(), "Production DB not found at {}", db_path.display());
    let pool = Arc::new(Pool::open(&db_path).expect("open DB"));
    let user_agent = format!("EconProject/{} contact@econproject.example", env!("CARGO_PKG_VERSION"));
    let periods = Arc::new(SqlitePeriodRepo::new(pool.clone()));
    let custom_metrics = Arc::new(SqliteCustomMetricRepo::new(pool.clone()));
    let deps = IngestionDeps {
        sec: Arc::new(SecClient::new(user_agent, 5).expect("sec client")),
        market_data: Arc::new(YahooMarketData::new().expect("market data")),
        companies: Arc::new(SqliteCompanyRepo::new(pool.clone())),
        filings: Arc::new(SqliteFilingRepo::new(pool.clone())),
        periods: periods.clone(),
        raw_facts: Arc::new(SqliteRawFactRepo::new(pool.clone())),
        normalized_facts: Arc::new(SqliteNormalizedFactRepo::new(pool.clone())),
        derived_metrics: Arc::new(SqliteDerivedMetricRepo::new(pool.clone())),
        prices: Arc::new(SqliteHistoricalPriceRepo::new(pool.clone())),
        current_prices: Arc::new(SqliteCurrentPriceRepo::new(pool.clone())),
        events: Arc::new(SqliteIngestionEventRepo::new(pool.clone())),
    };
    for t in std::env::args().skip(1) {
        let ticker = Ticker::from_str(&t);
        match ingest_company(&deps, &ticker).await {
            Ok((company, summary)) => {
                reviewed_metrics::sync(&company.cik, periods.as_ref(), custom_metrics.as_ref())
                    .await
                    .expect("reviewed metrics sync");
                reresolve_custom_points(periods.as_ref(), custom_metrics.as_ref(), &company.cik)
                    .await
                    .expect("reresolve custom points");
                println!("{t}: ok, {} normalized facts", summary.normalized_facts_ingested);
            }
            Err(e) => println!("{t}: FAILED {e:?}"),
        }
    }
}
