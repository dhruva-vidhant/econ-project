//! M16 — companyfacts.json fetcher and parser.
//!
//! Verified live: response is `{cik, entityName, facts: { taxonomy: {
//! concept: { label, description, units: { unit: [ { val, accn, fy, fp,
//! form, filed, start, end, frame } ] } } } } }`. USD values are
//! integer dollars at the source — converted to micro-units (×10⁶)
//! during parse to match the §6.2 storage convention.

use std::collections::BTreeMap;

use chrono::{NaiveDate, Utc};
use serde::Deserialize;

use crate::domain::{AccessionNo, Cik, Micro, RawFact, SourceKind, MICRO};
use crate::errors::SourceError;

use super::sec_client::SecClient;

#[derive(Debug, Deserialize)]
pub struct CompanyFactsRoot {
    pub cik: u64,
    #[serde(rename = "entityName", default)]
    pub entity_name: String,
    pub facts: BTreeMap<String, BTreeMap<String, ConceptFacts>>,
}

#[derive(Debug, Deserialize)]
pub struct ConceptFacts {
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub units: BTreeMap<String, Vec<UnitFact>>,
}

#[derive(Debug, Deserialize)]
pub struct UnitFact {
    pub val: serde_json::Number,
    pub accn: String,
    pub fy: Option<i32>,
    pub fp: Option<String>,
    #[serde(default)]
    pub form: Option<String>,
    pub filed: Option<String>,
    pub start: Option<String>,
    pub end: String,
    #[allow(dead_code)]
    #[serde(default)]
    pub frame: Option<String>,
}

pub fn url_for(cik: &Cik) -> String {
    format!("https://data.sec.gov/api/xbrl/companyfacts/CIK{}.json", cik.0)
}

pub async fn fetch_companyfacts(
    client: &SecClient,
    cik: &Cik,
) -> Result<CompanyFactsRoot, SourceError> {
    client.get_json::<CompanyFactsRoot>(&url_for(cik)).await
}

/// Convert raw companyfacts into `RawFact` rows. Values are scaled into
/// the §6.2 micro-unit convention.
pub fn to_raw_facts(cik: &Cik, root: &CompanyFactsRoot) -> Vec<RawFact> {
    let mut out = Vec::new();
    let now = Utc::now();
    for (taxonomy, concepts) in &root.facts {
        for (concept, cf) in concepts {
            for (unit, facts) in &cf.units {
                for f in facts {
                    let value = match scale_to_micro(unit, &f.val) {
                        Some(v) => v,
                        None => continue,
                    };
                    let period_start = f
                        .start
                        .as_deref()
                        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
                    let period_end = match NaiveDate::parse_from_str(&f.end, "%Y-%m-%d") {
                        Ok(d) => d,
                        Err(_) => continue,
                    };
                    let filed = f
                        .filed
                        .as_deref()
                        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
                    let is_instant = period_start.is_none();
                    out.push(RawFact {
                        id: 0,
                        cik: cik.clone(),
                        accession_no: AccessionNo(f.accn.clone()),
                        taxonomy: taxonomy.clone(),
                        concept: concept.clone(),
                        unit: unit.clone(),
                        value_numeric: value,
                        period_start,
                        period_end,
                        is_instant,
                        fy: f.fy,
                        fp: f.fp.clone(),
                        filed,
                        source_kind: SourceKind::XbrlApi,
                        ingested_at: now,
                    });
                }
            }
        }
    }
    out
}

/// Scale a JSON value into the §6.2 micro-unit convention based on its unit.
/// - `USD`, `USD/shares`, `pure` → ×10⁶
/// - `shares` → ×1
/// Returns None on overflow.
fn scale_to_micro(unit: &str, val: &serde_json::Number) -> Option<Micro> {
    let multiplier: i128 = match unit {
        "shares" => 1,
        _ => MICRO as i128,
    };
    if let Some(i) = val.as_i64() {
        let scaled = (i as i128).checked_mul(multiplier)?;
        return i64::try_from(scaled).ok();
    }
    if let Some(f) = val.as_f64() {
        let scaled = (f * multiplier as f64).round();
        if scaled.is_finite() && scaled >= i64::MIN as f64 && scaled <= i64::MAX as f64 {
            return Some(scaled as i64);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_usd_integer_correctly() {
        let v = serde_json::Number::from(215639000000_i64);
        let s = scale_to_micro("USD", &v).unwrap();
        assert_eq!(s, 215639000000_i64 * MICRO);
    }

    #[test]
    fn scales_shares_unchanged() {
        let v = serde_json::Number::from(15_000_000_000_i64);
        let s = scale_to_micro("shares", &v).unwrap();
        assert_eq!(s, 15_000_000_000_i64);
    }

    #[test]
    fn scales_eps_decimal_correctly() {
        let v = serde_json::Number::from_f64(1.234567).unwrap();
        let s = scale_to_micro("USD/shares", &v).unwrap();
        assert_eq!(s, 1_234_567);
    }

    #[test]
    fn url_format() {
        let url = url_for(&Cik("0000320193".into()));
        assert_eq!(url, "https://data.sec.gov/api/xbrl/companyfacts/CIK0000320193.json");
    }

    #[test]
    fn parses_minimal_companyfacts_shape() {
        let json = r#"
        {
          "cik": 320193,
          "entityName": "Apple Inc.",
          "facts": {
            "us-gaap": {
              "Revenues": {
                "label": "Revenues",
                "description": "",
                "units": {
                  "USD": [
                    {
                      "start": "2022-09-25",
                      "end": "2023-09-30",
                      "val": 383285000000,
                      "accn": "0000320193-23-000106",
                      "fy": 2023,
                      "fp": "FY",
                      "form": "10-K",
                      "filed": "2023-11-03",
                      "frame": "CY2023"
                    }
                  ]
                }
              }
            }
          }
        }"#;
        let root: CompanyFactsRoot = serde_json::from_str(json).unwrap();
        let facts = to_raw_facts(&Cik("0000320193".into()), &root);
        assert_eq!(facts.len(), 1);
        let f = &facts[0];
        assert_eq!(f.concept, "Revenues");
        assert_eq!(f.value_numeric, 383285000000_i64 * MICRO);
        assert_eq!(f.fp.as_deref(), Some("FY"));
        assert_eq!(f.fy, Some(2023));
        assert!(!f.is_instant);
    }

    #[test]
    fn parses_instant_fact() {
        let json = r#"
        {
          "cik": 320193,
          "entityName": "Apple Inc.",
          "facts": {
            "us-gaap": {
              "Assets": {
                "label": "Assets",
                "description": "",
                "units": {
                  "USD": [
                    { "end": "2023-09-30", "val": 352755000000, "accn": "0000320193-23-000106",
                      "fy": 2023, "fp": "FY", "form": "10-K", "filed": "2023-11-03" }
                  ]
                }
              }
            }
          }
        }"#;
        let root: CompanyFactsRoot = serde_json::from_str(json).unwrap();
        let facts = to_raw_facts(&Cik("0000320193".into()), &root);
        assert_eq!(facts.len(), 1);
        assert!(facts[0].is_instant);
        assert!(facts[0].period_start.is_none());
    }
}
