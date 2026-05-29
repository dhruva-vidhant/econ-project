//! M15 — submissions.json fetcher + parser.
//!
//! Verified live during the architecture review pass: the response contains
//! `cik`, `entityName`, `tickers`, `exchanges`, `fiscalYearEnd`, and
//! `filings.recent` with parallel arrays of accession numbers, form types,
//! filing dates, and items (the field we use to detect Item 4.02).

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Deserializer};

use crate::domain::{AccessionNo, Cik, Filing, FormType};
use crate::errors::SourceError;

use super::sec_client::SecClient;

/// Accept either a JSON string or null/missing as an empty string.
/// Foreign private issuers (e.g., BABA, NVO) get `entityName: null` and
/// `fiscalYearEnd: null` from `data.sec.gov/submissions/`, which the
/// previous `String + #[serde(default)]` declaration could not parse —
/// `default` only fills in *absent* fields, not present-and-null ones.
fn null_or_string<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(Option::<String>::deserialize(d)?.unwrap_or_default())
}

#[derive(Debug, Deserialize)]
pub struct SubmissionsRoot {
    pub cik: String,
    #[serde(rename = "entityName", default, deserialize_with = "null_or_string")]
    pub entity_name: String,
    #[serde(default)]
    pub tickers: Vec<String>,
    #[serde(default)]
    pub exchanges: Vec<String>,
    #[serde(rename = "fiscalYearEnd", default, deserialize_with = "null_or_string")]
    pub fiscal_year_end: String,
    pub filings: SubmissionsFilings,
}

impl SubmissionsRoot {
    /// Returns a usable fiscal-year-end string ("MMDD"). When the SEC
    /// `fiscalYearEnd` field is missing or null (typical for foreign
    /// private issuers), derives MMDD from the most recent annual
    /// filing's `reportDate`. Falls back to the calendar-year default
    /// "1231" only if no annual filing carries a parseable report date.
    pub fn resolved_fiscal_year_end(&self) -> String {
        if !self.fiscal_year_end.is_empty() {
            return self.fiscal_year_end.clone();
        }
        let r = &self.filings.recent;
        let n = r.accession_number.len();
        let mut best: Option<(NaiveDate, String)> = None;
        for i in 0..n {
            let form = FormType::from_str(r.form.get(i).map(String::as_str).unwrap_or(""));
            if !form.is_annual() { continue; }
            let report = match r.report_date.get(i) {
                Some(s) if !s.is_empty() => match NaiveDate::parse_from_str(s, "%Y-%m-%d") {
                    Ok(d) => d,
                    Err(_) => continue,
                },
                _ => continue,
            };
            let mmdd = format!("{:02}{:02}", report.month(), report.day());
            match &best {
                Some((d, _)) if *d >= report => {}
                _ => best = Some((report, mmdd)),
            }
        }
        best.map(|(_, mmdd)| mmdd).unwrap_or_else(|| "1231".to_string())
    }
}

#[derive(Debug, Deserialize)]
pub struct SubmissionsFilings {
    pub recent: SubmissionsRecent,
}

#[derive(Debug, Deserialize)]
pub struct SubmissionsRecent {
    #[serde(rename = "accessionNumber", default)]
    pub accession_number: Vec<String>,
    #[serde(rename = "filingDate", default)]
    pub filing_date: Vec<String>,
    #[serde(rename = "reportDate", default)]
    pub report_date: Vec<String>,
    #[serde(default)]
    pub form: Vec<String>,
    #[serde(default)]
    pub items: Vec<String>,
    #[serde(rename = "isXBRL", default)]
    pub is_xbrl: Vec<i64>,
}

pub fn url_for(cik: &Cik) -> String {
    format!("https://data.sec.gov/submissions/CIK{}.json", cik.0)
}

pub async fn fetch_submissions(client: &SecClient, cik: &Cik) -> Result<SubmissionsRoot, SourceError> {
    let url = url_for(cik);
    client.get_json::<SubmissionsRoot>(&url).await
}

/// Convert a `SubmissionsRoot` into `Filing` rows for persistence.
pub fn to_filings(cik: &Cik, root: &SubmissionsRoot) -> Vec<Filing> {
    let r = &root.filings.recent;
    let n = r.accession_number.len();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let form_str = r.form.get(i).cloned().unwrap_or_default();
        let form = FormType::from_str(&form_str);
        let filed_at = r
            .filing_date
            .get(i)
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
            .unwrap_or_else(|| NaiveDate::from_ymd_opt(1970, 1, 1).unwrap());
        let period_of_report = r
            .report_date
            .get(i)
            .and_then(|s| if s.is_empty() { None } else { NaiveDate::parse_from_str(s, "%Y-%m-%d").ok() });
        let items = r.items.get(i).cloned().unwrap_or_default();
        let is_4_02 = matches!(form, FormType::EightK)
            && items.split(',').any(|s| s.trim() == "4.02");
        out.push(Filing {
            accession_no: AccessionNo(r.accession_number[i].clone()),
            cik: cik.clone(),
            form_type: form.clone(),
            filed_at,
            period_of_report,
            is_amendment: form.is_amendment(),
            amends: None,
            item_4_02_8k: is_4_02,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_format() {
        let url = url_for(&Cik("0000320193".into()));
        assert_eq!(url, "https://data.sec.gov/submissions/CIK0000320193.json");
    }

    #[test]
    fn detects_item_4_02_8k() {
        let root = SubmissionsRoot {
            cik: "320193".into(),
            entity_name: "Apple".into(),
            tickers: vec!["AAPL".into()],
            exchanges: vec!["Nasdaq".into()],
            fiscal_year_end: "0926".into(),
            filings: SubmissionsFilings {
                recent: SubmissionsRecent {
                    accession_number: vec!["0000320193-24-000001".into(), "0000320193-24-000002".into()],
                    filing_date: vec!["2024-01-15".into(), "2024-02-01".into()],
                    report_date: vec!["2023-12-31".into(), "".into()],
                    form: vec!["8-K".into(), "8-K".into()],
                    items: vec!["4.02,9.01".into(), "1.01,9.01".into()],
                    is_xbrl: vec![1, 0],
                },
            },
        };
        let cik = Cik("0000320193".into());
        let filings = to_filings(&cik, &root);
        assert_eq!(filings.len(), 2);
        assert!(filings[0].item_4_02_8k);
        assert!(!filings[1].item_4_02_8k);
    }

    #[test]
    fn parses_null_entity_name_and_fiscal_year_end() {
        // Foreign private issuers (BABA, NVO, etc.) get JSON nulls for
        // these fields. Pre-fix this would fail with "invalid type: null,
        // expected a string".
        let json = r#"{
            "cik": "0001577552",
            "entityName": null,
            "tickers": ["BABA"],
            "exchanges": ["NYSE"],
            "fiscalYearEnd": null,
            "filings": { "recent": {
                "accessionNumber": [], "filingDate": [], "reportDate": [],
                "form": [], "items": [], "isXBRL": []
            }}
        }"#;
        let root: SubmissionsRoot = serde_json::from_str(json).unwrap();
        assert_eq!(root.entity_name, "");
        assert_eq!(root.fiscal_year_end, "");
    }

    #[test]
    fn fye_falls_back_to_latest_annual_report_date() {
        // When fiscalYearEnd is empty, derive MMDD from the latest annual
        // filing's reportDate. BABA's most-recent 20-F reports on March 31.
        let root = SubmissionsRoot {
            cik: "0001577552".into(),
            entity_name: "".into(),
            tickers: vec!["BABA".into()],
            exchanges: vec!["NYSE".into()],
            fiscal_year_end: "".into(),
            filings: SubmissionsFilings {
                recent: SubmissionsRecent {
                    accession_number: vec!["a".into(), "b".into(), "c".into()],
                    filing_date: vec!["2026-05-28".into(), "2025-07-29".into(), "2024-07-30".into()],
                    report_date: vec!["".into(), "2025-03-31".into(), "2024-03-31".into()],
                    form: vec!["6-K".into(), "20-F".into(), "20-F".into()],
                    items: vec!["".into(); 3],
                    is_xbrl: vec![0, 1, 1],
                },
            },
        };
        assert_eq!(root.resolved_fiscal_year_end(), "0331");
    }

    #[test]
    fn fye_falls_back_to_calendar_year_when_no_annual_report() {
        let root = SubmissionsRoot {
            cik: "x".into(), entity_name: "x".into(),
            tickers: vec![], exchanges: vec![],
            fiscal_year_end: "".into(),
            filings: SubmissionsFilings {
                recent: SubmissionsRecent {
                    accession_number: vec!["a".into()],
                    filing_date: vec!["2024-05-01".into()],
                    report_date: vec!["".into()],
                    form: vec!["6-K".into()],
                    items: vec!["".into()],
                    is_xbrl: vec![0],
                },
            },
        };
        assert_eq!(root.resolved_fiscal_year_end(), "1231");
    }

    #[test]
    fn fye_passes_through_when_present() {
        let root = SubmissionsRoot {
            cik: "x".into(), entity_name: "x".into(),
            tickers: vec![], exchanges: vec![],
            fiscal_year_end: "0926".into(),
            filings: SubmissionsFilings {
                recent: SubmissionsRecent {
                    accession_number: vec![], filing_date: vec![],
                    report_date: vec![], form: vec![], items: vec![],
                    is_xbrl: vec![],
                },
            },
        };
        assert_eq!(root.resolved_fiscal_year_end(), "0926");
    }

    #[test]
    fn maps_form_types_correctly() {
        let root = SubmissionsRoot {
            cik: "x".into(), entity_name: "x".into(), tickers: vec![], exchanges: vec![],
            fiscal_year_end: "x".into(),
            filings: SubmissionsFilings {
                recent: SubmissionsRecent {
                    accession_number: vec!["a".into(), "b".into(), "c".into()],
                    filing_date: vec!["2024-01-01".into(); 3],
                    report_date: vec!["".into(); 3],
                    form: vec!["10-K".into(), "10-Q/A".into(), "8-K".into()],
                    items: vec!["".into(); 3],
                    is_xbrl: vec![1, 1, 0],
                },
            },
        };
        let f = to_filings(&Cik("0000000001".into()), &root);
        assert_eq!(f[0].form_type, FormType::TenK);
        assert!(f[1].is_amendment);
        assert_eq!(f[1].form_type, FormType::TenQA);
        assert_eq!(f[2].form_type, FormType::EightK);
    }

    #[test]
    fn maps_foreign_issuer_form_types() {
        let root = SubmissionsRoot {
            cik: "x".into(), entity_name: "x".into(),
            tickers: vec![], exchanges: vec![],
            fiscal_year_end: "0331".into(),
            filings: SubmissionsFilings {
                recent: SubmissionsRecent {
                    accession_number: vec!["a".into(), "b".into()],
                    filing_date: vec!["2025-07-29".into(), "2025-09-01".into()],
                    report_date: vec!["2025-03-31".into(), "2025-03-31".into()],
                    form: vec!["20-F".into(), "20-F/A".into()],
                    items: vec!["".into(); 2],
                    is_xbrl: vec![1, 1],
                },
            },
        };
        let f = to_filings(&Cik("0001577552".into()), &root);
        assert_eq!(f[0].form_type, FormType::TwentyF);
        assert_eq!(f[1].form_type, FormType::TwentyFA);
        assert!(f[1].is_amendment);
        assert!(f[0].form_type.is_annual());
        assert!(f[1].form_type.is_annual());
    }
}
