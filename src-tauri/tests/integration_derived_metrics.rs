//! Production-mode end-to-end accuracy test for the read-time derived metrics
//! **free cash flow** and **operating margin** (PRD FR-030/FR-032).
//!
//! Unlike the synthetic unit tests in `derived::tests`, this runs against the
//! *real* production SQLite — the same `data.sqlite` the desktop app reads —
//! exercising the exact `derived::series::revenue_aware_series` code path the
//! IPC handlers use, over real SEC-sourced facts for every saved company.
//! Marked `#[ignore]` (it depends on a populated local DB). Run via:
//!
//!     cargo test --release --test integration_derived_metrics -- --ignored --nocapture
//!
//! Point at a different DB with `ECON_DB=/path/to/data.sqlite`. If no DB is
//! found the test prints a skip notice and passes (so it is safe in CI).
//!
//! What it validates:
//!   1. **Internal consistency** — for every company / period / period-kind,
//!      the FCF series value equals `net_income + D&A − capex` recomputed
//!      independently from the component series, and the operating-margin
//!      series value equals `operating_income ÷ revenue` (×1e6). This proves
//!      the period-join, input-completeness, and derived-capex flow-through
//!      logic is correct against real data.
//!   2. **Completeness** — a derived point exists for exactly the periods that
//!      have all required inputs (no silent drops, no spurious rows).
//!   3. **Known values** — spot-checks against figures hand-verified from the
//!      filings (Zoetis FY2025, Dollar General FY2026).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use econ_project_lib::db::Pool;
use econ_project_lib::derived::{self, series};
use econ_project_lib::domain::{Cik, Metric, PeriodKind};
use econ_project_lib::repos::company::{CompanyRepo, SqliteCompanyRepo};
use econ_project_lib::repos::derived_metric::SqliteDerivedMetricRepo;
use econ_project_lib::repos::normalized_fact::{NormalizedFactRepo, SqliteNormalizedFactRepo};

fn db_path() -> PathBuf {
    if let Ok(p) = std::env::var("ECON_DB") {
        return p.into();
    }
    let home = std::env::var("HOME").expect("HOME not set");
    format!("{home}/Library/Application Support/com.econproject.app/data.sqlite").into()
}

/// `(period_id, value)` map for a directly-stored metric's current series.
async fn direct_map(
    nf: &dyn NormalizedFactRepo,
    cik: &Cik,
    metric: Metric,
    kind: PeriodKind,
) -> HashMap<i64, i64> {
    nf.current_series(cik, metric, kind)
        .await
        .unwrap()
        .into_iter()
        .map(|(p, n)| (p.id, n.value))
        .collect()
}

/// `(period_id, value)` map for a read-time-derived metric series (the exact
/// IPC read path). `&SqliteNormalizedFactRepo` / `&SqliteDerivedMetricRepo`
/// coerce to the `&dyn` trait-object parameters the series API expects.
async fn series_map(
    nf: &SqliteNormalizedFactRepo,
    dm: &SqliteDerivedMetricRepo,
    cik: &Cik,
    metric: Metric,
    kind: PeriodKind,
) -> HashMap<i64, i64> {
    series::revenue_aware_series(nf, dm, cik, metric, kind)
        .await
        .unwrap()
        .into_iter()
        .map(|p| (p.period.id, p.value))
        .collect()
}

#[tokio::test]
#[ignore]
async fn fcf_and_operating_margin_accuracy_against_production_db() {
    let path = db_path();
    if !path.exists() {
        eprintln!(
            "[skip] no production DB at {} (set ECON_DB to run); test passes vacuously",
            path.display()
        );
        return;
    }
    println!("opening production SQLite at: {}", path.display());
    let pool = Arc::new(Pool::open(&path).unwrap());
    let companies = SqliteCompanyRepo::new(pool.clone());
    let nf = SqliteNormalizedFactRepo::new(pool.clone());
    let dm = SqliteDerivedMetricRepo::new(pool.clone());

    let saved = companies.list_saved().await.unwrap();
    assert!(!saved.is_empty(), "production DB has no saved companies to validate");
    println!("validating {} companies", saved.len());

    let mut fcf_periods_checked = 0usize;
    let mut margin_periods_checked = 0usize;

    for company in &saved {
        let cik = &company.cik;
        let ticker = &company.ticker.0;

        for kind in [PeriodKind::Annual, PeriodKind::Quarterly] {
            // ── Free cash flow ────────────────────────────────────────────
            let fcf = series_map(&nf, &dm, cik, Metric::FreeCashFlow, kind.clone()).await;
            let ni = direct_map(&nf, cik, Metric::NetIncome, kind.clone()).await;
            let da = direct_map(&nf, cik, Metric::DepreciationAmortization, kind.clone()).await;
            let capex = series_map(&nf, &dm, cik, Metric::CapitalExpenditures, kind.clone()).await;

            // Internal consistency: every emitted FCF equals the formula
            // recomputed from independently-fetched components.
            for (&pid, &got) in &fcf {
                let (&n, &d, &c) = (
                    ni.get(&pid).expect("FCF period must have net_income"),
                    da.get(&pid).expect("FCF period must have D&A"),
                    capex.get(&pid).expect("FCF period must have capex"),
                );
                let want = derived::free_cash_flow(n, d, c);
                assert_eq!(
                    got, want,
                    "{ticker} {kind:?} pid={pid}: FCF series={got} but ni+da-capex={want}"
                );
                fcf_periods_checked += 1;
            }
            // Completeness: a point exists for exactly the periods with all
            // three inputs present.
            let expected_fcf_periods: std::collections::HashSet<i64> = ni
                .keys()
                .filter(|p| da.contains_key(p) && capex.contains_key(p))
                .copied()
                .collect();
            let got_fcf_periods: std::collections::HashSet<i64> = fcf.keys().copied().collect();
            assert_eq!(
                got_fcf_periods, expected_fcf_periods,
                "{ticker} {kind:?}: FCF coverage mismatch (series vs periods-with-all-inputs)"
            );

            // ── Operating margin ──────────────────────────────────────────
            let margin = series_map(&nf, &dm, cik, Metric::OperatingMargin, kind.clone()).await;
            let oi = direct_map(&nf, cik, Metric::OperatingIncome, kind.clone()).await;
            let rev = series_map(&nf, &dm, cik, Metric::Revenue, kind.clone()).await;

            for (&pid, &got) in &margin {
                let &o = oi.get(&pid).expect("margin period must have operating_income");
                let &r = rev.get(&pid).expect("margin period must have revenue");
                let want =
                    derived::operating_margin_micro(o, r).expect("emitted margin must be defined");
                assert_eq!(
                    got, want,
                    "{ticker} {kind:?} pid={pid}: margin series={got} but oi/rev={want}"
                );
                // Sanity: these are healthy large-caps; no |margin| > 100%.
                assert!(
                    got.abs() <= 1_000_000,
                    "{ticker} {kind:?} pid={pid}: implausible operating margin {got} (>100%)"
                );
                margin_periods_checked += 1;
            }
            let expected_margin_periods: std::collections::HashSet<i64> = oi
                .keys()
                .filter(|p| {
                    rev.get(p)
                        .map(|&r| derived::operating_margin_micro(oi[p], r).is_some())
                        .unwrap_or(false)
                })
                .copied()
                .collect();
            let got_margin_periods: std::collections::HashSet<i64> =
                margin.keys().copied().collect();
            assert_eq!(
                got_margin_periods, expected_margin_periods,
                "{ticker} {kind:?}: operating-margin coverage mismatch"
            );
        }

        println!("  [{ticker}] consistency OK");
    }

    assert!(
        fcf_periods_checked > 0,
        "expected to validate at least one FCF period across all companies"
    );
    assert!(
        margin_periods_checked > 0,
        "expected to validate at least one operating-margin period"
    );
    println!(
        "validated {fcf_periods_checked} FCF periods and {margin_periods_checked} operating-margin periods"
    );

    // ── Known-value spot checks (hand-verified from filings) ──────────────
    spot_check_annual(
        &nf, &dm, &saved, "ZTS", 2025,
        Some(2_539_000_000_000_000), // FCF = $2.539B (NI 2.673 + D&A 0.487 − CapEx 0.621)
        Some(354_917),               // operating margin 35.49%
    )
    .await;
    spot_check_annual(
        &nf, &dm, &saved, "DG", 2026,
        None,
        Some(45_982), // operating margin 4.60%
    )
    .await;
    spot_check_annual(
        &nf, &dm, &saved, "LULU", 2026,
        None,
        Some(201_661), // operating margin 20.17%
    )
    .await;

    println!("\n=== Derived-metric accuracy verified end-to-end against production DB ===");
}

/// Look up a company's annual `(FreeCashFlow, OperatingMargin)` for `fiscal_year`
/// and assert against expected micro-unit values when the data is present.
/// Skips quietly if the company or year is absent (the DB is user-local).
async fn spot_check_annual(
    nf: &SqliteNormalizedFactRepo,
    dm: &SqliteDerivedMetricRepo,
    saved: &[econ_project_lib::domain::Company],
    ticker: &str,
    fiscal_year: i32,
    expect_fcf: Option<i64>,
    expect_margin: Option<i64>,
) {
    let Some(company) = saved.iter().find(|c| c.ticker.0 == ticker) else {
        eprintln!("[spot-check skip] {ticker} not in saved companies");
        return;
    };
    let cik = &company.cik;

    if let Some(want) = expect_fcf {
        let fcf = series::revenue_aware_series(nf, dm, cik, Metric::FreeCashFlow, PeriodKind::Annual)
            .await
            .unwrap();
        match fcf.iter().find(|p| p.period.fiscal_year == fiscal_year) {
            Some(p) => {
                assert_eq!(
                    p.value, want,
                    "{ticker} FY{fiscal_year} FCF: got {} want {want}",
                    p.value
                );
                println!("  [spot] {ticker} FY{fiscal_year} FCF = {} ✓ (${:.3}B)", p.value, p.value as f64 / 1e15);
            }
            None => eprintln!("[spot-check skip] {ticker} FY{fiscal_year} FCF absent"),
        }
    }

    if let Some(want) = expect_margin {
        let m = series::revenue_aware_series(nf, dm, cik, Metric::OperatingMargin, PeriodKind::Annual)
            .await
            .unwrap();
        match m.iter().find(|p| p.period.fiscal_year == fiscal_year) {
            Some(p) => {
                assert_eq!(
                    p.value, want,
                    "{ticker} FY{fiscal_year} operating margin: got {} want {want}",
                    p.value
                );
                println!("  [spot] {ticker} FY{fiscal_year} operating margin = {} ✓ ({:.2}%)", p.value, p.value as f64 / 1e4);
            }
            None => eprintln!("[spot-check skip] {ticker} FY{fiscal_year} margin absent"),
        }
    }
}
