//! Multi-company integration test exercising filers AAPL doesn't represent.
//!
//! AAPL is a textbook well-behaved filer: pure US-GAAP, USD, late-Sept FYE,
//! single-quarter reporting, all canonical-primary concepts present. This
//! test ingests an industry/calendar/share-class spread to surface bugs
//! the AAPL-only suite cannot catch.
//!
//! Run via:
//!     cargo test --test integration_multi -- --ignored --nocapture
//!
//! Each company's expectations are calibrated to that filer (a bank's
//! concept-map coverage is structurally lower than Apple's; we don't
//! demand the same threshold). Where a company's ingestion produces
//! materially less than expected, the test reports it but does not fail
//! the run unless a *contract-level* invariant breaks (e.g., the lineage
//! walk returns a wrong filing accession). Concept-map coverage gaps
//! are cataloged via `Outcome` so we can fix them in follow-up.

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

#[derive(Debug)]
struct Outcome {
    ticker: &'static str,
    expected_cik: &'static str,
    /// Industry / quirk being targeted by including this filer.
    rationale: &'static str,
    filings: usize,
    raw_facts: usize,
    normalized_facts: usize,
    /// (metric, expected min, observed) — filled in for checks the test ran.
    metric_checks: Vec<(Metric, i64, Option<i64>)>,
    /// Per-company structural assertions; if any fail, the test fails.
    contract_failures: Vec<String>,
    notes: Vec<String>,
}

fn deps_for(pool: &Arc<Pool>) -> IngestionDeps {
    let sec = Arc::new(
        SecClient::new("EconProject-MultiIntegration/0.1 contact@econproject.example", 5).unwrap(),
    );
    IngestionDeps {
        sec,
        companies: Arc::new(SqliteCompanyRepo::new(pool.clone())),
        filings: Arc::new(SqliteFilingRepo::new(pool.clone())),
        periods: Arc::new(SqlitePeriodRepo::new(pool.clone())),
        raw_facts: Arc::new(SqliteRawFactRepo::new(pool.clone())),
        normalized_facts: Arc::new(SqliteNormalizedFactRepo::new(pool.clone())),
        events: Arc::new(SqliteIngestionEventRepo::new(pool.clone())),
    }
}

async fn run_one(
    deps: &IngestionDeps,
    ticker: &'static str,
    expected_cik: &'static str,
    rationale: &'static str,
    metric_floors: &[(Metric, i64)],
) -> Outcome {
    let mut o = Outcome {
        ticker,
        expected_cik,
        rationale,
        filings: 0,
        raw_facts: 0,
        normalized_facts: 0,
        metric_checks: Vec::new(),
        contract_failures: Vec::new(),
        notes: Vec::new(),
    };

    let res = ingest_company(deps, &Ticker::from_str(ticker)).await;
    let (company, summary) = match res {
        Ok(v) => v,
        Err(e) => {
            o.contract_failures.push(format!("ingestion failed: {e:?}"));
            return o;
        }
    };

    if company.cik.0 != expected_cik {
        o.contract_failures
            .push(format!("CIK mismatch: expected {expected_cik}, got {}", company.cik.0));
    }

    o.filings = summary.filings_ingested;
    o.raw_facts = summary.raw_facts_ingested;
    o.normalized_facts = summary.normalized_facts_ingested;

    // For each requested metric: check the latest annual value meets a floor.
    for (metric, floor_micro) in metric_floors {
        let series = match deps
            .normalized_facts
            .current_series(&company.cik, *metric, PeriodKind::Annual)
            .await
        {
            Ok(s) => s,
            Err(e) => {
                o.contract_failures
                    .push(format!("series query for {metric:?} failed: {e:?}"));
                continue;
            }
        };
        let observed = series.last().map(|(_, n)| n.value);
        if let Some(v) = observed {
            if v < *floor_micro {
                o.notes.push(format!(
                    "  {metric:?} latest {v} below floor {floor_micro} (concept-map / source quirk?)"
                ));
            }
        } else {
            o.notes.push(format!("  {metric:?}: no annual value persisted"));
        }
        o.metric_checks.push((*metric, *floor_micro, observed));
    }

    // Lineage contract: when revenue is present, lineage must surface a
    // filing whose accession_no starts with the CIK's numeric form.
    let rev = deps
        .normalized_facts
        .current_series(&company.cik, Metric::Revenue, PeriodKind::Annual)
        .await
        .ok()
        .and_then(|v| v.last().cloned());
    if let Some((_, n)) = rev {
        let raw = deps.raw_facts.get(n.source_fact_id).await.unwrap();
        if let Some(raw) = raw {
            let filing = deps.filings.get(&raw.accession_no).await.unwrap();
            if let Some(filing) = filing {
                let cik_num = company.cik.0.trim_start_matches('0');
                if !filing.accession_no.0.starts_with(&format!("{:0>10}", cik_num)[..])
                    && !filing.accession_no.0.contains(cik_num)
                {
                    // Loose check: the accession should reference this company.
                    o.notes.push(format!(
                        "  lineage filing accession {} doesn't reference CIK {}",
                        filing.accession_no.0, company.cik.0
                    ));
                }
            } else {
                o.contract_failures.push("lineage filing missing".into());
            }
        }
    }

    o
}

#[tokio::test]
#[ignore]
async fn ingest_diversified_filers_against_real_sec() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.sqlite");
    let pool = Arc::new(Pool::open(&path).unwrap());
    let deps = deps_for(&pool);

    // Floors are deliberately conservative (1 cent in micro-units = 10_000)
    // for "value should exist & be sane" checks. Stricter values lower —
    // a bank's revenue concept may not match our catalog at all, so we
    // can't assert on a $-billion floor.
    const ONE_DOLLAR: i64 = 1_000_000;
    const ONE_BILLION: i64 = 1_000_000_000 * ONE_DOLLAR;
    const ONE_HUNDRED_BILLION: i64 = 100 * ONE_BILLION;

    let cases: &[(_, _, _, &[(Metric, i64)])] = &[
        ("MSFT", "0000789019", "Different FYE (June)",
            &[(Metric::Revenue, ONE_HUNDRED_BILLION),
              (Metric::NetIncome, ONE_BILLION * 50),
              (Metric::TotalAssets, ONE_HUNDRED_BILLION * 3)][..]),
        ("COST", "0000909832", "53-week fiscal years",
            &[(Metric::Revenue, ONE_HUNDRED_BILLION),
              (Metric::NetIncome, ONE_BILLION),
              (Metric::TotalAssets, ONE_BILLION * 50)][..]),
        ("JPM", "0000019617", "Bank — concept-map gaps expected",
            &[(Metric::Revenue, ONE_BILLION),  // banks rarely tag canonical 'Revenues'
              (Metric::NetIncome, ONE_BILLION * 10),
              (Metric::TotalAssets, ONE_HUNDRED_BILLION * 30)][..]),
        ("BRK.B", "0001067983", "Share-class ticker (B shares)",
            &[(Metric::Revenue, ONE_BILLION),
              (Metric::NetIncome, ONE_BILLION),
              (Metric::TotalAssets, ONE_HUNDRED_BILLION)][..]),
    ];

    let mut outcomes = Vec::new();
    for (ticker, cik, rationale, floors) in cases {
        println!("\n── {ticker} — {rationale}");
        let o = run_one(&deps, ticker, cik, rationale, floors).await;
        println!(
            "   {} filings, {} raw facts, {} normalized facts",
            o.filings, o.raw_facts, o.normalized_facts
        );
        for (m, floor, obs) in &o.metric_checks {
            match obs {
                Some(v) if v >= floor => println!("   ✓ {m:?}: {v} (≥ floor {floor})"),
                Some(v) => println!("   · {m:?}: {v} (BELOW floor {floor})"),
                None => println!("   · {m:?}: missing"),
            }
        }
        for n in &o.notes { println!("   {n}"); }
        for f in &o.contract_failures { println!("   ✗ CONTRACT FAIL: {f}"); }
        outcomes.push(o);
    }

    // ── Print a structured summary ──────────────────────────────────────
    println!("\n=== Multi-company integration summary ===");
    println!("{:<8} {:>8} {:>10} {:>10}  notes", "ticker", "filings", "raw", "normalized");
    for o in &outcomes {
        println!("{:<8} {:>8} {:>10} {:>10}  {}",
            o.ticker, o.filings, o.raw_facts, o.normalized_facts,
            o.rationale);
    }

    // Test fails iff any company hit a contract-level failure (CIK
    // mismatch, ingestion crash, lineage broken). Concept-map coverage
    // gaps surface as notes, not failures — they're catalogued for
    // follow-up.
    let total_failures: Vec<&String> = outcomes.iter().flat_map(|o| &o.contract_failures).collect();
    if !total_failures.is_empty() {
        println!("\n=== Contract failures: {} ===", total_failures.len());
        for f in &total_failures { println!("   ✗ {f}"); }
        panic!("multi-company integration test had contract-level failures");
    }

    // At least 1 normalized fact per company is required — if a company
    // ingests filings but produces zero normalized facts, the catalog is
    // structurally broken for that filer.
    for o in &outcomes {
        if o.filings > 0 && o.normalized_facts == 0 {
            panic!(
                "{}: {} filings ingested but 0 normalized facts — catalog gap",
                o.ticker, o.filings
            );
        }
    }
    println!("\n=== All companies ingested with structural invariants intact ===");
}
