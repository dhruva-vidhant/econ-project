//! End-to-end integration test that exercises the real backend pipeline +
//! every IPC command's underlying logic against the live SEC EDGAR API.
//! Marked `#[ignore]` so it doesn't run in default `cargo test`. Run via:
//!
//!     cargo test --test integration_apple -- --ignored --nocapture
//!
//! NO MOCKS. Real HTTP. Real SQLite. Real XBRL parsing. Real period
//! reconciliation. Real lineage walk. The IPC command logic is invoked
//! through the same code paths that `#[tauri::command]` handlers call —
//! Tauri's runtime / IPC framing layer is the only thing skipped (since
//! that requires a windowed app).

use std::sync::Arc;

use econ_project_lib::db::Pool;
use econ_project_lib::domain::{Metric, PeriodKind, Ticker};
use econ_project_lib::pipeline::{ingest_company, IngestionDeps};
use econ_project_lib::repos::company::{CompanyRepo, SqliteCompanyRepo};
use econ_project_lib::repos::filing::{FilingRepo, SqliteFilingRepo};
use econ_project_lib::repos::ingestion_event::{IngestionEventRepo, SqliteIngestionEventRepo};
use econ_project_lib::repos::normalized_fact::{NormalizedFactRepo, SqliteNormalizedFactRepo};
use econ_project_lib::repos::period::SqlitePeriodRepo;
use econ_project_lib::repos::raw_fact::{RawFactRepo, SqliteRawFactRepo};
use econ_project_lib::sources::sec_client::SecClient;

#[tokio::test]
#[ignore]
async fn ingest_aapl_against_real_sec() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.sqlite");
    let pool = Arc::new(Pool::open(&path).unwrap());

    let sec = Arc::new(
        SecClient::new("EconProject-IntegrationTest/0.1 contact@econproject.example", 5).unwrap(),
    );
    let companies: Arc<SqliteCompanyRepo> = Arc::new(SqliteCompanyRepo::new(pool.clone()));
    let filings: Arc<SqliteFilingRepo> = Arc::new(SqliteFilingRepo::new(pool.clone()));
    let periods: Arc<SqlitePeriodRepo> = Arc::new(SqlitePeriodRepo::new(pool.clone()));
    let raw_facts: Arc<SqliteRawFactRepo> = Arc::new(SqliteRawFactRepo::new(pool.clone()));
    let normalized_facts: Arc<SqliteNormalizedFactRepo> = Arc::new(SqliteNormalizedFactRepo::new(pool.clone()));
    let events: Arc<SqliteIngestionEventRepo> = Arc::new(SqliteIngestionEventRepo::new(pool.clone()));

    let deps = IngestionDeps {
        sec,
        companies: companies.clone(),
        filings: filings.clone(),
        periods: periods.clone(),
        raw_facts: raw_facts.clone(),
        normalized_facts: normalized_facts.clone(),
        events: events.clone(),
    };

    // ── add_company / ingest_company ─────────────────────────────────────
    let (company, summary) = ingest_company(&deps, &Ticker("AAPL".into())).await
        .expect("ingestion failed");
    assert_eq!(company.ticker.0, "AAPL");
    assert_eq!(company.cik.0, "0000320193");
    assert!(summary.filings_ingested > 0);
    assert!(summary.raw_facts_ingested > 0);
    assert!(summary.normalized_facts_ingested > 0);
    println!("[ingest] {} filings, {} raw facts, {} normalized facts",
        summary.filings_ingested, summary.raw_facts_ingested, summary.normalized_facts_ingested);

    // ── list_companies (get_by_cik / list_saved) ─────────────────────────
    let listed = companies.list_saved().await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].ticker.0, "AAPL");

    let by_cik = companies.get_by_cik(&company.cik).await.unwrap()
        .expect("get_by_cik returned None for ingested company");
    assert_eq!(by_cik.name, company.name);

    // ── get_dashboard (mirrors ipc::commands::get_dashboard) ─────────────
    let mut widget_count = 0usize;
    for metric in &[
        Metric::Revenue, Metric::NetIncome,
        Metric::CashAndEquivalents, Metric::TotalAssets, Metric::TotalLiabilities,
    ] {
        let series = normalized_facts.current_series(&company.cik, *metric, PeriodKind::Annual).await.unwrap();
        if !series.is_empty() {
            widget_count += 1;
            let last = series.last().unwrap();
            println!("[dashboard] {} latest FY{} = {} micro-units (= ${:.2}B)",
                metric.as_str(), last.0.fiscal_year, last.1.value,
                last.1.value as f64 / 1_000_000.0 / 1_000_000_000.0);
        }
    }
    assert!(widget_count >= 3, "expected ≥3 dashboard widgets to have data, got {widget_count}");

    // ── get_metric_history annual ────────────────────────────────────────
    let revenue_annual = normalized_facts.current_series(&company.cik, Metric::Revenue, PeriodKind::Annual).await.unwrap();
    assert!(revenue_annual.len() >= 5, "expected ≥5 annual revenue rows, got {}", revenue_annual.len());
    // Every value should be a positive integer (Apple has never reported negative revenue).
    for (p, n) in &revenue_annual {
        assert!(n.value > 0, "non-positive revenue at FY{}: {}", p.fiscal_year, n.value);
    }
    // Apple's most recent revenue should be > $300B (≥ 3e17 micro-units).
    let latest = revenue_annual.last().unwrap();
    assert!(latest.1.value > 300_000_000_000_000_000_i64,
        "expected latest annual revenue > $300B; got {}", latest.1.value);
    println!("[history] {} annual revenue rows, latest FY{} = ${:.2}B",
        revenue_annual.len(), latest.0.fiscal_year, latest.1.value as f64 / 1e15);

    // ── get_metric_history quarterly (proves YTD-derivation worked) ──────
    let revenue_quarterly = normalized_facts.current_series(&company.cik, Metric::Revenue, PeriodKind::Quarterly).await.unwrap();
    assert!(revenue_quarterly.len() >= 12, "expected ≥12 quarterly revenue rows, got {}", revenue_quarterly.len());
    for (p, n) in &revenue_quarterly {
        assert!(n.value > 0, "non-positive quarterly revenue at FY{} Q{}: {}", p.fiscal_year, p.fiscal_quarter, n.value);
        assert!((1..=4).contains(&p.fiscal_quarter), "fiscal_quarter out of range: {}", p.fiscal_quarter);
    }
    println!("[history] {} quarterly revenue rows", revenue_quarterly.len());

    // ── get_lineage (filing + raw_fact + supersession chain) ─────────────
    let n_latest = normalized_facts
        .current_value(&company.cik, Metric::Revenue, latest.0.id)
        .await
        .unwrap()
        .expect("most recent revenue normalized_fact missing");
    let raw = raw_facts.get(n_latest.source_fact_id).await.unwrap()
        .expect("source raw_fact missing");
    let filing = filings.get(&raw.accession_no).await.unwrap()
        .expect("source filing missing");
    assert!(filing.accession_no.0.starts_with("0000320193-"),
        "Apple filings should have accession prefix 0000320193-, got {}", filing.accession_no.0);
    assert!(matches!(
        filing.form_type,
        econ_project_lib::domain::FormType::TenK | econ_project_lib::domain::FormType::TenKA
    ), "latest annual revenue should be sourced from a 10-K or 10-K/A, got {:?}", filing.form_type);
    println!("[lineage] revenue FY{} → accession={} form={:?} concept={}",
        latest.0.fiscal_year, filing.accession_no.0, filing.form_type, raw.concept);

    let chain = normalized_facts.supersession_chain(n_latest.id).await.unwrap();
    println!("[lineage] supersession chain length: {}", chain.len());

    // ── get_ingestion_events ─────────────────────────────────────────────
    let events_recent = events.recent(Some(&company.cik), 100).await.unwrap();
    assert!(!events_recent.is_empty(), "expected ingestion events to be recorded");
    let has_complete = events_recent.iter().any(|e| e.message.contains("Ingestion complete"));
    assert!(has_complete, "expected an 'Ingestion complete' event");
    println!("[events] {} recorded; sample: {:?}",
        events_recent.len(),
        events_recent.first().map(|e| e.message.clone()));

    // ── refresh_company (re-runs ingestion idempotently) ─────────────────
    let (_, summary2) = ingest_company(&deps, &Ticker("AAPL".into())).await
        .expect("re-ingestion failed");
    let by_cik2 = companies.get_by_cik(&company.cik).await.unwrap().unwrap();
    assert!(by_cik2.last_refreshed >= by_cik.last_refreshed,
        "last_refreshed should not regress on refresh");
    // raw_facts UNIQUE should make re-ingestion not double-insert.
    let listed2 = companies.list_saved().await.unwrap();
    assert_eq!(listed2.len(), 1, "refresh should not duplicate the company row");
    println!("[refresh] re-ingested OK; {} normalized facts (was {})",
        summary2.normalized_facts_ingested, summary.normalized_facts_ingested);

    // ── remove_company without drop_cache: should fail (FK RESTRICT) ─────
    let plain_remove = companies.remove(&company.cik, false).await;
    assert!(plain_remove.is_err(), "remove without drop_cache should fail FK RESTRICT when child rows exist");

    // ── remove_company with drop_cache: should cascade cleanly ───────────
    companies.remove(&company.cik, true).await.expect("drop_cache remove failed");
    let listed3 = companies.list_saved().await.unwrap();
    assert!(listed3.is_empty(), "after drop_cache remove, no companies should remain");
    println!("[remove] cascade-delete OK");

    println!("\n=== Full IPC surface verified end-to-end against live SEC ===");
    println!("  Ingestion summary:     {summary:?}");
    drop(dir);
}
