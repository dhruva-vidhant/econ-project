//! Ingestion pipeline — M22-M27 (orchestrator + stages, V1 simplified).
//!
//! V1 implementation handles the common path:
//! 1. Discover: ticker → CIK + submissions index.
//! 2. Download: submissions.json + companyfacts.json.
//! 3. Parse: companyfacts → RawFacts (USD scaled to micro-units).
//! 4. Normalize: Concept-map → Metric; period reconciliation for
//!    direct-quarterly + annual values; sign convention applied.
//! 5. Persist: filings, periods, raw_facts, normalized_facts.
//!
//! Deferred for V1 (logged via `ingestion_event` when encountered):
//! - YTD → single-quarter derivation (Q3 = 9M − H1, etc.)
//! - 52/53-week detection
//! - Restatement supersession chain population
//! - Item 4.02 8-K HTML parsing
//! - FX conversion for non-USD filers
//! - Concept map alternates / `is_primary = 0` rows
//!
//! These are tracked in `docs/followup.md` (created at end of session).

pub mod orchestrator;

pub use orchestrator::{ingest_company, IngestionDeps, IngestionSummary};
