//! Domain types — M03. Canonical Rust types used across all modules.
//! See `docs/tech_spec.md` §4 for the full type catalog.

mod ids;
mod filing;
mod period;
mod fact;
mod metric;
mod event;
mod company;
mod lineage;

pub use company::*;
pub use event::*;
pub use fact::*;
pub use filing::*;
pub use ids::*;
pub use lineage::*;
pub use metric::*;
pub use period::*;

/// All currency / per-share values use INTEGER micro-units (×1,000,000).
/// See `docs/architecture.md` §6.2.
pub type Micro = i64;

/// Multiplier used for the micro-unit storage convention.
pub const MICRO: i64 = 1_000_000;
