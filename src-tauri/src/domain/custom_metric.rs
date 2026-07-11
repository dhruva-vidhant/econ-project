//! Custom-metric domain types — V2 (custom metrics + CSV export).
//!
//! User-supplied, per-company KPIs. Live in a store separate from the
//! authoritative `Metric` catalog and `normalized_fact` chain (design doc D1);
//! never enter normalization and carry no filing lineage.

use serde::{Deserialize, Serialize};

use super::{Cik, Period};

/// Unit interpretation for a custom metric's stored micro-value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitKind {
    Currency,
    Count,
    Ratio,
    Number,
}

impl UnitKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            UnitKind::Currency => "currency",
            UnitKind::Count => "count",
            UnitKind::Ratio => "ratio",
            UnitKind::Number => "number",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "currency" => UnitKind::Currency,
            "count" => UnitKind::Count,
            "ratio" => UnitKind::Ratio,
            "number" => UnitKind::Number,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CustomMetric {
    pub id: i64,
    pub cik: Cik,
    pub key: String,
    pub label: String,
    pub unit_kind: UnitKind,
    pub currency: Option<String>,
    pub decimals: u8,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CustomMetricPoint {
    pub id: i64,
    pub custom_metric_id: i64,
    pub period_id: Option<i64>,
    pub observed_date: Option<String>,
    pub value_micro: i64,
    pub source_note: Option<String>,
    pub imported_at: String,
}

/// One point in a custom metric's period-aligned series, mirroring
/// `derived::series::MetricSeriesPoint` for the built-in catalog.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CustomSeriesPoint {
    pub period: Period,
    pub value_micro: i64,
    pub unit_kind: UnitKind,
    pub point_id: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NewCustomMetric {
    pub key: String,
    pub label: String,
    pub unit_kind: UnitKind,
    pub currency: Option<String>,
    pub decimals: Option<u8>,
    pub description: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CustomMetricPatch {
    pub label: Option<String>,
    pub unit_kind: Option<UnitKind>,
    pub currency: Option<String>,
    pub decimals: Option<u8>,
    pub description: Option<String>,
}

/// Error parsing a user-supplied decimal string into micro-units.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseValueError(pub String);

impl std::fmt::Display for ParseValueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid value: {}", self.0)
    }
}
impl std::error::Error for ParseValueError {}

/// Parse a user decimal string into micro-units (×1e6), exact for typical
/// inputs. Rejects non-finite / non-numeric. Accepts a leading '+/-'; commas
/// (thousands separators) are stripped first.
///
/// Parsed as integer-and-fraction digit strings rather than through `f64`, so
/// arbitrarily large values (beyond `2^53`) remain exact (D4 precision
/// guarantee).
pub fn parse_micro(input: &str) -> Result<i64, ParseValueError> {
    let cleaned: String = input.trim().chars().filter(|&c| c != ',').collect();
    if cleaned.is_empty() {
        return Err(ParseValueError(input.to_string()));
    }
    let (neg, rest) = match cleaned.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, cleaned.strip_prefix('+').unwrap_or(&cleaned)),
    };
    if rest.is_empty() || !rest.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return Err(ParseValueError(input.to_string()));
    }
    let mut parts = rest.splitn(2, '.');
    let int_part = parts.next().unwrap_or("");
    let frac_part = parts.next().unwrap_or("");
    if int_part.is_empty() && frac_part.is_empty() {
        return Err(ParseValueError(input.to_string()));
    }
    if int_part.is_empty() && frac_part.chars().count() == 0 {
        return Err(ParseValueError(input.to_string()));
    }
    if !int_part.chars().all(|c| c.is_ascii_digit()) {
        return Err(ParseValueError(input.to_string()));
    }
    if !frac_part.chars().all(|c| c.is_ascii_digit()) {
        return Err(ParseValueError(input.to_string()));
    }
    if rest.matches('.').count() > 1 {
        return Err(ParseValueError(input.to_string()));
    }

    let int_val: i64 = if int_part.is_empty() { 0 } else {
        int_part.parse().map_err(|_| ParseValueError(input.to_string()))?
    };
    // Pad/truncate the fractional part to exactly 6 digits (micro precision).
    let mut frac6 = frac_part.to_string();
    frac6.truncate(6);
    while frac6.len() < 6 {
        frac6.push('0');
    }
    let frac_val: i64 = if frac6.is_empty() { 0 } else {
        frac6.parse().map_err(|_| ParseValueError(input.to_string()))?
    };

    let magnitude = int_val
        .checked_mul(1_000_000)
        .and_then(|v| v.checked_add(frac_val))
        .ok_or_else(|| ParseValueError(input.to_string()))?;
    Ok(if neg { -magnitude } else { magnitude })
}

/// Format an i64 micro value to an exact decimal string with `min_frac`..6
/// fractional digits (trailing zeros trimmed down to `min_frac`). Exact for
/// all i64 — never routed through f64 (D4 precision guarantee).
pub fn micro_to_decimal_string(v: i64, min_frac: u8) -> String {
    let min_frac = min_frac.min(6);
    let neg = v < 0;
    let abs = (v as i128).unsigned_abs();
    let int_part = abs / 1_000_000;
    let frac_part = abs % 1_000_000;
    let mut frac_str = format!("{frac_part:06}");
    // Trim trailing zeros down to min_frac digits.
    while frac_str.len() > min_frac as usize && frac_str.ends_with('0') {
        frac_str.pop();
    }
    let sign = if neg && (int_part != 0 || frac_part != 0) { "-" } else { "" };
    if frac_str.is_empty() {
        format!("{sign}{int_part}")
    } else {
        format!("{sign}{int_part}.{frac_str}")
    }
}

/// Format an i64 ratio-micro value (ratio × 1e6) as an exact percent string.
/// e.g. `41_234` → `"4.1234"`, `253_000` → `"25.3"`.
pub fn ratio_micro_to_percent_string(v: i64) -> String {
    // percent = ratio * 100, i.e. (v / 1e6) * 100 = v / 1e4. Shift the
    // micro-decimal-point by dividing the *scale*, not the value, to stay exact:
    // treat v as (percent × 1e4) and format with 4 decimal digits, trimmed.
    let neg = v < 0;
    let abs = (v as i128).unsigned_abs();
    let int_part = abs / 10_000;
    let frac_part = abs % 10_000;
    let mut frac_str = format!("{frac_part:04}");
    while frac_str.ends_with('0') && !frac_str.is_empty() {
        frac_str.pop();
    }
    let sign = if neg && (int_part != 0 || frac_part != 0) { "-" } else { "" };
    if frac_str.is_empty() {
        format!("{sign}{int_part}")
    } else {
        format!("{sign}{int_part}.{frac_str}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_micro_integer() {
        assert_eq!(parse_micro("25000000000").unwrap(), 25_000_000_000_000_000);
    }

    #[test]
    fn parse_micro_decimal() {
        assert_eq!(parse_micro("25.3").unwrap(), 25_300_000);
    }

    #[test]
    fn parse_micro_strips_commas() {
        assert_eq!(parse_micro("1,680,000,000").unwrap(), 1_680_000_000_000_000);
    }

    #[test]
    fn parse_micro_rejects_text() {
        assert!(parse_micro("n/a").is_err());
    }

    #[test]
    fn parse_micro_accepts_leading_sign() {
        assert_eq!(parse_micro("-25.3").unwrap(), -25_300_000);
        assert_eq!(parse_micro("+25.3").unwrap(), 25_300_000);
    }

    #[test]
    fn micro_to_decimal_2dp() {
        assert_eq!(
            micro_to_decimal_string(383_285_000_000_000_000, 2),
            "383285000000.00"
        );
    }

    #[test]
    fn micro_to_decimal_exact_beyond_2p53() {
        // 9_007_199_254_740_993_000_000 exceeds 2^53 — exercised as an i128
        // intermediate; but our storage is i64, so use a large-but-valid i64
        // micro value near i64::MAX and confirm exact digit reproduction
        // (no f64 rounding).
        let v: i64 = 9_007_199_254_740_993; // > 2^53
        assert_eq!(micro_to_decimal_string(v, 0), "9007199254.740993");
    }

    #[test]
    fn ratio_micro_to_percent() {
        assert_eq!(ratio_micro_to_percent_string(41_234), "4.1234");
    }

    #[test]
    fn ratio_micro_to_percent_trim() {
        assert_eq!(ratio_micro_to_percent_string(253_000), "25.3");
    }

    #[test]
    fn unitkind_round_trip() {
        for u in [UnitKind::Currency, UnitKind::Count, UnitKind::Ratio, UnitKind::Number] {
            assert_eq!(UnitKind::from_str(u.as_str()), Some(u));
        }
    }
}
