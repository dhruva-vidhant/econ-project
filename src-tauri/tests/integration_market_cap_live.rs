//! Live end-to-end test for the market-cap / TTM / free-cash-flow-yield
//! subsystem. Ingests a company against the **real** SEC EDGAR + Yahoo Finance
//! APIs, then exercises the read-time series the IPC handlers use.
//! Marked `#[ignore]`; requires network. Run via:
//!
//!     cargo test --release --test integration_market_cap_live -- --ignored --nocapture
//!
//! Validates:
//!   1. Prices were fetched and persisted (one per period end-date).
//!   2. Market cap = close × shares, in a sane range for a known large-cap.
//!   3. FCF yield = FCF ÷ market cap, internally consistent and plausible.
//!   4. Trailing-twelve-month FCF at a fiscal-year-end quarter reconciles with
//!      that fiscal year's annual FCF (sum of four quarters = the year).

use std::sync::Arc;

use econ_project_lib::db::Pool;
use econ_project_lib::derived::{self, series};
use econ_project_lib::derived::series::ReadCtx;
use econ_project_lib::domain::{Metric, PeriodKind, Ticker};
use econ_project_lib::pipeline::{ingest_company, IngestionDeps};
use econ_project_lib::repos::company::SqliteCompanyRepo;
use econ_project_lib::repos::current_price::SqliteCurrentPriceRepo;
use econ_project_lib::repos::derived_metric::SqliteDerivedMetricRepo;
use econ_project_lib::repos::filing::SqliteFilingRepo;
use econ_project_lib::repos::historical_price::{HistoricalPriceRepo, SqliteHistoricalPriceRepo};
use econ_project_lib::repos::ingestion_event::SqliteIngestionEventRepo;
use econ_project_lib::repos::normalized_fact::SqliteNormalizedFactRepo;
use econ_project_lib::repos::period::SqlitePeriodRepo;
use econ_project_lib::repos::raw_fact::SqliteRawFactRepo;
use econ_project_lib::sources::market_data::YahooMarketData;
use econ_project_lib::sources::sec_client::SecClient;

const TRILLION_MICRO: i64 = 1_000_000_000_000 * 1_000_000; // $1T in micro-units

#[tokio::test]
#[ignore]
async fn market_cap_and_fcf_yield_live() {
    let dir = tempfile::tempdir().unwrap();
    let pool = Arc::new(Pool::open(&dir.path().join("t.sqlite")).unwrap());

    let sec = Arc::new(
        SecClient::new("EconProject-MarketCapTest/0.1 contact@econproject.example", 5).unwrap(),
    );
    let market_data = Arc::new(YahooMarketData::new().unwrap());
    let companies = Arc::new(SqliteCompanyRepo::new(pool.clone()));
    let filings = Arc::new(SqliteFilingRepo::new(pool.clone()));
    let periods = Arc::new(SqlitePeriodRepo::new(pool.clone()));
    let raw_facts = Arc::new(SqliteRawFactRepo::new(pool.clone()));
    let normalized_facts = Arc::new(SqliteNormalizedFactRepo::new(pool.clone()));
    let derived_metrics = Arc::new(SqliteDerivedMetricRepo::new(pool.clone()));
    let prices = Arc::new(SqliteHistoricalPriceRepo::new(pool.clone()));
    let current_prices = Arc::new(SqliteCurrentPriceRepo::new(pool.clone()));
    let events = Arc::new(SqliteIngestionEventRepo::new(pool.clone()));

    let deps = IngestionDeps {
        sec,
        market_data,
        companies: companies.clone(),
        filings,
        periods,
        raw_facts,
        normalized_facts: normalized_facts.clone(),
        derived_metrics: derived_metrics.clone(),
        prices: prices.clone(),
        current_prices: current_prices.clone(),
        events,
    };

    let (company, _summary) = ingest_company(&deps, &Ticker("AAPL".into()))
        .await
        .expect("AAPL ingestion (SEC + Yahoo) must succeed");
    let cik = company.cik.clone();

    // ── 1. Prices persisted ──────────────────────────────────────────────
    let price_map = prices.map_for(&cik).await.unwrap();
    assert!(
        price_map.len() >= 10,
        "expected ≥10 end-of-day closes persisted, got {}",
        price_map.len()
    );
    println!("[prices] {} period end-date closes persisted", price_map.len());

    let ctx = ReadCtx {
        normalized_facts: normalized_facts.as_ref(),
        derived_metrics: derived_metrics.as_ref(),
        prices: prices.as_ref(),
        current_prices: current_prices.as_ref(),
    };

    // ── 2. Market cap sane ───────────────────────────────────────────────
    let mcap = series::revenue_aware_series(&ctx, &cik, Metric::HistoricalMarketCap, PeriodKind::Annual)
        .await
        .unwrap();
    assert!(!mcap.is_empty(), "expected a market-cap series for AAPL");
    for p in &mcap {
        assert!(p.value > 0, "market cap must be positive at FY{}", p.period.fiscal_year);
    }
    let latest_mcap = mcap.last().unwrap();
    assert!(
        latest_mcap.value > TRILLION_MICRO && latest_mcap.value < 6 * TRILLION_MICRO,
        "latest AAPL market cap should be $1T–$6T; got {} (= ${:.0}B)",
        latest_mcap.value,
        latest_mcap.value as f64 / 1e15
    );
    println!(
        "[mcap] latest FY{} market cap = ${:.0}B",
        latest_mcap.period.fiscal_year,
        latest_mcap.value as f64 / 1e15
    );

    // ── 3. FCF yield internally consistent + plausible ───────────────────
    let fcf = series_pid_map(&ctx, &cik, Metric::FreeCashFlow, PeriodKind::Annual).await;
    let mcap_pid: std::collections::HashMap<i64, i64> =
        mcap.iter().map(|p| (p.period.id, p.value)).collect();
    let yield_series =
        series::revenue_aware_series(&ctx, &cik, Metric::FreeCashFlowYield, PeriodKind::Annual)
            .await
            .unwrap();
    assert!(!yield_series.is_empty(), "expected an FCF-yield series for AAPL");
    for p in &yield_series {
        let f = fcf.get(&p.period.id).expect("yield period must have FCF");
        let m = mcap_pid.get(&p.period.id).expect("yield period must have market cap");
        let want = derived::fcf_yield_micro(*f, *m).expect("yield must be defined");
        assert_eq!(p.value, want, "FY{} FCF-yield mismatch", p.period.fiscal_year);
    }
    let latest_yield = yield_series.last().unwrap();
    // AAPL's free-cash-flow yield has historically sat in the low single digits.
    assert!(
        latest_yield.value > 2_000 && latest_yield.value < 150_000,
        "latest AAPL FCF yield should be ~0.2%–15%; got {} ({:.2}%)",
        latest_yield.value,
        latest_yield.value as f64 / 1e4
    );
    println!(
        "[yield] latest FY{} FCF yield = {:.2}%",
        latest_yield.period.fiscal_year,
        latest_yield.value as f64 / 1e4
    );

    // ── 4. TTM FCF reconciles with annual at fiscal year end ─────────────
    let ttm_q = series::revenue_aware_series(&ctx, &cik, Metric::FreeCashFlowTtm, PeriodKind::Quarterly)
        .await
        .unwrap();
    assert!(!ttm_q.is_empty(), "expected a quarterly TTM FCF series");
    let annual_by_year: std::collections::HashMap<i32, i64> =
        series::revenue_aware_series(&ctx, &cik, Metric::FreeCashFlow, PeriodKind::Annual)
            .await
            .unwrap()
            .into_iter()
            .map(|p| (p.period.fiscal_year, p.value))
            .collect();

    let mut reconciled = 0usize;
    for t in &ttm_q {
        assert!(
            t.value > 0,
            "TTM FCF should be positive for AAPL at FY{} Q{}",
            t.period.fiscal_year, t.period.fiscal_quarter
        );
        // A TTM ending at the Q4 (fiscal-year-end) quarter equals that fiscal
        // year's annual FCF up to derivation rounding (four quarters = year).
        if t.period.fiscal_quarter == 4 {
            if let Some(&annual) = annual_by_year.get(&t.period.fiscal_year) {
                let tol = (annual.abs() / 10).max(1); // 10% tolerance
                assert!(
                    (t.value - annual).abs() <= tol,
                    "TTM at FY{} Q4 ({}) should reconcile with annual FCF ({}) within 10%",
                    t.period.fiscal_year, t.value, annual
                );
                reconciled += 1;
            }
        }
    }
    assert!(reconciled >= 1, "expected ≥1 fiscal-year-end TTM to reconcile with annual FCF");
    println!("[ttm] {} quarterly TTM points; {} Q4 points reconciled with annual FCF", ttm_q.len(), reconciled);

    println!("\n=== Market cap / TTM / FCF yield verified live against SEC + Yahoo ===");
    drop(dir);
}

async fn series_pid_map(
    ctx: &ReadCtx<'_>,
    cik: &econ_project_lib::domain::Cik,
    metric: Metric,
    kind: PeriodKind,
) -> std::collections::HashMap<i64, i64> {
    series::revenue_aware_series(ctx, cik, metric, kind)
        .await
        .unwrap()
        .into_iter()
        .map(|p| (p.period.id, p.value))
        .collect()
}
