//! One-off: refresh WFC against the live production SQLite so newly-added
//! concept-map entries take effect without the user clicking Refresh in
//! the UI. Marked `#[ignore]`. Run via:
//!
//!     cargo test --test refresh_wfc_prod_db -- --ignored --nocapture
//!
//! Safe to run while the dev binary is open: SQLite WAL mode allows
//! multiple processes to coexist; only one writer at a time, but
//! ingest_company is the only writer.

use std::path::PathBuf;
use std::sync::Arc;

use econ_project_lib::db::Pool;
use econ_project_lib::domain::Ticker;
use econ_project_lib::pipeline::{ingest_company, IngestionDeps};
use econ_project_lib::repos::company::SqliteCompanyRepo;
use econ_project_lib::repos::derived_metric::SqliteDerivedMetricRepo;
use econ_project_lib::repos::filing::SqliteFilingRepo;
use econ_project_lib::repos::ingestion_event::SqliteIngestionEventRepo;
use econ_project_lib::repos::normalized_fact::SqliteNormalizedFactRepo;
use econ_project_lib::repos::period::SqlitePeriodRepo;
use econ_project_lib::repos::raw_fact::SqliteRawFactRepo;
use econ_project_lib::sources::sec_client::SecClient;

#[tokio::test]
#[ignore]
async fn refresh_wfc_in_production_db() {
    let home = std::env::var("HOME").expect("HOME not set");
    let path: PathBuf = format!("{home}/Library/Application Support/com.econproject.app/data.sqlite").into();
    println!("opening production SQLite at: {}", path.display());
    let pool = Arc::new(Pool::open(&path).unwrap());

    let sec = Arc::new(
        SecClient::new(
            "EconProject-RefreshTool/0.1 contact@econproject.example",
            5,
        )
        .unwrap(),
    );
    let deps = IngestionDeps {
        sec,
        companies: Arc::new(SqliteCompanyRepo::new(pool.clone())),
        filings: Arc::new(SqliteFilingRepo::new(pool.clone())),
        periods: Arc::new(SqlitePeriodRepo::new(pool.clone())),
        raw_facts: Arc::new(SqliteRawFactRepo::new(pool.clone())),
        normalized_facts: Arc::new(SqliteNormalizedFactRepo::new(pool.clone())),
        derived_metrics: Arc::new(SqliteDerivedMetricRepo::new(pool.clone())),
        events: Arc::new(SqliteIngestionEventRepo::new(pool.clone())),
    };

    let (company, summary) = ingest_company(&deps, &Ticker::from_str("WFC")).await.unwrap();
    println!(
        "WFC ingested: cik={} {} filings, {} raw, {} normalized",
        company.cik.0, summary.filings_ingested, summary.raw_facts_ingested, summary.normalized_facts_ingested
    );
}
