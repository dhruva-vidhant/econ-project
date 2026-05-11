use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::ids::{Cik, Ticker};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Company {
    pub cik: Cik,
    pub ticker: Ticker,
    pub name: String,
    pub exchange: Option<String>,
    pub sic: Option<String>,
    /// MMDD format (e.g., "0926" for late-September FYE).
    pub fiscal_year_end: Option<String>,
    pub added_at: DateTime<Utc>,
    pub last_refreshed: Option<DateTime<Utc>>,
}
