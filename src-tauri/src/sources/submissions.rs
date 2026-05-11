//! M15 — submissions.json fetcher + parser.
//!
//! Verified live during the architecture review pass: the response contains
//! `cik`, `entityName`, `tickers`, `exchanges`, `fiscalYearEnd`, and
//! `filings.recent` with parallel arrays of accession numbers, form types,
//! filing dates, and items (the field we use to detect Item 4.02).

use chrono::NaiveDate;
use serde::Deserialize;

use crate::domain::{AccessionNo, Cik, Filing, FormType};
use crate::errors::SourceError;

use super::sec_client::SecClient;

#[derive(Debug, Deserialize)]
pub struct SubmissionsRoot {
    pub cik: String,
    #[serde(rename = "entityName", default)]
    pub entity_name: String,
    #[serde(default)]
    pub tickers: Vec<String>,
    #[serde(default)]
    pub exchanges: Vec<String>,
    #[serde(rename = "fiscalYearEnd", default)]
    pub fiscal_year_end: String,
    pub filings: SubmissionsFilings,
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
}
