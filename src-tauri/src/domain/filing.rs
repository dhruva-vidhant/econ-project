use chrono::NaiveDate;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::ids::{AccessionNo, Cik};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormType {
    TenK,
    TenQ,
    TenKA,
    TenQA,
    EightK,
    /// Catch-all for forms outside the canonical V1 set (e.g., "POS AM",
    /// "SD"). Carries the SEC's literal form-type string.
    Other(String),
}

// Serialize/deserialize FormType as a flat string so the IPC wire format
// is uniform. The default derive would emit `Other("SD")` as
// `{"other": "SD"}`, which the React UI cannot render directly (React
// error #31: "Objects are not valid as a React child").
impl Serialize for FormType {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for FormType {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(FormType::from_str(&s))
    }
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
    fn form_type_serde_is_flat_string() {
        // All variants — including Other(_) — must serialize as a bare
        // JSON string so the React UI can render them directly.
        for (variant, json) in [
            (FormType::TenK, "\"10-K\""),
            (FormType::TenQ, "\"10-Q\""),
            (FormType::TenKA, "\"10-K/A\""),
            (FormType::TenQA, "\"10-Q/A\""),
            (FormType::EightK, "\"8-K\""),
            (FormType::Other("POS AM".into()), "\"POS AM\""),
        ] {
            assert_eq!(serde_json::to_string(&variant).unwrap(), json);
            let round: FormType = serde_json::from_str(json).unwrap();
            assert_eq!(round, variant);
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
