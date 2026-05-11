//! IPC layer — M29 (typed Tauri commands), M30 (event channel).
//!
//! Public surface used by `lib::run`:
//! - `setup`: called inside `Builder::setup`; wires up app state.
//! - `handler`: returns the assembled `tauri::generate_handler!` macro output
//!   so all commands are registered. We use a function pointer here rather
//!   than calling the macro inline so the command list lives in one place.

use serde::Serialize;
use tauri::Manager;

use crate::errors::AppError;

mod state;
mod commands;

pub use state::AppState;

/// Typed responses (also used by the frontend client).
#[derive(Debug, Clone, Serialize)]
pub struct PingResponse {
    pub message: String,
    pub version: &'static str,
}

#[tauri::command]
fn ping() -> Result<PingResponse, AppError> {
    Ok(PingResponse {
        message: "pong".into(),
        version: env!("CARGO_PKG_VERSION"),
    })
}

pub fn setup<R: tauri::Runtime>(app: &mut tauri::App<R>) -> Result<(), Box<dyn std::error::Error>> {
    let state = state::AppState::initialize(app.handle())?;
    app.manage(state);
    Ok(())
}

/// Returns the invoke handler that maps `#[tauri::command]`s to invocations.
pub fn handler() -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        ping,
        commands::list_companies,
        commands::add_company,
        commands::remove_company,
        commands::get_metric_history,
        commands::get_dashboard,
        commands::get_ingestion_events,
        commands::get_supersession_chain,
    ]
}
