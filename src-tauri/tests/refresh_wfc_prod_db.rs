//! One-off: refresh saved companies against the live production SQLite so
//! concept-map / pipeline fixes take effect without the user clicking
//! Refresh in the UI for each one. Marked `#[ignore]`. Run via:
//!
//!     cargo test --test refresh_wfc_prod_db -- --ignored --nocapture
//!
//! Tickers can be overridden by setting REFRESH_TICKERS to a comma-
//! separated list (default: every saved company in the DB). The
//! per-company stale state — normalized_fact, period, derived_metric
//! rows — is wiped before re-ingest so freshly-applied concept-map
//! entries and period-derivation rules can take effect on rows that
//! were originally normalized under older logic.
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
use econ_project_lib::repos::historical_price::SqliteHistoricalPriceRepo;
use econ_project_lib::repos::current_price::SqliteCurrentPriceRepo;
use econ_project_lib::sources::market_data::YahooMarketData;
use econ_project_lib::sources::sec_client::SecClient;

#[tokio::test]
#[ignore]
async fn refresh_saved_companies_in_production_db() {
    let home = std::env::var("HOME").expect("HOME not set");
    let path: PathBuf = format!("{home}/Library/Application Support/com.econproject.app/data.sqlite").into();
    println!("opening production SQLite at: {}", path.display());
    let pool = Arc::new(Pool::open(&path).unwrap());

    let tickers: Vec<String> = match std::env::var("REFRESH_TICKERS") {
        Ok(s) => s.split(',').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect(),
        Err(_) => {
            // No override: refresh everything saved.
            let g = pool.read().unwrap();
            let mut stmt = g.conn().prepare("SELECT ticker FROM company ORDER BY ticker").unwrap();
            stmt.query_map([], |r| r.get::<_, String>(0))
                .unwrap()
                .map(|r| r.unwrap())
                .collect()
        }
    };
    println!("refreshing tickers: {tickers:?}");

    let sec = Arc::new(
        SecClient::new(
            "EconProject-RefreshTool/0.1 contact@econproject.example",
            5,
        )
        .unwrap(),
    );
    let deps = IngestionDeps {
        sec,
        market_data: Arc::new(YahooMarketData::new().unwrap()),
        companies: Arc::new(SqliteCompanyRepo::new(pool.clone())),
        filings: Arc::new(SqliteFilingRepo::new(pool.clone())),
        periods: Arc::new(SqlitePeriodRepo::new(pool.clone())),
        raw_facts: Arc::new(SqliteRawFactRepo::new(pool.clone())),
        normalized_facts: Arc::new(SqliteNormalizedFactRepo::new(pool.clone())),
        derived_metrics: Arc::new(SqliteDerivedMetricRepo::new(pool.clone())),
        prices: Arc::new(SqliteHistoricalPriceRepo::new(pool.clone())),
        current_prices: Arc::new(SqliteCurrentPriceRepo::new(pool.clone())),
        events: Arc::new(SqliteIngestionEventRepo::new(pool.clone())),
    };

    for t in &tickers {
        // Find the cik so we can wipe stale normalized state. The
        // upstream raw_fact / filing rows stay — they're the immutable
        // source of truth and re-ingest is idempotent on them. We do
        // need to drop the derived layer though, otherwise rows that
        // moved to a different (fy, fq) under the new period rules
        // would leave orphans behind under the old (fy, fq).
        let cik: Option<String> = {
            let g = pool.read().unwrap();
            g.conn()
                .query_row(
                    "SELECT cik FROM company WHERE ticker = ?1",
                    rusqlite::params![t],
                    |r| r.get(0),
                )
                .ok()
        };
        if let Some(cik) = &cik {
            let mut g = pool.write().await;
            let conn = g.conn();
            conn.execute("DELETE FROM derived_metric WHERE cik = ?1", rusqlite::params![cik]).unwrap();
            conn.execute("DELETE FROM normalized_fact WHERE cik = ?1", rusqlite::params![cik]).unwrap();
            conn.execute("DELETE FROM period WHERE cik = ?1", rusqlite::params![cik]).unwrap();
        }
        let (company, summary) = ingest_company(&deps, &Ticker::from_str(t)).await.unwrap();
        println!(
            "{} ingested: cik={} {} filings, {} raw, {} normalized",
            t, company.cik.0, summary.filings_ingested, summary.raw_facts_ingested, summary.normalized_facts_ingested
        );
    }
}
