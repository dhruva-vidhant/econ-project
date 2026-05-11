use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use super::ids::{AccessionNo, Cik};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FormType {
    #[serde(rename = "10-K")] TenK,
    #[serde(rename = "10-Q")] TenQ,
    #[serde(rename = "10-K/A")] TenKA,
    #[serde(rename = "10-Q/A")] TenQA,
    #[serde(rename = "8-K")] EightK,
    #[serde(rename = "other")] Other(String),
}

impl FormType {
    pub fn from_str(s: &str) -> FormType {
        match s {
            "10-K" => FormType::TenK,
            "10-Q" => FormType::TenQ,
            "10-K/A" => FormType::TenKA,
            "10-Q/A" => FormType::TenQA,
            "8-K" => FormType::EightK,
            other => FormType::Other(other.to_string()),
        }
    }
    pub fn as_str(&self) -> &str {
        match self {
            FormType::TenK => "10-K",
            FormType::TenQ => "10-Q",
            FormType::TenKA => "10-K/A",
            FormType::TenQA => "10-Q/A",
            FormType::EightK => "8-K",
            FormType::Other(s) => s.as_str(),
        }
    }
    pub fn is_amendment(&self) -> bool {
        matches!(self, FormType::TenKA | FormType::TenQA)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Filing {
    pub accession_no: AccessionNo,
    pub cik: Cik,
    pub form_type: FormType,
    pub filed_at: NaiveDate,
    pub period_of_report: Option<NaiveDate>,
    pub is_amendment: bool,
    pub amends: Option<AccessionNo>,
    pub item_4_02_8k: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_type_round_trip() {
        for s in ["10-K", "10-Q", "10-K/A", "10-Q/A", "8-K"] {
            let ft = FormType::from_str(s);
            assert_eq!(ft.as_str(), s);
        }
    }

    #[test]
    fn form_type_is_amendment() {
        assert!(FormType::TenKA.is_amendment());
        assert!(FormType::TenQA.is_amendment());
        assert!(!FormType::TenK.is_amendment());
        assert!(!FormType::EightK.is_amendment());
    }

    #[test]
    fn filing_serde() {
        let f = Filing {
            accession_no: AccessionNo("0000320193-24-000123".into()),
            cik: Cik("0000320193".into()),
            form_type: FormType::TenK,
            filed_at: NaiveDate::from_ymd_opt(2024, 11, 1).unwrap(),
            period_of_report: NaiveDate::from_ymd_opt(2024, 9, 28),
            is_amendment: false,
            amends: None,
            item_4_02_8k: false,
        };
        let j = serde_json::to_string(&f).unwrap();
        let f2: Filing = serde_json::from_str(&j).unwrap();
        assert_eq!(f, f2);
    }
}
