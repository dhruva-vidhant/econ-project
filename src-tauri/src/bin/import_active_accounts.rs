//! One-shot custom-metric loader for PayPal's "X.
//!
//! Source: `paypal_active_accounts_by_quarter.csv` (project root).
//! Target: production SQLite at ~/Library/Application Support/com.econproject.app/data.sqlite
//! Company: PayPal Holdings (CIK 0001633917)
//!
//! The CSV has two columns: "Quarter" (e.g. "Q3 2026") and active accounts in
//! millions (e.g. "440"); values are converted to an absolute count on import. This binary transforms to the canonical import format
//! (metric, period, value, unit, label) and calls `import_csv` directly.
//!
//! Run with:
//!   cargo run --bin import_active_accounts
//!
//! Idempotent — safe to re-run after CSV updates. Points that already exist are
//! left untouched; only new periods are added.
//!
//! To create a new custom-metric loader for a different CSV:
//!   1. Copy this file to src/bin/import_<metric_name>.rs
//!   2. Update `transform_csv` for the new CSV's column layout
//!   3. Set the correct CIK, metric key, unit type, and granularity
//!   4. Add a [[bin]] entry in Cargo.toml
//!   5. Run with: cargo run --bin import_<metric_name>

use std::path::PathBuf;
use std::sync::Arc;

use econ_project_lib::db::Pool;
use econ_project_lib::domain::{Cik, PeriodKind, UnitKind};
use econ_project_lib::import::csv::{import_csv, ImportOptions};
use econ_project_lib::repos::custom_metric::SqliteCustomMetricRepo;
use econ_project_lib::repos::period::{PeriodRepo, SqlitePeriodRepo};

fn transform_csv(raw: &str) -> String {
    let mut out = String::from("metric,period,value,unit,label\n");
    for line in raw.lines().skip(1) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.splitn(2, ',');
        let quarter_token = parts.next().unwrap().trim();
        let value_raw = parts.next().unwrap_or("").trim();
        if value_raw.is_empty() {
            continue;
        }

        // Millions -> absolute count.
        let value_clean = match value_raw.replace(',', "").parse::<f64>() {
            Ok(m) => format!("{}", (m * 1_000_000.0).round() as i64),
            Err(_) => continue,
        };

        // "Q3 2015" → "FY2015 Q3"
        let fiscal_token = if let Some(rest) = quarter_token.strip_prefix('Q') {
            let mut pieces = rest.splitn(2, ' ');
            let q = pieces.next().unwrap();
            let year = pieces.next().unwrap_or("");
            format!("FY{year} Q{q}")
        } else {
            quarter_token.to_string()
        };

        out.push_str(&format!(
            "active_accounts,{},{},count,Active Accounts\n",
            fiscal_token, value_clean
        ));
    }
    out
}

#[tokio::main]
async fn main() {
    let db_path = dirs::data_dir()
        .expect("no data dir")
        .join("com.econproject.app")
        .join("data.sqlite");
    assert!(db_path.exists(), "Production DB not found at {}", db_path.display());

    let csv_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("paypal_active_accounts_by_quarter.csv");
    assert!(csv_path.exists(), "CSV not found at {}", csv_path.display());

    let raw_csv = std::fs::read_to_string(&csv_path).expect("read CSV");
    let transformed = transform_csv(&raw_csv);

    println!("Transformed CSV:\n{transformed}");

    let pool = Arc::new(Pool::open(&db_path).expect("open DB"));
    let cik = Cik("0001633917".into());

    let period_repo = SqlitePeriodRepo::new(pool.clone());
    let periods = period_repo.list_for_cik(&cik, None).await.expect("list periods");
    println!("Found {} periods for PayPal", periods.len());

    let custom_repo = SqliteCustomMetricRepo::new(pool);
    let options = ImportOptions {
        granularity: PeriodKind::Quarterly,
        default_unit: UnitKind::Count,
        skip_existing: true,
    };

    let report = import_csv(&cik, &transformed, &options, &custom_repo, &periods)
        .await
        .expect("import_csv");

    println!("\n=== Import Report ===");
    println!("Metrics created: {:?}", report.metrics_created);
    println!("Imported: {}", report.imported);
    println!("Updated: {}", report.updated);
    println!("Skipped (already present): {}", report.skipped);
    println!("Unaligned: {}", report.unaligned);
    if !report.rejected.is_empty() {
        println!("Rejected:");
        for r in &report.rejected {
            println!("  row {}: {} — {}", r.row_index, r.reason, r.raw);
        }
    }
    println!("\nDone.");
}
