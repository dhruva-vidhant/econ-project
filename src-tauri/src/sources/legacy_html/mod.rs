//! Shared retrieval and parsing framework for configured pre-XBRL SEC filings.

use chrono::{Datelike, NaiveDate, Utc};
use once_cell::sync::Lazy;
use regex::Regex;
use scraper::{Html, Selector};

use crate::domain::{AccessionNo, Cik, Filing, FormType, RawFact, SourceKind, MICRO};
use crate::errors::SourceError;

use super::sec_client::SecClient;
use super::submissions::{self, HistoricalSubmissionsFile, SubmissionsRecent};

mod bbw;

type ParseFn = fn(&Cik, &Filing, &str, &str) -> Result<Vec<RawFact>, SourceError>;

pub(super) struct IssuerConfig {
    cik: &'static str,
    historical_anchor: &'static str,
    filings: &'static [FilingConfig],
    parse: ParseFn,
}

pub(super) struct FilingConfig {
    accession: &'static str,
    form_type: FormType,
}

impl FilingConfig {
    pub(super) const fn new(accession: &'static str, form_type: FormType) -> Self {
        Self {
            accession,
            form_type,
        }
    }
}

static REGISTRY: &[&IssuerConfig] = &[&bbw::CONFIG];

static AMOUNT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?:\(\s*)?-?(?:\d{1,3}(?:,\d{3})+|\d+\.\d+|\d+)(?:\s*\))?").unwrap());

/// Fetch configured pre-XBRL filings for an issuer. Unconfigured issuers are unaffected.
pub async fn fetch_configured_history(
    client: &SecClient,
    cik: &Cik,
    files: &[HistoricalSubmissionsFile],
) -> Result<Vec<(Filing, Vec<RawFact>)>, SourceError> {
    let Some(config) = configured_issuer(cik) else {
        return Ok(Vec::new());
    };
    let file = files
        .iter()
        .find(|file| {
            file.filing_from.as_str() <= config.historical_anchor
                && file.filing_to.as_str() >= config.historical_anchor
        })
        .ok_or_else(|| {
            mismatch("historical submissions index does not cover configured filings")
        })?;
    let historical = submissions::fetch_historical(client, file).await?;
    let mut out = Vec::with_capacity(config.filings.len());
    for expected in config.filings {
        let index = historical
            .accession_number
            .iter()
            .position(|accession| accession == expected.accession)
            .ok_or_else(|| {
                mismatch("a required legacy filing is absent from historical submissions")
            })?;
        let filing = filing_at(cik, &historical, index, &expected.form_type)?;
        let primary_document = historical
            .primary_document
            .get(index)
            .filter(|document| !document.is_empty())
            .ok_or_else(|| mismatch("a required legacy filing has no primary document"))?;
        let url = primary_document_url(cik, &filing.accession_no, primary_document);
        let bytes = client.get_bytes(&url).await?;
        let html = String::from_utf8_lossy(&bytes);
        let facts = (config.parse)(cik, &filing, &html, &url)?;
        out.push((filing, facts));
    }
    Ok(out)
}

fn configured_issuer(cik: &Cik) -> Option<&'static IssuerConfig> {
    REGISTRY.iter().copied().find(|config| config.cik == cik.0)
}

fn filing_at(
    cik: &Cik,
    recent: &SubmissionsRecent,
    index: usize,
    expected_form: &FormType,
) -> Result<Filing, SourceError> {
    let accession = recent
        .accession_number
        .get(index)
        .ok_or_else(|| mismatch("historical filing has no accession number"))?;
    let filed_at = parse_date(recent.filing_date.get(index), "filing date")?;
    let period_of_report = parse_date(recent.report_date.get(index), "report date")?;
    let form_type = FormType::from_str(recent.form.get(index).map(String::as_str).unwrap_or(""));
    if &form_type != expected_form {
        return Err(mismatch("a required legacy filing has an unexpected form"));
    }
    Ok(Filing {
        accession_no: AccessionNo(accession.clone()),
        cik: cik.clone(),
        form_type,
        filed_at,
        period_of_report: Some(period_of_report),
        is_amendment: false,
        amends: None,
        item_4_02_8k: false,
    })
}

fn parse_date(value: Option<&String>, field: &str) -> Result<NaiveDate, SourceError> {
    value
        .and_then(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
        .ok_or_else(|| mismatch(&format!("legacy filing has an invalid {field}")))
}

fn primary_document_url(cik: &Cik, accession: &AccessionNo, document: &str) -> String {
    let cik_number = cik.0.parse::<u64>().unwrap_or(0);
    format!(
        "https://www.sec.gov/Archives/edgar/data/{}/{}/{}",
        cik_number,
        accession.0.replace('-', ""),
        document
    )
}

pub(super) fn tuple_date(parts: (i32, u32, u32)) -> NaiveDate {
    NaiveDate::from_ymd_opt(parts.0, parts.1, parts.2).unwrap()
}

pub(super) fn first_amounts_for_labels(
    table: &scraper::ElementRef<'_>,
    labels: &[&str],
    boundary_labels: &[&str],
) -> Option<Vec<ParsedAmount>> {
    labels
        .iter()
        .find_map(|label| amounts_after_label(table, label, boundary_labels))
}

pub(super) fn amounts_after_label(
    table: &scraper::ElementRef<'_>,
    label: &str,
    boundary_labels: &[&str],
) -> Option<Vec<ParsedAmount>> {
    let text = normalized_text(*table);
    let start = text.find(label)? + label.len();
    let after_label = &text[start..];
    let requested_end = boundary_labels
        .iter()
        .filter_map(|boundary| after_label.find(boundary))
        .min()
        .unwrap_or(after_label.len().min(300));
    let mut end = requested_end.min(300);
    while !after_label.is_char_boundary(end) {
        end -= 1;
    }
    let values = AMOUNT_RE
        .find_iter(&after_label[..end])
        .filter_map(|value| ParsedAmount::parse(value.as_str()))
        .collect::<Vec<_>>();
    (!values.is_empty()).then_some(values)
}

pub(super) fn find_table<'a>(
    document: &'a Html,
    selector: &Selector,
    first: &str,
    second: &str,
) -> Option<scraper::ElementRef<'a>> {
    document.select(selector).find(|table| {
        let text = normalized_text(*table);
        text.contains(first) && text.contains(second)
    })
}

pub(super) fn first_value(
    table: &scraper::ElementRef<'_>,
    label: &str,
    occurrence: usize,
) -> Option<ParsedAmount> {
    let text = normalized_text(*table);
    text.match_indices(label)
        .filter_map(|(start, _)| {
            let after_label = &text[start + label.len()..];
            AMOUNT_RE
                .find(after_label.get(..after_label.len().min(160))?)
                .and_then(|value| ParsedAmount::parse(value.as_str()))
        })
        .nth(occurrence)
}

pub(super) fn normalized_text(element: scraper::ElementRef<'_>) -> String {
    element
        .text()
        .flat_map(str::split_whitespace)
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[derive(Clone, Copy)]
pub(super) struct ParsedAmount {
    negative: bool,
    value: f64,
}

impl ParsedAmount {
    pub(super) fn parse(input: &str) -> Option<Self> {
        let negative = input.trim_start().starts_with('(') || input.trim_start().starts_with('-');
        let value = input
            .replace(['(', ')', '-', ' '], "")
            .replace(',', "")
            .parse()
            .ok()?;
        Some(Self { negative, value })
    }
}

#[derive(Clone, Copy)]
pub(super) enum Scale {
    Thousands,
}

impl Scale {
    pub(super) fn apply(self, amount: ParsedAmount) -> Result<i64, SourceError> {
        let multiplier = match self {
            Scale::Thousands => 1_000_f64 * MICRO as f64,
        };
        let signed = if amount.negative {
            -amount.value
        } else {
            amount.value
        };
        let scaled = signed * multiplier;
        if !scaled.is_finite() || scaled < i64::MIN as f64 || scaled > i64::MAX as f64 {
            return Err(mismatch("legacy HTML value overflowed micro-unit storage"));
        }
        Ok(scaled.round() as i64)
    }
}

pub(super) fn raw_fact(
    cik: &Cik,
    filing: &Filing,
    concept: &str,
    unit: &str,
    value_numeric: i64,
    period_start: Option<NaiveDate>,
    period_end: NaiveDate,
    fp: &str,
) -> RawFact {
    RawFact {
        id: 0,
        cik: cik.clone(),
        accession_no: filing.accession_no.clone(),
        taxonomy: "us-gaap".into(),
        concept: concept.into(),
        unit: unit.into(),
        value_numeric,
        period_start,
        period_end,
        is_instant: period_start.is_none(),
        fy: Some(period_end.year()),
        fp: Some(fp.into()),
        filed: Some(filing.filed_at),
        source_kind: SourceKind::SecFilingHtml,
        ingested_at: Utc::now(),
    }
}

fn mismatch(detail: &str) -> SourceError {
    mismatch_url("historical SEC submissions", detail)
}

pub(super) fn mismatch_url(url: &str, detail: &str) -> SourceError {
    SourceError::SchemaMismatch {
        url: url.into(),
        detail: detail.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_matches_only_configured_cik() {
        assert!(configured_issuer(&Cik("0001113809".into())).is_some());
        assert!(configured_issuer(&Cik("0000000000".into())).is_none());
    }

    #[test]
    fn archive_url_uses_unpadded_cik_and_stripped_accession() {
        assert_eq!(
            primary_document_url(
                &Cik("0001113809".into()),
                &AccessionNo("0000950137-05-003677".into()),
                "c93387e10vk.htm"
            ),
            "https://www.sec.gov/Archives/edgar/data/1113809/000095013705003677/c93387e10vk.htm"
        );
    }

    #[test]
    fn parses_parenthesized_amount_and_scales_thousands() {
        let amount = ParsedAmount::parse("(16,494)").unwrap();
        assert_eq!(Scale::Thousands.apply(amount).unwrap(), -16_494_000_000_000);
    }

    #[test]
    fn normalizes_html_text_and_extracts_labeled_amounts() {
        let document = Html::parse_fragment(
            "<table><tr><td>Total Revenues</td><td>(1,234)</td><td>567</td></tr></table>",
        );
        let selector = Selector::parse("table").unwrap();
        let table = document.select(&selector).next().unwrap();
        assert_eq!(normalized_text(table), "total revenues (1,234) 567");
        let values = amounts_after_label(&table, "total revenues", &[]).unwrap();
        assert_eq!(
            Scale::Thousands.apply(values[0]).unwrap(),
            -1_234_000_000_000
        );
        assert_eq!(Scale::Thousands.apply(values[1]).unwrap(), 567_000_000_000);
    }
}
