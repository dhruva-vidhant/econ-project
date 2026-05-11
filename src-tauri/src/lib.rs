//! Library entry point for the EconProject Tauri application.
//!
//! Module layout (matches `docs/tech_spec.md` §3):
//! - `domain` (M03): canonical Rust types used across modules.
//! - `errors` (M04): typed error hierarchy + IPC `AppError`.
//! - `db` (M02, M05): SQLite migrations + connection pool.
//! - `repos` (M06–M12): per-table repositories.
//! - `sources` (M13–M17): SEC EDGAR + market-data adapters.
//! - `normalize` (M18–M21): canonical metric catalog + period/unit/sign rules.
//! - `pipeline` (M22–M27): ingestion stages + orchestrator.
//! - `derived` (M28): derived metric formulas.
//! - `ipc` (M29–M30): typed Tauri commands + event channel.

pub mod domain;
pub mod errors;
pub mod db;
pub mod repos;
pub mod sources;
pub mod normalize;
pub mod pipeline;
pub mod derived;
pub mod ipc;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,econ_project_lib=debug".into()),
        )
        .init();

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init());
    // Embedded W3C WebDriver server, gated on the `e2e-webdriver`
    // feature. Production builds (`cargo build` / `npm run tauri build`)
    // do NOT include this dependency at all — the crate is `optional`
    // in Cargo.toml and only linked when the feature is on. Activate
    // with `cargo build --features e2e-webdriver` for E2E test runs.
    #[cfg(feature = "e2e-webdriver")]
    let builder = builder.plugin(tauri_plugin_wdio_webdriver::init());
    builder
        .setup(ipc::setup)
        .invoke_handler(ipc::handler())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
