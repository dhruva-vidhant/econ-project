//! Typed error hierarchy. M04 in `docs/tech_spec.md`.
//!
//! - `SourceError`: HTTP/parse/schema problems from external sources.
//! - `RepoError`: persistence-layer problems.
//! - `PipelineError`: pipeline-stage problems (wraps Source/Repo).
//! - `AppError`: top-level IPC-facing error with a stable `code: &'static str`.

use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SourceError {
    #[error("HTTP error {status} for {url}")]
    Http { status: u16, url: String },

    #[error("rate limit exceeded for {url}")]
    RateLimit { url: String },

    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("response did not match expected schema for {url}: {detail}")]
    SchemaMismatch { url: String, detail: String },

    #[error("json decode error: {0}")]
    JsonDecode(#[from] serde_json::Error),

    #[error("ticker {0} not found in SEC ticker map")]
    UnknownTicker(String),

    #[error("source unavailable: {0}")]
    Unavailable(String),
}

#[derive(Debug, Error)]
pub enum RepoError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("connection-pool error: {0}")]
    Pool(String),

    #[error("migration error: {0}")]
    Migration(String),

    #[error("integrity check failed: {0}")]
    Integrity(String),

    #[error("not found")]
    NotFound,
}

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error(transparent)]
    Source(#[from] SourceError),

    #[error(transparent)]
    Repo(#[from] RepoError),

    #[error("normalization error: {0}")]
    Normalize(String),

    #[error("period reconciliation error: {0}")]
    Period(String),

    #[error("currency mismatch (expected {expected}, got {got})")]
    CurrencyMismatch { expected: String, got: String },

    #[error("partial ingestion: {succeeded} succeeded, {skipped} skipped")]
    Partial { succeeded: usize, skipped: usize },
}

/// Top-level error exposed across the IPC boundary. Carries a stable
/// `code` so the UI can branch on it without parsing messages.
#[derive(Debug, Error, Serialize)]
#[serde(tag = "kind", content = "detail")]
pub enum AppError {
    #[error("network error: {message}")]
    Network { code: &'static str, message: String },

    #[error("source error: {message}")]
    Source { code: &'static str, message: String },

    #[error("ingestion error: {message}")]
    Ingestion { code: &'static str, message: String },

    #[error("storage error: {message}")]
    Storage { code: &'static str, message: String },

    #[error("not found: {message}")]
    NotFound { code: &'static str, message: String },

    #[error("invalid input: {message}")]
    InvalidInput { code: &'static str, message: String },

    #[error("internal error: {message}")]
    Internal { code: &'static str, message: String },
}

impl AppError {
    pub fn internal(message: impl Into<String>) -> Self {
        AppError::Internal { code: "internal", message: message.into() }
    }
    pub fn not_found(message: impl Into<String>) -> Self {
        AppError::NotFound { code: "not_found", message: message.into() }
    }
    pub fn invalid(message: impl Into<String>) -> Self {
        AppError::InvalidInput { code: "invalid_input", message: message.into() }
    }
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Network { code, .. }
            | AppError::Source { code, .. }
            | AppError::Ingestion { code, .. }
            | AppError::Storage { code, .. }
            | AppError::NotFound { code, .. }
            | AppError::InvalidInput { code, .. }
            | AppError::Internal { code, .. } => code,
        }
    }
}

impl From<SourceError> for AppError {
    fn from(e: SourceError) -> Self {
        match e {
            SourceError::Http { .. } | SourceError::Network(_) | SourceError::RateLimit { .. } => {
                AppError::Network { code: "network", message: e.to_string() }
            }
            SourceError::UnknownTicker(_) => {
                AppError::NotFound { code: "unknown_ticker", message: e.to_string() }
            }
            _ => AppError::Source { code: "source", message: e.to_string() },
        }
    }
}

impl From<RepoError> for AppError {
    fn from(e: RepoError) -> Self {
        match e {
            RepoError::NotFound => AppError::not_found("record not found"),
            _ => AppError::Storage { code: "storage", message: e.to_string() },
        }
    }
}

impl From<PipelineError> for AppError {
    fn from(e: PipelineError) -> Self {
        match e {
            PipelineError::Source(s) => s.into(),
            PipelineError::Repo(r) => r.into(),
            _ => AppError::Ingestion { code: "ingestion", message: e.to_string() },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_error_codes_are_stable() {
        let e = AppError::not_found("x");
        assert_eq!(e.code(), "not_found");
    }

    #[test]
    fn source_error_to_app_error_unknown_ticker() {
        let s = SourceError::UnknownTicker("AAPL".into());
        let a: AppError = s.into();
        assert_eq!(a.code(), "unknown_ticker");
    }
}
