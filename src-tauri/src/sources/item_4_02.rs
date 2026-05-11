//! 8-K Item 4.02 disclosure parser.
//!
//! Identifies the specific fiscal periods the company is flagging as
//! unreliable. Per the project's accuracy contract, this must work
//! regardless of wording or whether structured tags are present.
//!
//! Strategy:
//! 1. Strip HTML to plain text using `scraper`.
//! 2. Extract Item 4.02 prose specifically (between "Item 4.02" header
//!    and the next "Item N.NN" header or the signature block).
//! 3. Apply a panel of regex patterns covering the conventional phrasings
//!    used in Item 4.02 disclosures, capturing fiscal-year and quarter-end
//!    dates explicitly. Patterns are deliberately overlapping; we union
//!    the results and dedupe.
//! 4. Resolve each captured date to a fiscal year (and optionally quarter)
//!    using calendar arithmetic + the company's fiscal-year-end pattern.

use chrono::NaiveDate;
use regex::Regex;
use scraper::{Html, Selector};

use crate::errors::SourceError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AffectedPeriod {
    /// "Year ended", "Quarter ended", or generic "Period ended".
    pub period_kind_hint: PeriodKindHint,
    pub end_date: NaiveDate,
}

#[derive(Clone, Debug, PartialEq, Eq, Copy)]
pub enum PeriodKindHint {
    Annual,
    Quarterly,
    Either,
}

/// Parse Item 4.02 affected periods from an 8-K's primary document HTML.
pub fn extract_affected_periods(html: &str) -> Result<Vec<AffectedPeriod>, SourceError> {
    let plain = html_to_plain_text(html);
    let item_text = isolate_item_4_02_text(&plain);
    let mut found = extract_periods_from_text(&item_text);
    if found.is_empty() {
        // Fall back to scanning the whole document — some filings have
        // unusual section layouts. We still require an Item 4.02 marker
        // somewhere in the document to ensure we're parsing the right form.
        if mentions_item_4_02(&plain) {
            found = extract_periods_from_text(&plain);
        }
    }
    if found.is_empty() {
        return Err(SourceError::SchemaMismatch {
            url: "(8-K primary doc)".into(),
            detail: "could not identify any affected fiscal period in Item 4.02 prose".into(),
        });
    }
    dedupe(&mut found);
    Ok(found)
}

fn html_to_plain_text(html: &str) -> String {
    let doc = Html::parse_document(html);
    let body_sel = Selector::parse("body").unwrap();
    let mut out = String::with_capacity(html.len() / 4);
    if let Some(body) = doc.select(&body_sel).next() {
        for text in body.text() {
            // Collapse whitespace as we go; HTML's text() yields fragments.
            for ch in text.chars() {
                if ch.is_whitespace() {
                    if !out.ends_with(' ') { out.push(' '); }
                } else {
                    out.push(ch);
                }
            }
        }
    } else {
        // No <body>? Fall back to all text.
        for text in doc.root_element().text() {
            for ch in text.chars() {
                if ch.is_whitespace() {
                    if !out.ends_with(' ') { out.push(' '); }
                } else {
                    out.push(ch);
                }
            }
        }
    }
    out
}

fn mentions_item_4_02(text: &str) -> bool {
    text.to_lowercase().contains("item 4.02")
        || text.to_lowercase().contains("non-reliance")
        || text.to_lowercase().contains("non reliance")
}

/// Try to isolate the Item 4.02 section. Looks for "Item 4.02" up to the
/// next "Item N.NN" header (or end of document). Falls back to the whole
/// text if the header is absent (caller decides whether to use it).
fn isolate_item_4_02_text(text: &str) -> String {
    let lower = text.to_lowercase();
    let start_re = Regex::new(r"item\s*4\.02").unwrap();
    let next_re = Regex::new(r"item\s*\d+\.\d+").unwrap();
    let start = match start_re.find(&lower) {
        Some(m) => m.start(),
        None => return String::new(),
    };
    // Find the next "Item X.YY" after our start.
    let after = &lower[start + 1..];
    let end_off = next_re.find(after).map(|m| start + 1 + m.start()).unwrap_or(text.len());
    text[start..end_off].to_string()
}

fn extract_periods_from_text(text: &str) -> Vec<AffectedPeriod> {
    let mut out: Vec<AffectedPeriod> = Vec::new();

    // Patterns to capture (case-insensitive). The (?i) flag makes the regex case-insensitive.
    // 1) "fiscal year ended [Month] [Day], [Year]" → annual
    // 2) "year ended [Month] [Day], [Year]" → annual
    // 3) "[Month] [Day], [Year] fiscal year" → annual (looser)
    // 4) "fiscal year [Year]" or "year [Year]" → annual (year-only)
    // 5) "quarter ended [Month] [Day], [Year]" → quarterly
    // 6) "quarterly period ended [Month] [Day], [Year]" → quarterly
    // 7) "three months ended [Month] [Day], [Year]" → quarterly
    // 8) "six months ended [Month] [Day], [Year]" → quarterly (H1)
    // 9) "nine months ended [Month] [Day], [Year]" → quarterly (9M)
    let annual_full_date = Regex::new(r"(?i)(?:fiscal\s+year|year)\s+ended\s+([A-Za-z]+)\s+(\d{1,2}),?\s*(\d{4})").unwrap();
    let quarter_full_date = Regex::new(r"(?i)(?:quarter|quarterly\s+period|three\s+months|six\s+months|nine\s+months)\s+ended\s+([A-Za-z]+)\s+(\d{1,2}),?\s*(\d{4})").unwrap();
    let year_only = Regex::new(r"(?i)(?:fiscal\s+year|year)\s+(\d{4})").unwrap();
    let date_then_label = Regex::new(r"(?i)([A-Za-z]+)\s+(\d{1,2}),?\s*(\d{4}).{0,80}?(fiscal\s+year|annual)").unwrap();

    for cap in annual_full_date.captures_iter(text) {
        if let Some(d) = parse_md_y(&cap[1], &cap[2], &cap[3]) {
            out.push(AffectedPeriod { period_kind_hint: PeriodKindHint::Annual, end_date: d });
        }
    }
    for cap in quarter_full_date.captures_iter(text) {
        if let Some(d) = parse_md_y(&cap[1], &cap[2], &cap[3]) {
            out.push(AffectedPeriod { period_kind_hint: PeriodKindHint::Quarterly, end_date: d });
        }
    }
    for cap in date_then_label.captures_iter(text) {
        if let Some(d) = parse_md_y(&cap[1], &cap[2], &cap[3]) {
            out.push(AffectedPeriod { period_kind_hint: PeriodKindHint::Annual, end_date: d });
        }
    }
    // Year-only is intentionally weaker; only used when no full-date match was found.
    if out.is_empty() {
        for cap in year_only.captures_iter(text) {
            if let Ok(y) = cap[1].parse::<i32>() {
                // Year-only references are too imprecise for V1's accuracy bar; skip rather
                // than over-flag. The parser will surface SchemaMismatch and the orchestrator
                // logs a user-visible diagnostic.
                let _ = y;
            }
        }
    }
    out
}

fn parse_md_y(month: &str, day: &str, year: &str) -> Option<NaiveDate> {
    let m = month_name_to_num(month)?;
    let d: u32 = day.parse().ok()?;
    let y: i32 = year.parse().ok()?;
    NaiveDate::from_ymd_opt(y, m, d)
}

fn month_name_to_num(s: &str) -> Option<u32> {
    match s.to_lowercase().as_str() {
        "january" | "jan" => Some(1),
        "february" | "feb" => Some(2),
        "march" | "mar" => Some(3),
        "april" | "apr" => Some(4),
        "may" => Some(5),
        "june" | "jun" => Some(6),
        "july" | "jul" => Some(7),
        "august" | "aug" => Some(8),
        "september" | "sep" | "sept" => Some(9),
        "october" | "oct" => Some(10),
        "november" | "nov" => Some(11),
        "december" | "dec" => Some(12),
        _ => None,
    }
}

fn dedupe(v: &mut Vec<AffectedPeriod>) {
    v.sort_by(|a, b| a.end_date.cmp(&b.end_date).then((a.period_kind_hint as u8).cmp(&(b.period_kind_hint as u8))));
    v.dedup();
}

/// Build the URL for an 8-K's primary document.
pub fn primary_doc_url(cik: &str, accession_no: &str, primary_doc: &str) -> String {
    let cik_num: u64 = cik.parse().unwrap_or(0);
    let stripped = accession_no.replace('-', "");
    format!(
        "https://www.sec.gov/Archives/edgar/data/{cik_num}/{stripped}/{primary_doc}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_year_ended_date() {
        let html = r#"
        <html><body>
        <h2>Item 4.02 Non-Reliance on Previously Issued Financial Statements</h2>
        <p>The Audit Committee concluded on January 15, 2024 that the Company's
        previously issued financial statements for the year ended December 31, 2022
        should no longer be relied upon.</p>
        <h2>Item 9.01 Financial Statements and Exhibits</h2>
        </body></html>"#;
        let v = extract_affected_periods(html).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].period_kind_hint, PeriodKindHint::Annual);
        assert_eq!(v[0].end_date, NaiveDate::from_ymd_opt(2022, 12, 31).unwrap());
    }

    #[test]
    fn parses_quarter_ended_date() {
        let html = r#"
        <html><body>
        <p>Item 4.02 Non-Reliance on Previously Issued Financial Statements.
        Management has concluded that the financial statements as of and for the
        quarter ended September 30, 2023 should not be relied upon.</p>
        </body></html>"#;
        let v = extract_affected_periods(html).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].period_kind_hint, PeriodKindHint::Quarterly);
        assert_eq!(v[0].end_date, NaiveDate::from_ymd_opt(2023, 9, 30).unwrap());
    }

    #[test]
    fn parses_multiple_periods() {
        let html = r#"
        <html><body>
        <p>Item 4.02. The Company's financial statements for the year ended
        December 31, 2021 and the year ended December 31, 2022 should not be
        relied upon. The quarterly period ended March 31, 2023 is also affected.</p>
        </body></html>"#;
        let v = extract_affected_periods(html).unwrap();
        assert!(v.len() >= 2);
        // Should contain the 2022 annual and 2023 Q1 dates at minimum.
        assert!(v.iter().any(|p| p.end_date == NaiveDate::from_ymd_opt(2022, 12, 31).unwrap()
            && p.period_kind_hint == PeriodKindHint::Annual));
        assert!(v.iter().any(|p| p.end_date == NaiveDate::from_ymd_opt(2023, 3, 31).unwrap()
            && p.period_kind_hint == PeriodKindHint::Quarterly));
    }

    #[test]
    fn strict_failure_when_no_period_found() {
        let html = "<html><body><p>Item 4.02 Some discussion that doesn't name a period.</p></body></html>";
        let r = extract_affected_periods(html);
        assert!(r.is_err());
    }

    #[test]
    fn primary_doc_url_format() {
        let url = primary_doc_url("0000320193", "0000320193-24-000123", "aapl-20240115.htm");
        assert_eq!(url, "https://www.sec.gov/Archives/edgar/data/320193/000032019324000123/aapl-20240115.htm");
    }
}
