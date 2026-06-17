//! Live LULU ingestion regression test for drift-FYE fiscal calendars.
//!
//! Lululemon is a 52/53-week retailer (FYE recorded as "0202") whose real
//! year-end drifts across the Jan/Feb month boundary year to year. Before
//! the drift-tolerant fiscal-calendar fix, `compute_fiscal_quarter`'s
//! exact-month match dropped every January-ending year from the annual and
//! instant paths — more than half of LULU's annual history — and
//! `compute_fiscal_year` rolled a year-end drifting past the nominal MMDD
//! into the next fiscal year (collision + silent drop).
//!
//! This test ingests LULU live and asserts:
//!   - the annual Net Income series spans many years with NO interior gaps
//!     (the January-ending years are present), and
//!   - the quarterly Net Income series is ordered Q1..Q4 within each year.
//!
//! Run via:
//!   cargo test --test integration_lulu_live -- --ignored --nocapture
//!
//! Real network: hits SEC EDGAR. Rate-limited at 5 req/s by SecClient.

use std::sync::Arc;

use econ_project_lib::db::Pool;
use econ_project_lib::domain::{Metric, PeriodKind, Ticker};
use econ_project_lib::pipeline::{ingest_company, IngestionDeps};
use econ_project_lib::repos::company::SqliteCompanyRepo;
use econ_project_lib::repos::filing::SqliteFilingRepo;
use econ_project_lib::repos::ingestion_event::SqliteIngestionEventRepo;
use econ_project_lib::repos::normalized_fact::SqliteNormalizedFactRepo;
use econ_project_lib::repos::period::SqlitePeriodRepo;
use econ_project_lib::repos::raw_fact::SqliteRawFactRepo;
use econ_project_lib::repos::historical_price::SqliteHistoricalPriceRepo;
use econ_project_lib::sources::market_data::YahooMarketData;
use econ_project_lib::sources::sec_client::SecClient;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn lulu_annual_series_has_no_gaps_and_quarters_are_ordered() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let pool = Arc::new(Pool::open(tmp.path()).unwrap());

    let sec = Arc::new(
        SecClient::new("EconProject-LULU-Live/0.1 contact@econproject.example", 5).unwrap(),
    );
    let deps = IngestionDeps {
        sec,
        market_data: Arc::new(YahooMarketData::new().unwrap()),
        companies: Arc::new(SqliteCompanyRepo::new(pool.clone())),
        filings: Arc::new(SqliteFilingRepo::new(pool.clone())),
        periods: Arc::new(SqlitePeriodRepo::new(pool.clone())),
        raw_facts: Arc::new(SqliteRawFactRepo::new(pool.clone())),
        normalized_facts: Arc::new(SqliteNormalizedFactRepo::new(pool.clone())),
        derived_metrics: Arc::new(
            econ_project_lib::repos::derived_metric::SqliteDerivedMetricRepo::new(pool.clone()),
        ),
        prices: Arc::new(SqliteHistoricalPriceRepo::new(pool.clone())),
        events: Arc::new(SqliteIngestionEventRepo::new(pool.clone())),
    };

    let ticker = Ticker("LULU".into());
    let (company, summary) = ingest_company(&deps, &ticker)
        .await
        .expect("LULU ingestion must not error");

    println!("\n=== LULU ingestion summary ===");
    println!("  cik:  {}", company.cik.0);
    println!("  fye:  {:?}", company.fiscal_year_end);
    println!("  filings: {}", summary.filings_ingested);
    println!("  normalized_facts: {}", summary.normalized_facts_ingested);

    assert_eq!(company.cik.0, "0001397187", "LULU CIK");

    // Annual Net Income series.
    let annual = deps
        .normalized_facts
        .current_series(&company.cik, Metric::NetIncome, PeriodKind::Annual)
        .await
        .unwrap();
    let years: Vec<i32> = annual.iter().map(|(p, _)| p.fiscal_year).collect();
    println!("\n  annual net-income fiscal years: {years:?}");

    assert!(
        years.len() >= 12,
        "expected a deep annual history (>=12 years), got {} — drift-FYE years are being dropped",
        years.len()
    );

    // No interior gaps: consecutive fiscal years must differ by exactly 1.
    // (This is what regressed — January-ending years were silently dropped,
    // leaving holes like 2015 -> 2020.)
    let mut sorted = years.clone();
    sorted.sort_unstable();
    sorted.dedup();
    let mut gaps = Vec::new();
    for w in sorted.windows(2) {
        if w[1] - w[0] != 1 {
            gaps.push((w[0], w[1]));
        }
    }
    assert!(
        gaps.is_empty(),
        "annual fiscal-year series has interior gaps {gaps:?} (full set: {sorted:?})"
    );

    // Series must be sorted ascending by period end (chart x-axis order).
    let ends: Vec<_> = annual.iter().map(|(p, _)| p.end_date).collect();
    let mut ends_sorted = ends.clone();
    ends_sorted.sort_unstable();
    assert_eq!(ends, ends_sorted, "annual series not ordered by end_date");

    // Quarterly Net Income: within each fiscal year, quarters must appear in
    // 1,2,3,4 order when read in series (end_date) order.
    let quarterly = deps
        .normalized_facts
        .current_series(&company.cik, Metric::NetIncome, PeriodKind::Quarterly)
        .await
        .unwrap();
    let q_ends: Vec<_> = quarterly.iter().map(|(p, _)| p.end_date).collect();
    let mut q_ends_sorted = q_ends.clone();
    q_ends_sorted.sort_unstable();
    assert_eq!(q_ends, q_ends_sorted, "quarterly series not ordered by end_date");

    // Spot-check FY2023 (the screenshotted year): quarters 1..4 present and
    // ordered by end_date.
    let mut fy2023: Vec<(u8, chrono::NaiveDate)> = quarterly
        .iter()
        .filter(|(p, _)| p.fiscal_year == 2023)
        .map(|(p, _)| (p.fiscal_quarter, p.end_date))
        .collect();
    println!("\n  FY2023 quarters (in series order): {fy2023:?}");
    fy2023.sort_by_key(|(_, end)| *end);
    let fq_order: Vec<u8> = fy2023.iter().map(|(fq, _)| *fq).collect();
    assert_eq!(
        fq_order,
        vec![1, 2, 3, 4],
        "FY2023 quarters not in 1-2-3-4 order by end_date"
    );
}
