//! End-to-end integration test that ingests Apple's actual SEC filings.
//! Marked `#[ignore]` so it doesn't run in default `cargo test` (it hits
//! the real SEC EDGAR API). Run via:
//!
//!     cargo test --test integration_apple -- --ignored --nocapture
//!
//! This is the simplest possible "does the V1 pipeline actually work"
//! check.

use std::sync::Arc;

use econ_project_lib::db::Pool;
use econ_project_lib::domain::Ticker;
use econ_project_lib::pipeline::{ingest_company, IngestionDeps};
use econ_project_lib::repos::company::SqliteCompanyRepo;
use econ_project_lib::repos::filing::SqliteFilingRepo;
use econ_project_lib::repos::ingestion_event::SqliteIngestionEventRepo;
use econ_project_lib::repos::normalized_fact::SqliteNormalizedFactRepo;
use econ_project_lib::repos::period::SqlitePeriodRepo;
use econ_project_lib::repos::raw_fact::SqliteRawFactRepo;
use econ_project_lib::sources::sec_client::SecClient;

#[tokio::test]
#[ignore]
async fn ingest_aapl_against_real_sec() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.sqlite");
    let pool = Arc::new(Pool::open(&path).unwrap());

    let sec = Arc::new(
        SecClient::new("EconProject-IntegrationTest/0.1 contact@local", 5).unwrap(),
    );
    let deps = IngestionDeps {
        sec,
        companies: Arc::new(SqliteCompanyRepo::new(pool.clone())),
        filings: Arc::new(SqliteFilingRepo::new(pool.clone())),
        periods: Arc::new(SqlitePeriodRepo::new(pool.clone())),
        raw_facts: Arc::new(SqliteRawFactRepo::new(pool.clone())),
        normalized_facts: Arc::new(SqliteNormalizedFactRepo::new(pool.clone())),
        events: Arc::new(SqliteIngestionEventRepo::new(pool.clone())),
    };

    let (company, summary) = ingest_company(&deps, &Ticker("AAPL".into())).await
        .expect("ingestion failed");
    assert_eq!(company.ticker.0, "AAPL");
    assert!(summary.filings_ingested > 0, "no filings ingested");
    assert!(summary.raw_facts_ingested > 0, "no raw facts ingested");
    assert!(summary.normalized_facts_ingested > 0, "no normalized facts produced");
    println!("Ingestion summary: {summary:#?}");
}
