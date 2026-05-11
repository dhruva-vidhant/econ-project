use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::ids::{AccessionNo, Cik};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    #[serde(rename = "info")] Info,
    #[serde(rename = "warn")] Warn,
    #[serde(rename = "error")] Error,
}

impl Severity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warn => "warn",
            Severity::Error => "error",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "info" => Some(Severity::Info),
            "warn" => Some(Severity::Warn),
            "error" => Some(Severity::Error),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IngestionEvent {
    pub id: i64,
    pub cik: Option<Cik>,
    pub accession_no: Option<AccessionNo>,
    pub stage: String,
    pub level: Severity,
    pub user_visible: bool,
    pub message: String,
    pub detail_json: Option<String>,
    pub occurred_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_round_trip() {
        for s in [Severity::Info, Severity::Warn, Severity::Error] {
            assert_eq!(Severity::from_str(s.as_str()), Some(s));
        }
    }
}
