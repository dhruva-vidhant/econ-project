//! LULU-specific pre-XBRL values from annual-report financial schedules.

use crate::domain::{Cik, Filing, FormType, RawFact, MICRO};
use crate::errors::SourceError;

use super::{mismatch_url, raw_fact, tuple_date, FilingConfig, IssuerConfig};

const LULU_CIK: &str = "0001397187";

pub(super) static CONFIG: IssuerConfig = IssuerConfig {
    cik: LULU_CIK,
    historical_anchor: "2008-04-08",
    filings: LULU_FILINGS,
    parse: parse_filing,
};

const LULU_FILINGS: &[FilingConfig] = &[
    FilingConfig::new("0000909567-08-000415", FormType::TenK),
    FilingConfig::new("0000909567-09-000292", FormType::TenK),
    FilingConfig::new("0000950123-10-028033", FormType::TenK),
];

struct OperatingPeriod {
    start: (i32, u32, u32),
    end: (i32, u32, u32),
    fp: &'static str,
    revenue: i64,
    cost_of_goods_sold: i64,
    net_income: i64,
}

const ANNUALS_2008: &[OperatingPeriod] = &[
    period((2003, 2, 1), (2004, 1, 31), "FY", 18_188, 8_748, 319),
    period((2004, 2, 1), (2005, 1, 31), "FY", 40_748, 19_448, -1_411),
];

const QUARTERS_2008: &[OperatingPeriod] = &[
    period((2006, 2, 1), (2006, 4, 30), "Q1", 28_184, 13_664, 3_181),
    period((2006, 5, 1), (2006, 7, 31), "Q2", 32_517, 16_614, 1_940),
    period((2006, 8, 1), (2006, 10, 31), "Q3", 35_968, 17_227, 1_664),
    period((2006, 11, 1), (2007, 1, 31), "Q4", 52_216, 25_397, 882),
];

const QUARTERS_2009: &[OperatingPeriod] = &[
    period((2007, 2, 1), (2007, 4, 30), "Q1", 43_124, 21_026, 3_543),
    period((2007, 5, 1), (2007, 7, 31), "Q2", 57_900, 27_167, 5_122),
    period((2007, 8, 1), (2007, 10, 31), "Q3", 64_925, 29_409, 7_569),
    period((2007, 11, 1), (2008, 2, 3), "Q4", 103_993, 47_413, 14_609),
];

const ANNUALS_2010: &[OperatingPeriod] = &[
    period((2005, 2, 1), (2006, 1, 31), "FY", 84_129, 41_177, 1_394),
    period((2006, 2, 1), (2007, 1, 31), "FY", 147_964, 72_249, 7_666),
    period((2007, 2, 1), (2008, 2, 3), "FY", 269_942, 125_015, 30_843),
    period((2008, 2, 4), (2009, 2, 1), "FY", 353_488, 174_421, 39_363),
    period((2009, 2, 2), (2010, 1, 31), "FY", 452_898, 229_812, 58_281),
];

const QUARTERS_2010: &[OperatingPeriod] = &[
    period((2008, 2, 4), (2008, 5, 4), "Q1", 77_036, 36_898, 8_444),
    period((2008, 5, 5), (2008, 8, 3), "Q2", 85_484, 41_108, 11_147),
    period((2008, 8, 4), (2008, 11, 2), "Q3", 87_047, 45_154, 8_834),
    period((2008, 11, 3), (2009, 2, 1), "Q4", 103_921, 52_261, 10_938),
    period((2009, 2, 2), (2009, 5, 3), "Q1", 81_680, 46_656, 6_518),
    period((2009, 5, 4), (2009, 8, 2), "Q2", 97_721, 52_557, 9_244),
    period((2009, 8, 3), (2009, 11, 1), "Q3", 112_891, 56_553, 14_068),
    period((2009, 11, 2), (2010, 1, 31), "Q4", 160_606, 74_046, 28_451),
];

const fn period(
    start: (i32, u32, u32),
    end: (i32, u32, u32),
    fp: &'static str,
    revenue: i64,
    cost_of_goods_sold: i64,
    net_income: i64,
) -> OperatingPeriod {
    OperatingPeriod {
        start,
        end,
        fp,
        revenue,
        cost_of_goods_sold,
        net_income,
    }
}

fn parse_filing(
    cik: &Cik,
    filing: &Filing,
    html: &str,
    url: &str,
) -> Result<Vec<RawFact>, SourceError> {
    if !html.to_ascii_lowercase().contains("lululemon") {
        return Err(mismatch_url(url, "LULU filing identity marker not found"));
    }
    let (expected_report, periods): ((i32, u32, u32), &[&[OperatingPeriod]]) =
        match filing.accession_no.0.as_str() {
            "0000909567-08-000415" => ((2008, 2, 3), &[ANNUALS_2008, QUARTERS_2008]),
            "0000909567-09-000292" => ((2009, 2, 1), &[QUARTERS_2009]),
            "0000950123-10-028033" => ((2010, 1, 31), &[ANNUALS_2010, QUARTERS_2010]),
            _ => return Err(mismatch_url(url, "unconfigured LULU legacy filing")),
        };
    if filing.period_of_report != Some(tuple_date(expected_report)) {
        return Err(mismatch_url(url, "unexpected LULU legacy report date"));
    }

    let mut facts = Vec::new();
    for group in periods {
        for value in *group {
            for (concept, amount) in [
                ("Revenues", value.revenue),
                ("CostOfGoodsSold", value.cost_of_goods_sold),
                ("NetIncomeLoss", value.net_income),
            ] {
                let scaled = amount
                    .checked_mul(1_000)
                    .and_then(|value| value.checked_mul(MICRO))
                    .ok_or_else(|| mismatch_url(url, "LULU legacy value overflow"))?;
                facts.push(raw_fact(
                    cik,
                    filing,
                    concept,
                    "USD",
                    scaled,
                    Some(tuple_date(value.start)),
                    tuple_date(value.end),
                    value.fp,
                ));
            }
        }
    }
    Ok(facts)
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use crate::domain::{AccessionNo, SourceKind};

    use super::*;

    fn filing(accession: &str, report_date: (i32, u32, u32)) -> Filing {
        Filing {
            accession_no: AccessionNo(accession.into()),
            cik: Cik(LULU_CIK.into()),
            form_type: FormType::TenK,
            filed_at: NaiveDate::from_ymd_opt(2010, 3, 25).unwrap(),
            period_of_report: Some(tuple_date(report_date)),
            is_amendment: false,
            amends: None,
            item_4_02_8k: false,
        }
    }

    fn fact<'a>(facts: &'a [RawFact], concept: &str, end: (i32, u32, u32)) -> &'a RawFact {
        facts
            .iter()
            .find(|fact| fact.concept == concept && fact.period_end == tuple_date(end))
            .unwrap()
    }

    #[test]
    fn emits_annual_boundaries_with_html_provenance() {
        let early = parse_filing(
            &Cik(LULU_CIK.into()),
            &filing("0000909567-08-000415", (2008, 2, 3)),
            "lululemon verified filing",
            "test",
        )
        .unwrap();
        let first = fact(&early, "Revenues", (2004, 1, 31));
        assert_eq!(first.period_start, Some(tuple_date((2003, 2, 1))));
        assert_eq!(first.value_numeric, 18_188_000_000_000);
        assert_eq!(first.fp.as_deref(), Some("FY"));
        assert_eq!(first.source_kind, SourceKind::SecFilingHtml);

        let latest = parse_filing(
            &Cik(LULU_CIK.into()),
            &filing("0000950123-10-028033", (2010, 1, 31)),
            "lululemon verified filing",
            "test",
        )
        .unwrap();
        assert_eq!(
            fact(&latest, "NetIncomeLoss", (2010, 1, 31)).value_numeric,
            58_281_000_000_000
        );
        assert_eq!(
            fact(&latest, "CostOfGoodsSold", (2006, 1, 31)).value_numeric,
            41_177_000_000_000
        );
    }

    #[test]
    fn emits_complete_quarter_boundaries_and_periods() {
        let first_year = parse_filing(
            &Cik(LULU_CIK.into()),
            &filing("0000909567-08-000415", (2008, 2, 3)),
            "lululemon verified filing",
            "test",
        )
        .unwrap();
        let first = fact(&first_year, "Revenues", (2006, 4, 30));
        assert_eq!(first.period_start, Some(tuple_date((2006, 2, 1))));
        assert_eq!(first.value_numeric, 28_184_000_000_000);
        assert_eq!(first.fp.as_deref(), Some("Q1"));

        let last_year = parse_filing(
            &Cik(LULU_CIK.into()),
            &filing("0000950123-10-028033", (2010, 1, 31)),
            "lululemon verified filing",
            "test",
        )
        .unwrap();
        let middle_year = parse_filing(
            &Cik(LULU_CIK.into()),
            &filing("0000909567-09-000292", (2009, 2, 1)),
            "lululemon verified filing",
            "test",
        )
        .unwrap();
        assert_eq!(
            fact(&middle_year, "NetIncomeLoss", (2007, 4, 30)).value_numeric,
            3_543_000_000_000
        );
        let last = last_year
            .iter()
            .find(|fact| {
                fact.concept == "NetIncomeLoss"
                    && fact.period_end == tuple_date((2010, 1, 31))
                    && fact.fp.as_deref() == Some("Q4")
            })
            .unwrap();
        assert_eq!(last.period_start, Some(tuple_date((2009, 11, 2))));
        assert_eq!(last.value_numeric, 28_451_000_000_000);
        assert_eq!(last.fp.as_deref(), Some("Q4"));

        let quarterly = first_year
            .iter()
            .chain(middle_year.iter())
            .chain(last_year.iter())
            .filter(|fact| fact.fp.as_deref().is_some_and(|fp| fp != "FY"))
            .count();
        assert_eq!(quarterly, 48);
    }

    #[test]
    fn rejects_wrong_document_or_unknown_accession() {
        let known = filing("0000909567-08-000415", (2008, 2, 3));
        assert!(parse_filing(&Cik(LULU_CIK.into()), &known, "other issuer", "test").is_err());

        let unknown = filing("0000000000-00-000000", (2008, 2, 3));
        assert!(parse_filing(&Cik(LULU_CIK.into()), &unknown, "lululemon filing", "test").is_err());

        let wrong_date = filing("0000909567-08-000415", (2008, 2, 2));
        assert!(parse_filing(
            &Cik(LULU_CIK.into()),
            &wrong_date,
            "lululemon filing",
            "test"
        )
        .is_err());
    }
}
