//! BBW-specific pre-XBRL filing configuration and HTML parsers.

use chrono::NaiveDate;
use scraper::{Html, Selector};

use crate::domain::{Cik, Filing, FormType, RawFact};
use crate::errors::SourceError;

use super::{
    amounts_after_label, find_table, first_amounts_for_labels, first_value, mismatch_url,
    normalized_text, raw_fact, tuple_date, FilingConfig, IssuerConfig, Scale,
};

const BBW_CIK: &str = "0001113809";
const ROW_BOUNDARY_LABELS: &[&str] = &[" total revenues", " net retail sales"];

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

pub(super) static CONFIG: IssuerConfig = IssuerConfig {
    cik: BBW_CIK,
    historical_anchor: "2005-03-29",
    filings: BBW_FILINGS,
    parse: parse_filing,
};

const BBW_FILINGS: &[FilingConfig] = &[
    FilingConfig::new("0000950137-05-003677", FormType::TenK),
    FilingConfig::new("0000950137-06-003138", FormType::TenK),
    FilingConfig::new("0000950137-07-003883", FormType::TenK),
    FilingConfig::new("0001193125-08-055664", FormType::TenK),
    FilingConfig::new("0001193125-09-058733", FormType::TenK),
    FilingConfig::new("0000950137-04-010981", FormType::TenQ),
    FilingConfig::new("0000950137-05-006068", FormType::TenQ),
    FilingConfig::new("0000950137-05-010328", FormType::TenQ),
    FilingConfig::new("0000950137-05-013841", FormType::TenQ),
    FilingConfig::new("0000950137-06-005729", FormType::TenQ),
    FilingConfig::new("0000950137-06-008978", FormType::TenQ),
    FilingConfig::new("0000950137-06-012176", FormType::TenQ),
    FilingConfig::new("0000950137-07-007249", FormType::TenQ),
    FilingConfig::new("0001193125-07-177576", FormType::TenQ),
    FilingConfig::new("0001193125-07-240474", FormType::TenQ),
    FilingConfig::new("0001193125-08-107855", FormType::TenQ),
    FilingConfig::new("0001193125-08-168236", FormType::TenQ),
    FilingConfig::new("0001193125-08-227842", FormType::TenQ),
    FilingConfig::new("0001193125-09-111477", FormType::TenQ),
    FilingConfig::new("0001193125-09-174221", FormType::TenQ),
    FilingConfig::new("0001193125-09-232255", FormType::TenQ),
];

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

fn parse_filing(
    cik: &Cik,
    filing: &Filing,
    html: &str,
    url: &str,
) -> Result<Vec<RawFact>, SourceError> {
    if let Some(annual) = BBW_LEGACY_ANNUALS
        .iter()
        .find(|annual| annual.accession == filing.accession_no.0)
    {
        return parse_bbw_10k(cik, filing, annual, html, url);
    }
    let quarter = BBW_LEGACY_QUARTERS
        .iter()
        .find(|quarter| quarter.accession == filing.accession_no.0)
        .ok_or_else(|| mismatch_url(url, "unconfigured BBW legacy filing"))?;
    parse_bbw_10q(cik, filing, quarter, html, url)
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
        let Some(values) = first_amounts_for_labels(&operations, labels, ROW_BOUNDARY_LABELS)
        else {
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
        let Some(values) = first_amounts_for_labels(&cash_flows, labels, ROW_BOUNDARY_LABELS)
        else {
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
            if let Some(value) = amounts_after_label(&balance_sheet, label, ROW_BOUNDARY_LABELS)
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
