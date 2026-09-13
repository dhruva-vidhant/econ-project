//! Reviewed Phillips 66 refining metrics sourced from SEC filings.

use std::collections::HashMap;

use crate::domain::{Cik, NewCustomMetric, UnitKind};
use crate::errors::RepoError;
use crate::repos::custom_metric::CustomMetricRepo;
use crate::repos::period::PeriodRepo;

const PSX_CIK: &str = "0001534701";
pub const REALIZED_MARGIN_KEY: &str = "psx_realized_refining_margin_per_barrel";

struct Point {
    year: i32,
    quarter: u8,
    value_micro: i64,
    accession: &'static str,
    derivation: Option<&'static str>,
}

const POINTS: &[Point] = &[
    annual(2012, 13_350_000, "0001534701-15-000038"),
    quarterly(2012, 2, 12_560_000, "0001193125-12-334961"),
    quarterly(2012, 3, 17_050_000, "0001193125-12-446999"),
    annual(2013, 9_900_000, "0001534701-16-000119"),
    quarterly(2013, 1, 13_940_000, "0001534701-13-000009"),
    quarterly(2013, 2, 9_880_000, "0001534701-13-000016"),
    quarterly(2013, 3, 6_140_000, "0001534701-13-000022"),
    annual(2014, 9_930_000, "0001534701-16-000119"),
    quarterly(2014, 1, 9_880_000, "0001534701-14-000061"),
    quarterly(2014, 2, 9_660_000, "0001534701-14-000100"),
    quarterly(2014, 3, 10_890_000, "0001534701-14-000116"),
    annual(2015, 11_840_000, "0001534701-18-000065"),
    quarterly(2015, 1, 12_260_000, "0001534701-15-000049"),
    quarterly(2015, 2, 11_700_000, "0001534701-15-000066"),
    quarterly(2015, 3, 13_960_000, "0001534701-15-000075"),
    annual(2016, 6_990_000, "0001534701-19-000063"),
    quarterly(2016, 1, 7_110_000, "0001534701-16-000130"),
    quarterly(2016, 2, 7_130_000, "0001534701-16-000153"),
    quarterly(2016, 3, 7_230_000, "0001534701-16-000175"),
    annual(2017, 9_130_000, "0001534701-20-000070"),
    quarterly(2017, 1, 8_550_000, "0001534701-17-000074"),
    quarterly(2017, 2, 8_440_000, "0001534701-17-000083"),
    quarterly(2017, 3, 10_490_000, "0001534701-17-000105"),
    derived_q4(2017, 8_977_673, "0001534701-18-000065; 0001534701-17-000105", "($7,270M FY - $5,391M 9M) / (795,931K FY - 586,634K 9M processed barrels)"),
    annual(2018, 12_990_000, "0001534701-21-000074"),
    quarterly(2018, 1, 9_290_000, "0001534701-18-000071"),
    quarterly(2018, 2, 12_280_000, "0001534701-18-000094"),
    quarterly(2018, 3, 13_360_000, "0001534701-18-000109"),
    derived_q4(2018, 16_528_926, "0001534701-19-000063; 0001534701-18-000109", "($10,478M FY - $6,950M 9M) / (806,600K FY - 593,156K 9M processed barrels)"),
    annual(2019, 9_910_000, "0001534701-22-000078"),
    quarterly(2019, 1, 7_230_000, "0001534701-19-000072"),
    quarterly(2019, 2, 11_370_000, "0001534701-19-000095"),
    quarterly(2019, 3, 11_180_000, "0001534701-19-000135"),
    derived_q4(2019, 9_500_972, "0001534701-20-000070; 0001534701-19-000135", "($8,002M FY - $6,013M 9M) / (807,231K FY - 597,884K 9M processed barrels)"),
    annual(2020, 3_770_000, "0001534701-23-000053"),
    quarterly(2020, 1, 7_110_000, "0001534701-20-000085"),
    quarterly(2020, 2, 2_600_000, "0001534701-20-000120"),
    quarterly(2020, 3, 1_780_000, "0001534701-20-000154"),
    derived_q4(2020, 2_181_993, "0001534701-21-000074; 0001534701-20-000154", "($2,310M FY - $1,983M 9M) / (657,702K FY - 507,839K 9M processed barrels)"),
    annual(2021, 7_420_000, "0001534701-24-000078"),
    quarterly(2021, 1, 4_360_000, "0001534701-21-000106"),
    quarterly(2021, 2, 3_920_000, "0001534701-21-000150"),
    quarterly(2021, 3, 8_570_000, "0001534701-21-000209"),
    derived_q4(2021, 11_602_435, "0001534701-22-000078; 0001534701-21-000209", "($5,116M FY - $3,061M 9M) / (715,780K FY - 538,662K 9M processed barrels)"),
    annual(2022, 21_770_000, "0001534701-25-000074"),
    quarterly(2022, 1, 10_550_000, "0001534701-22-000103"),
    quarterly(2022, 2, 28_310_000, "0001534701-22-000143"),
    quarterly(2022, 3, 26_580_000, "0001534701-22-000185"),
    derived_q4(2022, 20_590_403, "0001534701-23-000053; 0001534701-22-000185", "($14,983M FY - $11,379M 9M) / (695,157K FY - 520,124K 9M processed barrels)"),
    annual(2023, 17_260_000, "0001534701-26-000006"),
    quarterly(2023, 1, 20_720_000, "0001534701-23-000080"),
    quarterly(2023, 2, 15_320_000, "0001534701-23-000123"),
    quarterly(2023, 3, 18_960_000, "0001534701-23-000148"),
    derived_q4(2023, 14_408_525, "0001534701-24-000078; 0001534701-23-000148", "($11,939M FY - $9,435M 9M) / (689,401K FY - 515,615K 9M processed barrels)"),
    annual(2024, 8_840_000, "0001534701-25-000074"),
    quarterly(2024, 1, 10_910_000, "0001534701-24-000104"),
    quarterly(2024, 2, 10_010_000, "0001534701-24-000132"),
    quarterly(2024, 3, 8_310_000, "0001534701-24-000166"),
    derived_q4(2024, 6_086_616, "0001534701-25-000074; 0001534701-24-000166", "($6,014M FY - $4,973M 9M) / (680,043K FY - 509,012K 9M processed barrels)"),
    annual(2025, 10_880_000, "0001534701-26-000006"),
    quarterly(2025, 1, 6_810_000, "0001534701-25-000094"),
    quarterly(2025, 2, 11_250_000, "0001534701-25-000137"),
    quarterly(2025, 3, 12_150_000, "0001534701-25-000155"),
    derived_q4(2025, 12_493_073, "0001534701-26-000006; 0001534701-25-000155", "($7,480M FY - $5,113M 9M) / (687,189K FY - 497,724K 9M processed barrels); Q4 includes fully consolidated WRB operations"),
    quarterly(2026, 1, 10_110_000, "0001534701-26-000022"),
    quarterly(2026, 2, 24_080_000, "0001534701-26-000032"),
];

const fn annual(year: i32, value_micro: i64, accession: &'static str) -> Point {
    Point {
        year,
        quarter: 0,
        value_micro,
        accession,
        derivation: None,
    }
}

const fn quarterly(year: i32, quarter: u8, value_micro: i64, accession: &'static str) -> Point {
    Point {
        year,
        quarter,
        value_micro,
        accession,
        derivation: None,
    }
}

const fn derived_q4(
    year: i32,
    value_micro: i64,
    accession: &'static str,
    derivation: &'static str,
) -> Point {
    Point {
        year,
        quarter: 4,
        value_micro,
        accession,
        derivation: Some(derivation),
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SyncSummary {
    pub metrics_created: usize,
    pub points_upserted: usize,
}

pub fn is_managed(cik: &Cik, key: &str) -> bool {
    cik.0 == PSX_CIK && key == REALIZED_MARGIN_KEY
}

pub async fn sync(
    cik: &Cik,
    periods: &dyn PeriodRepo,
    metrics: &dyn CustomMetricRepo,
) -> Result<SyncSummary, RepoError> {
    if cik.0 != PSX_CIK {
        return Ok(SyncSummary::default());
    }
    let period_by_slot: HashMap<(i32, u8), i64> = periods
        .list_for_cik(cik, None)
        .await?
        .into_iter()
        .map(|period| ((period.fiscal_year, period.fiscal_quarter), period.id))
        .collect();
    for point in POINTS {
        if !period_by_slot.contains_key(&(point.year, point.quarter)) {
            return Err(RepoError::Integrity(format!(
                "PSX refining margin requires FY{} Q{} period",
                point.year, point.quarter
            )));
        }
    }

    if let Some(metric) = metrics.get_by_key(cik, REALIZED_MARGIN_KEY).await? {
        if metric.unit_kind != UnitKind::Number || metric.currency.is_some() {
            return Err(RepoError::Integrity(format!(
                "managed PSX metric {REALIZED_MARGIN_KEY} conflicts with an existing definition"
            )));
        }
    }

    let mut summary = SyncSummary::default();
    let metric = match metrics.get_by_key(cik, REALIZED_MARGIN_KEY).await? {
        Some(metric) => metric,
        None => {
            summary.metrics_created = 1;
            metrics
                .create(
                    cik,
                    &NewCustomMetric {
                        key: REALIZED_MARGIN_KEY.into(),
                        label: "Realized Refining Margin per Barrel".into(),
                        unit_kind: UnitKind::Number,
                        currency: None,
                        decimals: Some(2),
                        description: Some("Worldwide realized refining margin in USD per processed barrel, reported by Phillips 66. Q4 values from 2017 onward are derived as (full-year margin dollars minus nine-month margin dollars) divided by (full-year processed inputs minus nine-month processed inputs).".into()),
                    },
                )
                .await?
        }
    };
    for point in POINTS {
        let period_id = period_by_slot[&(point.year, point.quarter)];
        let period = if point.quarter == 0 {
            format!("FY{}", point.year)
        } else {
            format!("FY{} Q{}", point.year, point.quarter)
        };
        let note = match point.derivation {
            Some(derivation) => format!(
                "SEC-reviewed PSX filing; derived from accessions {}; {}",
                point.accession, derivation
            ),
            None => format!(
                "SEC-reviewed PSX filing; reported worldwide value; source accession {}",
                point.accession
            ),
        };
        metrics
            .upsert_point(
                metric.id,
                Some(period_id),
                Some(&period),
                point.value_micro,
                Some(&note),
            )
            .await?;
        summary.points_upserted += 1;
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Pool;
    use crate::domain::{Company, Period, PeriodKind, Ticker};
    use crate::repos::company::{CompanyRepo, SqliteCompanyRepo};
    use crate::repos::custom_metric::SqliteCustomMetricRepo;
    use crate::repos::period::SqlitePeriodRepo;
    use chrono::{NaiveDate, Utc};
    use std::sync::Arc;

    async fn setup() -> (Arc<Pool>, Cik) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.sqlite");
        Box::leak(Box::new(dir));
        let pool = Arc::new(Pool::open(&path).unwrap());
        let cik = Cik(PSX_CIK.into());
        SqliteCompanyRepo::new(pool.clone())
            .upsert(&Company {
                cik: cik.clone(),
                ticker: Ticker("PSX".into()),
                name: "Phillips 66".into(),
                exchange: Some("NYSE".into()),
                sic: None,
                fiscal_year_end: Some("1231".into()),
                added_at: Utc::now(),
                last_refreshed: None,
            })
            .await
            .unwrap();
        let periods = SqlitePeriodRepo::new(pool.clone());
        for point in POINTS {
            periods
                .upsert_returning_id(&Period {
                    id: 0,
                    cik: cik.clone(),
                    fiscal_year: point.year,
                    fiscal_quarter: point.quarter,
                    fiscal_year_end: "1231".into(),
                    start_date: NaiveDate::from_ymd_opt(point.year, 1, 1).unwrap(),
                    end_date: NaiveDate::from_ymd_opt(
                        point.year,
                        if point.quarter == 0 {
                            12
                        } else {
                            point.quarter as u32 * 3
                        },
                        if point.quarter == 0 || point.quarter == 1 || point.quarter == 4 {
                            31
                        } else {
                            30
                        },
                    )
                    .unwrap(),
                    kind: if point.quarter == 0 {
                        PeriodKind::Annual
                    } else {
                        PeriodKind::Quarterly
                    },
                    is_53_week: false,
                })
                .await
                .unwrap();
        }
        (pool, cik)
    }

    #[test]
    fn manifest_has_reported_and_defensible_derived_coverage() {
        assert_eq!(POINTS.iter().filter(|p| p.quarter == 0).count(), 14);
        assert_eq!(POINTS.iter().filter(|p| p.quarter > 0).count(), 52);
        assert!(!POINTS.iter().any(|p| p.year == 2012 && p.quarter == 1));
        assert!(!POINTS
            .iter()
            .any(|p| (2012..=2016).contains(&p.year) && p.quarter == 4));
        assert_eq!(POINTS.last().unwrap().value_micro, 24_080_000);
    }

    #[tokio::test]
    async fn sync_is_idempotent_and_preserves_provenance() {
        let (pool, cik) = setup().await;
        let periods = SqlitePeriodRepo::new(pool.clone());
        let metrics = SqliteCustomMetricRepo::new(pool);
        assert_eq!(
            sync(&cik, &periods, &metrics)
                .await
                .unwrap()
                .metrics_created,
            1
        );
        assert_eq!(
            sync(&cik, &periods, &metrics)
                .await
                .unwrap()
                .metrics_created,
            0
        );
        let metric = metrics
            .get_by_key(&cik, REALIZED_MARGIN_KEY)
            .await
            .unwrap()
            .unwrap();
        assert!(is_managed(&cik, &metric.key));
        assert_eq!(
            metrics
                .points_aligned(metric.id, PeriodKind::Annual)
                .await
                .unwrap()
                .len(),
            14
        );
        let quarterly = metrics
            .points_aligned(metric.id, PeriodKind::Quarterly)
            .await
            .unwrap();
        assert_eq!(quarterly.len(), 52);
        assert!(quarterly
            .iter()
            .find(|(p, _)| p.fiscal_year == 2025 && p.fiscal_quarter == 4)
            .unwrap()
            .1
            .source_note
            .as_deref()
            .unwrap()
            .contains("497,724K"));
    }

    #[tokio::test]
    async fn unsupported_company_is_a_noop() {
        let (pool, _) = setup().await;
        let periods = SqlitePeriodRepo::new(pool.clone());
        let metrics = SqliteCustomMetricRepo::new(pool);
        assert_eq!(
            sync(&Cik("0000320193".into()), &periods, &metrics)
                .await
                .unwrap(),
            SyncSummary::default()
        );
    }
}
