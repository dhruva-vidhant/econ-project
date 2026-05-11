use serde::{Deserialize, Serialize};
use std::fmt;

/// SEC Central Index Key, 10-digit zero-padded.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Cik(pub String);

impl Cik {
    /// Normalize a numeric CIK or already-padded string into the canonical
    /// 10-digit zero-padded form.
    pub fn from_any(input: impl AsRef<str>) -> Result<Self, String> {
        let s = input.as_ref().trim();
        let digits: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
        if digits.is_empty() || digits.len() > 10 {
            return Err(format!("invalid CIK: {s:?}"));
        }
        Ok(Cik(format!("{:0>10}", digits)))
    }
    pub fn as_str(&self) -> &str { &self.0 }
}

impl fmt::Display for Cik {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}", self.0) }
}

/// Stock ticker symbol, uppercased.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Ticker(pub String);

impl Ticker {
    pub fn from_str(s: &str) -> Self { Ticker(s.trim().to_uppercase()) }
    pub fn as_str(&self) -> &str { &self.0 }
}

impl fmt::Display for Ticker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}", self.0) }
}

/// SEC accession number, e.g. "0000320193-24-000123".
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AccessionNo(pub String);

impl AccessionNo {
    pub fn as_str(&self) -> &str { &self.0 }
    /// Strip dashes for the URL form used in SEC archives:
    /// "0000320193-24-000123" → "000032019324000123".
    pub fn stripped(&self) -> String { self.0.replace('-', "") }
}

impl fmt::Display for AccessionNo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}", self.0) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cik_pads_short() {
        assert_eq!(Cik::from_any("320193").unwrap().as_str(), "0000320193");
    }

    #[test]
    fn cik_accepts_padded() {
        assert_eq!(Cik::from_any("0000320193").unwrap().as_str(), "0000320193");
    }

    #[test]
    fn cik_rejects_empty() {
        assert!(Cik::from_any("").is_err());
        assert!(Cik::from_any("abc").is_err());
    }

    #[test]
    fn ticker_uppercases() {
        assert_eq!(Ticker::from_str("aapl").as_str(), "AAPL");
    }

    #[test]
    fn accession_no_stripped() {
        let a = AccessionNo("0000320193-24-000123".into());
        assert_eq!(a.stripped(), "000032019324000123");
    }
}
