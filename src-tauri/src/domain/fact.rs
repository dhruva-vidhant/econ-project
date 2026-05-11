use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use super::ids::{AccessionNo, Cik};
use super::metric::Metric;
use super::Micro;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceKind {
    #[serde(rename = "xbrl_api")] XbrlApi,
    #[serde(rename = "xbrl_xml")] XbrlXml,
}

impl SourceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            SourceKind::XbrlApi => "xbrl_api",
            SourceKind::XbrlXml => "xbrl_xml",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "xbrl_api" => Some(SourceKind::XbrlApi),
            "xbrl_xml" => Some(SourceKind::XbrlXml),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawFact {
    pub id: i64,
    pub cik: Cik,
    pub accession_no: AccessionNo,
    pub taxonomy: String,
    pub concept: String,
    pub unit: String,
    pub value_numeric: Micro,
    pub period_start: Option<NaiveDate>,
    pub period_end: NaiveDate,
    pub is_instant: bool,
    pub fy: Option<i32>,
    pub fp: Option<String>,
    pub filed: Option<NaiveDate>,
    pub source_kind: SourceKind,
    pub ingested_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedFact {
    pub id: i64,
    pub cik: Cik,
    pub metric: Metric,
    pub period_id: i64,
    pub value: Micro,
    pub unit: String,
    pub source_fact_id: i64,
    pub source_kind: SourceKind,
    pub is_primary: bool,
    pub original_value: Option<Micro>,
    pub original_unit: Option<String>,
    pub fx_rate_micro: Option<i64>,
    pub fx_rate_source: Option<String>,
    pub fx_rate_date: Option<NaiveDate>,
    pub superseded_by: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DerivedMetric {
    pub id: i64,
    pub cik: Cik,
    pub formula_id: String,
    pub period_id: i64,
    pub value: Option<Micro>,
    pub is_complete: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_kind_round_trip() {
        for k in [SourceKind::XbrlApi, SourceKind::XbrlXml] {
            assert_eq!(SourceKind::from_str(k.as_str()), Some(k));
        }
    }
}
