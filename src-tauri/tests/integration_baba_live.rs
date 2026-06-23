//! Live BABA ingestion regression test.
//!
//! BABA is a foreign private issuer (files 20-F instead of 10-K). SEC
//! submissions returns `entityName: null` and `fiscalYearEnd: null` for
//! foreign filers — pre-fix, this caused `error decoding response body`
//! on add-ticker and the dashboard showed the network-error toast.
//!
//! Run via:
//!   cargo test --test integration_baba_live -- --ignored --nocapture
//!
//! Real network: hits SEC EDGAR. Rate-limited at 5 req/s by SecClient.

use std::sync::Arc;

use econ_project_lib::db::Pool;
use econ_project_lib::domain::{Metric, Ticker};
use econ_project_lib::pipeline::{ingest_company, IngestionDeps};
use econ_project_lib::repos::company::SqliteCompanyRepo;
use econ_project_lib::repos::filing::SqliteFilingRepo;
use econ_project_lib::repos::ingestion_event::SqliteIngestionEventRepo;
use econ_project_lib::repos::normalized_fact::SqliteNormalizedFactRepo;
use econ_project_lib::repos::period::SqlitePeriodRepo;
use econ_project_lib::repos::raw_fact::SqliteRawFactRepo;
use econ_project_lib::repos::historical_price::SqliteHistoricalPriceRepo;
use econ_project_lib::repos::current_price::SqliteCurrentPriceRepo;
use econ_project_lib::sources::market_data::YahooMarketData;
use econ_project_lib::sources::sec_client::SecClient;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn baba_ingests_without_error() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let pool = Arc::new(Pool::open(tmp.path()).unwrap());

    let sec = Arc::new(
        SecClient::new("EconProject-BABA-Live/0.1 contact@econproject.example", 5).unwrap(),
    );
    let deps = IngestionDeps {
        sec,
        market_data: Arc::new(YahooMarketData::new().unwrap()),
        companies: Arc::new(SqliteCompanyRepo::new(pool.clone())),
        filings: Arc::new(SqliteFilingRepo::new(pool.clone())),
        periods: Arc::new(SqlitePeriodRepo::new(pool.clone())),
        raw_facts: Arc::new(SqliteRawFactRepo::new(pool.clone())),
        normalized_facts: Arc::new(SqliteNormalizedFactRepo::new(pool.clone())),
        derived_metrics: Arc::new(econ_project_lib::repos::derived_metric::SqliteDerivedMetricRepo::new(pool.clone())),
        prices: Arc::new(SqliteHistoricalPriceRepo::new(pool.clone())),
        current_prices: Arc::new(SqliteCurrentPriceRepo::new(pool.clone())),
        events: Arc::new(SqliteIngestionEventRepo::new(pool.clone())),
    };

    let ticker = Ticker("BABA".into());
    let (company, summary) = ingest_company(&deps, &ticker)
        .await
        .expect("BABA ingestion must not error");

    println!("\n=== BABA ingestion summary ===");
    println!("  cik: {}", company.cik.0);
    println!("  name: {}", company.name);
    println!("  fye: {:?}", company.fiscal_year_end);
    println!("  filings: {}", summary.filings_ingested);
    println!("  raw_facts: {}", summary.raw_facts_ingested);
    println!("  normalized_facts: {}", summary.normalized_facts_ingested);

    assert_eq!(company.cik.0, "0001577552", "BABA CIK");
    assert_eq!(
        company.fiscal_year_end.as_deref(),
        Some("0331"),
        "BABA FYE must derive to March 31"
    );
    assert!(
        summary.filings_ingested > 50,
        "BABA should have many filings (20-F + 6-K + ...)"
    );
    assert!(
        summary.normalized_facts_ingested > 100,
        "BABA should produce hundreds of normalized facts"
    );

    let periods = deps.periods.list_for_cik(&company.cik, None).await.unwrap();
    assert!(!periods.is_empty(), "BABA must have at least one Period row");

    let mut found_revenue = false;
    let mut found_net_income = false;
    for p in &periods {
        if deps.normalized_facts.current_value(&company.cik, Metric::Revenue, p.id).await.unwrap().is_some() {
            found_revenue = true;
        }
        if deps.normalized_facts.current_value(&company.cik, Metric::NetIncome, p.id).await.unwrap().is_some() {
            found_net_income = true;
        }
        if found_revenue && found_net_income { break; }
    }
    assert!(found_revenue, "BABA should produce at least one Revenue normalized fact");
    assert!(found_net_income, "BABA should produce at least one NetIncome normalized fact");
}
