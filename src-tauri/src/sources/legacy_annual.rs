//! Dynamic retrieval of BBW's audited pre-XBRL SEC filing history.

use chrono::{Datelike, NaiveDate, Utc};
use once_cell::sync::Lazy;
use regex::Regex;
use scraper::{Html, Selector};

use crate::domain::{AccessionNo, Cik, Filing, FormType, RawFact, SourceKind, MICRO};
use crate::errors::SourceError;

use super::sec_client::SecClient;
use super::submissions::{self, HistoricalSubmissionsFile, SubmissionsRecent};

const BBW_CIK: &str = "0001113809";

struct LegacyAnnual {
    accession: &'static str,
    start: (i32, u32, u32),
    end: (i32, u32, u32),
}

struct LegacyQuarter {
    accession: &'static str,
    fp: &'static str,
    fiscal_start: (i32, u32, u32),
    quarter_start: (i32, u32, u32),
    end: (i32, u32, u32),
    single_value_index: usize,
    ytd_value_index: Option<usize>,
    instant_value_index: usize,
}

const BBW_LEGACY_ANNUALS: &[LegacyAnnual] = &[
    LegacyAnnual {
        accession: "0000950137-05-003677",
        start: (2004, 1, 4),
        end: (2005, 1, 1),
    },
    LegacyAnnual {
        accession: "0000950137-06-003138",
        start: (2005, 1, 2),
        end: (2005, 12, 31),
    },
    LegacyAnnual {
        accession: "0000950137-07-003883",
        start: (2006, 1, 1),
        end: (2006, 12, 30),
    },
    LegacyAnnual {
        accession: "0001193125-08-055664",
        start: (2006, 12, 31),
        end: (2007, 12, 29),
    },
    LegacyAnnual {
        accession: "0001193125-09-058733",
        start: (2007, 12, 30),
        end: (2009, 1, 3),
    },
];

const BBW_LEGACY_QUARTERS: &[LegacyQuarter] = &[
    LegacyQuarter {
        accession: "0000950137-04-010981",
        fp: "Q3",
        fiscal_start: (2004, 1, 4),
        quarter_start: (2004, 7, 4),
        end: (2004, 10, 2),
        single_value_index: 1,
        ytd_value_index: Some(3),
        instant_value_index: 1,
    },
    LegacyQuarter {
        accession: "0000950137-05-006068",
        fp: "Q1",
        fiscal_start: (2005, 1, 2),
        quarter_start: (2005, 1, 2),
        end: (2005, 4, 2),
        single_value_index: 0,
        ytd_value_index: None,
        instant_value_index: 0,
    },
    LegacyQuarter {
        accession: "0000950137-05-010328",
        fp: "Q2",
        fiscal_start: (2005, 1, 2),
        quarter_start: (2005, 4, 3),
        end: (2005, 7, 2),
        single_value_index: 0,
        ytd_value_index: Some(2),
        instant_value_index: 0,
    },
    LegacyQuarter {
        accession: "0000950137-05-013841",
        fp: "Q3",
        fiscal_start: (2005, 1, 2),
        quarter_start: (2005, 7, 3),
        end: (2005, 10, 1),
        single_value_index: 0,
        ytd_value_index: Some(2),
        instant_value_index: 0,
    },
    LegacyQuarter {
        accession: "0000950137-06-005729",
        fp: "Q1",
        fiscal_start: (2006, 1, 1),
        quarter_start: (2006, 1, 1),
        end: (2006, 4, 1),
        single_value_index: 0,
        ytd_value_index: None,
        instant_value_index: 0,
    },
    LegacyQuarter {
        accession: "0000950137-06-008978",
        fp: "Q2",
        fiscal_start: (2006, 1, 1),
        quarter_start: (2006, 4, 2),
        end: (2006, 7, 1),
        single_value_index: 0,
        ytd_value_index: Some(2),
        instant_value_index: 0,
    },
    LegacyQuarter {
        accession: "0000950137-06-012176",
        fp: "Q3",
        fiscal_start: (2006, 1, 1),
        quarter_start: (2006, 7, 2),
        end: (2006, 9, 30),
        single_value_index: 0,
        ytd_value_index: Some(2),
        instant_value_index: 0,
    },
    LegacyQuarter {
        accession: "0000950137-07-007249",
        fp: "Q1",
        fiscal_start: (2006, 12, 31),
        quarter_start: (2006, 12, 31),
        end: (2007, 3, 31),
        single_value_index: 0,
        ytd_value_index: None,
        instant_value_index: 0,
    },
    LegacyQuarter {
        accession: "0001193125-07-177576",
        fp: "Q2",
        fiscal_start: (2006, 12, 31),
        quarter_start: (2007, 4, 1),
        end: (2007, 6, 30),
        single_value_index: 0,
        ytd_value_index: Some(2),
        instant_value_index: 0,
    },
    LegacyQuarter {
        accession: "0001193125-07-240474",
        fp: "Q3",
        fiscal_start: (2006, 12, 31),
        quarter_start: (2007, 7, 1),
        end: (2007, 9, 29),
        single_value_index: 0,
        ytd_value_index: Some(2),
        instant_value_index: 0,
    },
    LegacyQuarter {
        accession: "0001193125-08-107855",
        fp: "Q1",
        fiscal_start: (2007, 12, 30),
        quarter_start: (2007, 12, 30),
        end: (2008, 3, 29),
        single_value_index: 0,
        ytd_value_index: None,
        instant_value_index: 0,
    },
    LegacyQuarter {
        accession: "0001193125-08-168236",
        fp: "Q2",
        fiscal_start: (2007, 12, 30),
        quarter_start: (2008, 3, 30),
        end: (2008, 6, 28),
        single_value_index: 0,
        ytd_value_index: Some(2),
        instant_value_index: 0,
    },
    LegacyQuarter {
        accession: "0001193125-08-227842",
        fp: "Q3",
        fiscal_start: (2007, 12, 30),
        quarter_start: (2008, 6, 29),
        end: (2008, 9, 27),
        single_value_index: 0,
        ytd_value_index: Some(2),
        instant_value_index: 0,
    },
    LegacyQuarter {
        accession: "0001193125-09-111477",
        fp: "Q1",
        fiscal_start: (2009, 1, 4),
        quarter_start: (2009, 1, 4),
        end: (2009, 4, 4),
        single_value_index: 0,
        ytd_value_index: None,
        instant_value_index: 0,
    },
    LegacyQuarter {
        accession: "0001193125-09-174221",
        fp: "Q2",
        fiscal_start: (2009, 1, 4),
        quarter_start: (2009, 4, 5),
        end: (2009, 7, 4),
        single_value_index: 0,
        ytd_value_index: Some(2),
        instant_value_index: 0,
    },
    LegacyQuarter {
        accession: "0001193125-09-232255",
        fp: "Q3",
        fiscal_start: (2009, 1, 4),
        quarter_start: (2009, 7, 5),
        end: (2009, 10, 3),
        single_value_index: 0,
        ytd_value_index: Some(2),
        instant_value_index: 0,
    },
];

static AMOUNT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?:\(\s*)?-?(?:\d{1,3}(?:,\d{3})+|\d+\.\d+|\d+)(?:\s*\))?").unwrap());

/// Fetch BBW's post-IPO, pre-XBRL annual reports through the SEC metadata
/// advertised by its submissions response. Other issuers are unaffected.
pub async fn fetch_public_history(
    client: &SecClient,
    cik: &Cik,
    files: &[HistoricalSubmissionsFile],
) -> Result<Vec<(Filing, Vec<RawFact>)>, SourceError> {
    if cik.0 != BBW_CIK {
        return Ok(Vec::new());
    }
    let file = files
        .iter()
        .find(|file| {
            file.filing_from.as_str() <= "2005-03-29" && file.filing_to.as_str() >= "2005-03-29"
        })
        .ok_or_else(|| {
            mismatch("historical submissions index no longer covers BBW's first 10-K")
        })?;
    let historical = submissions::fetch_historical(client, file).await?;
    let mut out = Vec::with_capacity(BBW_LEGACY_ANNUALS.len() + BBW_LEGACY_QUARTERS.len());
    for annual in BBW_LEGACY_ANNUALS {
        let index = historical
            .accession_number
            .iter()
            .position(|accession| accession == annual.accession)
            .ok_or_else(|| {
                mismatch("a required BBW legacy 10-K is absent from historical submissions")
            })?;
        let filing = filing_at(cik, &historical, index)?;
        let primary_document = historical
            .primary_document
            .get(index)
            .filter(|document| !document.is_empty())
            .ok_or_else(|| mismatch("a required BBW legacy 10-K has no primary document"))?;
        let url = primary_document_url(cik, &filing.accession_no, primary_document);
        let bytes = client.get_bytes(&url).await?;
        let html = String::from_utf8_lossy(&bytes);
        let facts = parse_bbw_10k(cik, &filing, annual, &html, &url)?;
        out.push((filing, facts));
    }
    for quarter in BBW_LEGACY_QUARTERS {
        let index = historical
            .accession_number
            .iter()
            .position(|accession| accession == quarter.accession)
            .ok_or_else(|| {
                mismatch("a required BBW legacy 10-Q is absent from historical submissions")
            })?;
        let filing = filing_at(cik, &historical, index)?;
        let primary_document = historical
            .primary_document
            .get(index)
            .filter(|document| !document.is_empty())
            .ok_or_else(|| mismatch("a required BBW legacy 10-Q has no primary document"))?;
        let url = primary_document_url(cik, &filing.accession_no, primary_document);
        let bytes = client.get_bytes(&url).await?;
        let html = String::from_utf8_lossy(&bytes);
        let facts = parse_bbw_10q(cik, &filing, quarter, &html, &url)?;
        out.push((filing, facts));
    }
    Ok(out)
}

fn filing_at(cik: &Cik, recent: &SubmissionsRecent, index: usize) -> Result<Filing, SourceError> {
    let accession = recent
        .accession_number
        .get(index)
        .ok_or_else(|| mismatch("historical filing has no accession number"))?;
    let filed_at = parse_date(recent.filing_date.get(index), "filing date")?;
    let period_of_report = parse_date(recent.report_date.get(index), "report date")?;
    let form = recent.form.get(index).map(String::as_str).unwrap_or("");
    let form_type = FormType::from_str(form);
    if !matches!(form_type, FormType::TenK | FormType::TenQ) {
        return Err(mismatch(
            "a required BBW legacy filing has an unexpected form",
        ));
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
        .ok_or_else(|| mismatch(&format!("BBW's first 10-K has an invalid {field}")))
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

fn parse_bbw_10k(
    cik: &Cik,
    filing: &Filing,
    annual: &LegacyAnnual,
    html: &str,
    url: &str,
) -> Result<Vec<RawFact>, SourceError> {
    let document = Html::parse_document(html);
    let table_selector = Selector::parse("table").unwrap();
    let operations = find_table(&document, &table_selector, "total revenues", "net income")
        .ok_or_else(|| mismatch_url(url, "audited operations table not found"))?;
    let cash_flows = find_table(
        &document,
        &table_selector,
        "net cash provided by operating activities",
        "depreciation and amortization",
    )
    .ok_or_else(|| mismatch_url(url, "audited cash-flow table not found"))?;
    let balance_sheet = find_table(&document, &table_selector, "total assets", "cash and cash")
        .ok_or_else(|| mismatch_url(url, "audited balance sheet not found"))?;

    let period_start =
        NaiveDate::from_ymd_opt(annual.start.0, annual.start.1, annual.start.2).unwrap();
    let period_end = filing.period_of_report.unwrap();
    if period_end != NaiveDate::from_ymd_opt(annual.end.0, annual.end.1, annual.end.2).unwrap() {
        return Err(mismatch_url(url, "unexpected legacy annual period end"));
    }

    let mut facts = Vec::new();
    let required_duration_specs = [
        (
            &operations,
            "total revenues",
            "Revenues",
            "USD",
            Scale::Thousands,
        ),
        (
            &operations,
            "net income",
            "NetIncomeLoss",
            "USD",
            Scale::Thousands,
        ),
        (
            &cash_flows,
            "net cash provided by operating activities",
            "NetCashProvidedByUsedInOperatingActivities",
            "USD",
            Scale::Thousands,
        ),
        (
            &cash_flows,
            "depreciation and amortization",
            "DepreciationAndAmortization",
            "USD",
            Scale::Thousands,
        ),
    ];
    for (table, label, concept, unit, scale) in required_duration_specs {
        let value = first_value(table, label, 0)
            .ok_or_else(|| mismatch_url(url, &format!("missing audited row '{label}'")))?;
        facts.push(raw_fact(
            cik,
            filing,
            concept,
            unit,
            scale.apply(value)?,
            Some(period_start),
            period_end,
            "FY",
        ));
    }

    for (table, labels, concept) in [
        (
            &operations,
            &["costs of merchandise sold", "cost of merchandise sold"][..],
            "CostOfGoodsSold",
        ),
        (
            &cash_flows,
            &[
                "purchases of property and equipment",
                "capital expenditures",
                "capital spending",
            ][..],
            "PaymentsToAcquirePropertyPlantAndEquipment",
        ),
    ] {
        if let Some(value) = labels.iter().find_map(|label| first_value(table, label, 0)) {
            facts.push(raw_fact(
                cik,
                filing,
                concept,
                "USD",
                Scale::Thousands.apply(value)?,
                Some(period_start),
                period_end,
                "FY",
            ));
        }
    }

    for (label, concept) in [
        (
            "cash and cash equivalents",
            "CashAndCashEquivalentsAtCarryingValue",
        ),
        ("total assets", "Assets"),
    ] {
        let value = first_value(&balance_sheet, label, 0)
            .ok_or_else(|| mismatch_url(url, &format!("missing audited row '{label}'")))?;
        facts.push(raw_fact(
            cik,
            filing,
            concept,
            "USD",
            Scale::Thousands.apply(value)?,
            None,
            period_end,
            "FY",
        ));
    }
    Ok(facts)
}

fn parse_bbw_10q(
    cik: &Cik,
    filing: &Filing,
    quarter: &LegacyQuarter,
    html: &str,
    url: &str,
) -> Result<Vec<RawFact>, SourceError> {
    let document = Html::parse_document(html);
    let table_selector = Selector::parse("table").unwrap();
    let operations = document
        .select(&table_selector)
        .find(|table| {
            let text = normalized_text(*table);
            text.contains("total revenues")
                && (text.contains("net income") || text.contains("net loss"))
        })
        .ok_or_else(|| mismatch_url(url, "quarterly operations table not found"))?;
    let cash_flows = document
        .select(&table_selector)
        .find(|table| {
            let text = normalized_text(*table);
            text.contains("depreciation and amortization") && text.contains("operating activities")
        })
        .ok_or_else(|| mismatch_url(url, "quarterly cash-flow table not found"))?;
    let balance_sheet = document.select(&table_selector).find(|table| {
        let text = normalized_text(*table);
        text.contains("total assets") && text.contains("cash and cash equivalents")
    });

    let fiscal_start = tuple_date(quarter.fiscal_start);
    let quarter_start = tuple_date(quarter.quarter_start);
    let period_end = filing.period_of_report.unwrap();
    if period_end != tuple_date(quarter.end) {
        return Err(mismatch_url(url, "unexpected legacy quarterly period end"));
    }

    let mut facts = Vec::new();
    for (labels, concept) in [
        (&["total revenues"][..], "Revenues"),
        (
            &["net income", "net income (loss)", "net loss"][..],
            "NetIncomeLoss",
        ),
        (
            &["costs of merchandise sold", "cost of merchandise sold"][..],
            "CostOfGoodsSold",
        ),
    ] {
        let Some(values) = first_amounts_for_labels(&operations, labels) else {
            if concept == "CostOfGoodsSold" {
                continue;
            }
            return Err(mismatch_url(
                url,
                &format!("missing quarterly row for {concept}"),
            ));
        };
        let single = values
            .get(quarter.single_value_index)
            .copied()
            .ok_or_else(|| {
                mismatch_url(url, &format!("missing current-quarter value for {concept}"))
            })?;
        facts.push(raw_fact(
            cik,
            filing,
            concept,
            "USD",
            Scale::Thousands.apply(single)?,
            Some(quarter_start),
            period_end,
            quarter.fp,
        ));
        if let Some(index) = quarter.ytd_value_index {
            let ytd = values.get(index).copied().ok_or_else(|| {
                mismatch_url(url, &format!("missing current-YTD value for {concept}"))
            })?;
            facts.push(raw_fact(
                cik,
                filing,
                concept,
                "USD",
                Scale::Thousands.apply(ytd)?,
                Some(fiscal_start),
                period_end,
                quarter.fp,
            ));
        }
    }

    for (labels, concept) in [
        (
            &[
                "net cash provided by operating activities",
                "net cash used in operating activities",
                "net cash from operating activities",
                "net cash provided by (used in) operating activities",
            ][..],
            "NetCashProvidedByUsedInOperatingActivities",
        ),
        (
            &["depreciation and amortization"][..],
            "DepreciationAndAmortization",
        ),
        (
            &[
                "purchases of property and equipment, net",
                "purchases of property and equipment",
            ][..],
            "PaymentsToAcquirePropertyPlantAndEquipment",
        ),
    ] {
        let Some(values) = first_amounts_for_labels(&cash_flows, labels) else {
            continue;
        };
        let value = values
            .get(quarter.instant_value_index)
            .copied()
            .ok_or_else(|| {
                mismatch_url(
                    url,
                    &format!("missing current cash-flow value for {concept}"),
                )
            })?;
        facts.push(raw_fact(
            cik,
            filing,
            concept,
            "USD",
            Scale::Thousands.apply(value)?,
            Some(fiscal_start),
            period_end,
            quarter.fp,
        ));
    }

    if let Some(balance_sheet) = balance_sheet {
        for (label, concept) in [
            (
                "cash and cash equivalents",
                "CashAndCashEquivalentsAtCarryingValue",
            ),
            ("total assets", "Assets"),
        ] {
            if let Some(value) = amounts_after_label(&balance_sheet, label)
                .and_then(|values| values.get(quarter.instant_value_index).copied())
            {
                facts.push(raw_fact(
                    cik,
                    filing,
                    concept,
                    "USD",
                    Scale::Thousands.apply(value)?,
                    None,
                    period_end,
                    quarter.fp,
                ));
            }
        }
    }
    Ok(facts)
}

fn tuple_date(parts: (i32, u32, u32)) -> NaiveDate {
    NaiveDate::from_ymd_opt(parts.0, parts.1, parts.2).unwrap()
}

fn first_amounts_for_labels(
    table: &scraper::ElementRef<'_>,
    labels: &[&str],
) -> Option<Vec<ParsedAmount>> {
    labels
        .iter()
        .find_map(|label| amounts_after_label(table, label))
}

fn amounts_after_label(table: &scraper::ElementRef<'_>, label: &str) -> Option<Vec<ParsedAmount>> {
    let text = normalized_text(*table);
    let start = text.find(label)? + label.len();
    let after_label = &text[start..];
    let requested_end = after_label
        .find(" total revenues")
        .or_else(|| after_label.find(" net retail sales"))
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

fn find_table<'a>(
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

fn first_value(
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

fn normalized_text(element: scraper::ElementRef<'_>) -> String {
    element
        .text()
        .flat_map(str::split_whitespace)
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[derive(Clone, Copy)]
struct ParsedAmount {
    negative: bool,
    value: f64,
}

impl ParsedAmount {
    fn parse(input: &str) -> Option<Self> {
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
enum Scale {
    Thousands,
}

impl Scale {
    fn apply(self, amount: ParsedAmount) -> Result<i64, SourceError> {
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
            return Err(mismatch(
                "legacy annual value overflowed micro-unit storage",
            ));
        }
        Ok(scaled.round() as i64)
    }
}

fn raw_fact(
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
    mismatch_url("BBW historical SEC submissions", detail)
}

fn mismatch_url(url: &str, detail: &str) -> SourceError {
    SourceError::SchemaMismatch {
        url: url.into(),
        detail: detail.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_url_uses_unpadded_cik_and_stripped_accession() {
        assert_eq!(
            primary_document_url(
                &Cik(BBW_CIK.into()),
                &AccessionNo(BBW_LEGACY_ANNUALS[0].accession.into()),
                "c93387e10vk.htm"
            ),
            "https://www.sec.gov/Archives/edgar/data/1113809/000095013705003677/c93387e10vk.htm"
        );
    }

    #[test]
    fn parses_split_parentheses_and_scales_values() {
        let amount = ParsedAmount::parse("(16,494)").unwrap();
        assert_eq!(Scale::Thousands.apply(amount).unwrap(), -16_494_000_000_000);
    }
}
