//! Reviewed Build-A-Bear operating metrics sourced from SEC annual reports.

use std::collections::HashMap;

use crate::domain::{Cik, NewCustomMetric, PeriodKind, UnitKind};
use crate::errors::RepoError;
use crate::repos::custom_metric::CustomMetricRepo;
use crate::repos::period::PeriodRepo;

const BBW_CIK: &str = "0001113809";
const MANAGED_PREFIX: &str = "bbw_";

struct Definition {
    key: &'static str,
    label: &'static str,
    unit: UnitKind,
    currency: Option<&'static str>,
    decimals: u8,
    description: &'static str,
    values: &'static [(i32, i64)],
}

const SOURCE_YEARS: &[i32] = &[
    2004, 2005, 2006, 2007, 2008, 2009, 2010, 2011, 2012, 2013, 2014, 2015, 2016, 2017, 2018, 2019,
    2020, 2021, 2022, 2023, 2024, 2025,
];

const MARGIN_USD: &[(i32, i64)] = &[
    (2004, 149_566_000_000_000),
    (2005, 178_528_000_000_000),
    (2006, 205_063_000_000_000),
    (2007, 209_090_000_000_000),
    (2008, 190_500_000_000_000),
    (2009, 142_572_000_000_000),
    (2010, 155_128_000_000_000),
    (2011, 154_468_000_000_000),
    (2012, 145_687_000_000_000),
    (2013, 153_477_000_000_000),
    (2014, 176_838_000_000_000),
    (2015, 175_614_000_000_000),
    (2016, 161_679_000_000_000),
    (2017, 163_948_000_000_000),
    (2018, 139_470_000_000_000),
    (2019, 146_839_000_000_000),
    (2020, 101_427_000_000_000),
    (2021, 211_308_000_000_000),
    (2022, 234_692_000_000_000),
    (2023, 249_348_000_000_000),
    (2024, 253_125_000_000_000),
    (2025, 272_727_000_000_000),
];

const MARGIN_PERCENT: &[(i32, i64)] = &[
    (2004, 498_000),
    (2005, 497_000),
    (2006, 474_000),
    (2007, 447_000),
    (2008, 413_000),
    (2009, 367_000),
    (2010, 401_000),
    (2011, 399_000),
    (2012, 389_000),
    (2013, 411_000),
    (2014, 456_000),
    (2015, 471_000),
    (2016, 452_000),
    (2017, 469_000),
    (2018, 427_000),
    (2019, 454_000),
    (2020, 407_000),
    (2021, 531_000),
    (2022, 526_000),
    (2023, 547_000),
    (2024, 550_000),
    (2025, 561_000),
];

const COMPANY_LOCATIONS: &[(i32, i64)] = &[
    (2004, 170_000_000),
    (2005, 200_000_000),
    (2006, 271_000_000),
    (2007, 321_000_000),
    (2008, 351_000_000),
    (2009, 350_000_000),
    (2010, 359_000_000),
    (2011, 356_000_000),
    (2012, 351_000_000),
    (2013, 323_000_000),
    (2014, 324_000_000),
    (2015, 329_000_000),
    (2016, 346_000_000),
    (2017, 361_000_000),
    (2018, 373_000_000),
    (2019, 372_000_000),
    (2020, 354_000_000),
    (2021, 346_000_000),
    (2022, 350_000_000),
    (2023, 359_000_000),
    (2024, 368_000_000),
    (2025, 375_000_000),
];

const PARTNER_LOCATIONS: &[(i32, i64)] = &[
    (2018, 40_000_000),
    (2019, 60_000_000),
    (2020, 56_000_000),
    (2021, 61_000_000),
    (2022, 70_000_000),
    (2023, 92_000_000),
    (2024, 138_000_000),
    (2025, 178_000_000),
];

const FRANCHISE_LOCATIONS: &[(i32, i64)] = &[
    (2005, 30_000_000),
    (2006, 34_000_000),
    (2007, 53_000_000),
    (2008, 62_000_000),
    (2009, 65_000_000),
    (2010, 63_000_000),
    (2011, 79_000_000),
    (2012, 91_000_000),
    (2013, 86_000_000),
    (2014, 71_000_000),
    (2015, 77_000_000),
    (2016, 92_000_000),
    (2017, 102_000_000),
    (2018, 97_000_000),
    (2019, 92_000_000),
    (2020, 71_000_000),
    (2021, 72_000_000),
    (2022, 68_000_000),
    (2023, 74_000_000),
    (2024, 83_000_000),
    (2025, 109_000_000),
];

const SALES_PER_STORE: &[(i32, i64)] = &[
    (2004, 1_857_000_000_000),
    (2005, 1_864_000_000_000),
    (2006, 1_761_000_000_000),
    (2007, 1_576_000_000_000),
    (2008, 1_329_000_000_000),
    (2009, 1_044_000_000_000),
    (2010, 1_030_000_000_000),
    (2011, 1_021_000_000_000),
    (2012, 1_003_000_000_000),
    (2013, 1_080_000_000_000),
    (2014, 1_158_000_000_000),
    (2015, 1_075_000_000_000),
    (2016, 1_007_000_000_000),
    (2017, 918_000_000_000),
];

const SALES_PER_SQ_FT: &[(i32, i64)] = &[
    (2004, 602_000_000),
    (2005, 615_000_000),
    (2006, 573_000_000),
    (2007, 516_000_000),
    (2008, 445_000_000),
    (2009, 358_000_000),
    (2010, 356_000_000),
    (2011, 354_000_000),
    (2012, 350_000_000),
    (2013, 381_000_000),
    (2014, 409_000_000),
    (2015, 394_000_000),
    (2016, 371_000_000),
    (2017, 343_000_000),
];

const DEFINITIONS: &[Definition] = &[
    Definition { key: "bbw_retail_gross_margin", label: "Retail Gross Margin", unit: UnitKind::Currency, currency: Some("USD"), decimals: 0, description: "Reported retail gross margin. FY2004-FY2009 was labeled gross margin; FY2010 onward excludes wholesale merchandise costs.", values: MARGIN_USD },
    Definition { key: "bbw_retail_gross_margin_percent", label: "Retail Gross Margin Percent", unit: UnitKind::Ratio, currency: None, decimals: 1, description: "Reported retail gross margin divided by net retail sales.", values: MARGIN_PERCENT },
    Definition { key: "bbw_company_managed_locations", label: "Company-Managed Locations", unit: UnitKind::Count, currency: None, decimals: 0, description: "Corporately managed locations at fiscal year end; excludes partner-operated and franchised locations.", values: COMPANY_LOCATIONS },
    Definition { key: "bbw_partner_operated_locations", label: "Partner-Operated Locations", unit: UnitKind::Count, currency: None, decimals: 0, description: "Partner-operated locations at fiscal year end, beginning when separately disclosed.", values: PARTNER_LOCATIONS },
    Definition { key: "bbw_international_franchise_locations", label: "International Franchise Locations", unit: UnitKind::Count, currency: None, decimals: 0, description: "International franchise locations at fiscal year end; FY2004 omitted because only a post-year-end count was disclosed.", values: FRANCHISE_LOCATIONS },
    Definition { key: "bbw_average_net_retail_sales_per_store", label: "Average Net Retail Sales per Store", unit: UnitKind::Currency, currency: Some("USD"), decimals: 0, description: "Company-reported North American average for stores open throughout the year, excluding e-commerce. Reporting discontinued after FY2017.", values: SALES_PER_STORE },
    Definition { key: "bbw_net_retail_sales_per_square_foot", label: "Net Retail Sales per Square Foot", unit: UnitKind::Currency, currency: Some("USD"), decimals: 0, description: "Company-reported North American sales per leased/gross square foot. Reporting discontinued after FY2017.", values: SALES_PER_SQ_FT },
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SyncSummary {
    pub metrics_created: usize,
    pub points_upserted: usize,
}

pub async fn sync(
    cik: &Cik,
    periods: &dyn PeriodRepo,
    metrics: &dyn CustomMetricRepo,
) -> Result<SyncSummary, RepoError> {
    if cik.0 != BBW_CIK {
        return Ok(SyncSummary::default());
    }
    let annual_periods = periods.list_for_cik(cik, Some(PeriodKind::Annual)).await?;
    let period_by_year: HashMap<i32, i64> = annual_periods
        .into_iter()
        .map(|period| (period.fiscal_year, period.id))
        .collect();
    for source_year in SOURCE_YEARS {
        let app_year = app_fiscal_year(*source_year);
        if !period_by_year.contains_key(&app_year) {
            return Err(RepoError::Integrity(format!(
                "BBW custom metrics require annual period FY{app_year} for company FY{source_year}"
            )));
        }
    }

    let mut summary = SyncSummary::default();
    for definition in DEFINITIONS {
        debug_assert!(definition.key.starts_with(MANAGED_PREFIX));
        if let Some(metric) = metrics.get_by_key(cik, definition.key).await? {
            if metric.unit_kind != definition.unit
                || metric.currency.as_deref() != definition.currency
            {
                return Err(RepoError::Integrity(format!(
                    "managed BBW metric {} conflicts with an existing definition",
                    definition.key
                )));
            }
        }
    }

    for definition in DEFINITIONS {
        let metric = match metrics.get_by_key(cik, definition.key).await? {
            Some(metric) => metric,
            None => {
                summary.metrics_created += 1;
                metrics
                    .create(
                        cik,
                        &NewCustomMetric {
                            key: definition.key.into(),
                            label: definition.label.into(),
                            unit_kind: definition.unit,
                            currency: definition.currency.map(str::to_owned),
                            decimals: Some(definition.decimals),
                            description: Some(definition.description.into()),
                        },
                    )
                    .await?
            }
        };
        for (year, value) in definition.values {
            let period_id = period_by_year[&app_fiscal_year(*year)];
            let token = format!("company FY{year}");
            let note = format!(
                "SEC-reviewed BBW annual report; source accession {}",
                source_accession(definition.key, *year)
            );
            metrics
                .upsert_point(
                    metric.id,
                    Some(period_id),
                    Some(&token),
                    *value,
                    Some(&note),
                )
                .await?;
            summary.points_upserted += 1;
        }
    }
    Ok(summary)
}

fn app_fiscal_year(source_year: i32) -> i32 {
    if source_year >= 2018 {
        source_year + 1
    } else {
        source_year
    }
}

fn source_accession(key: &str, year: i32) -> &'static str {
    if key == "bbw_international_franchise_locations" && year <= 2009 {
        return match year {
            2005 => "0000950137-06-003138",
            2006 => "0000950137-07-003883",
            2007 => "0001193125-08-055664",
            2008 => "0001193125-09-058733",
            2009 => "0001193125-10-060466",
            _ => unreachable!("validated franchise fiscal year"),
        };
    }
    match year {
        2004 => "0001193125-09-058733",
        2005 => "0001193125-10-060466",
        2006 => "0001437749-11-001588",
        2007 => "0001437749-12-002376",
        2008 => "0001437749-13-002868",
        2009 => "0001437749-14-004065",
        2010 => "0001437749-15-005513",
        2011 => "0001437749-16-027902",
        2012 => "0001437749-17-004631",
        2013..=2016 => "0001437749-18-004676",
        2017 => "0001437749-19-007482",
        2018 => "0001437749-20-007818",
        2019 => "0001437749-21-009026",
        2020 => "0001437749-22-008972",
        2021 => "0001437749-23-010169",
        2022 => "0001437749-24-012440",
        2023 => "0001437749-25-012234",
        2024 | 2025 => "0001437749-26-012501",
        _ => unreachable!("validated BBW fiscal year"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Pool;
    use crate::domain::{Company, Period, Ticker};
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
        let cik = Cik(BBW_CIK.into());
        SqliteCompanyRepo::new(pool.clone())
            .upsert(&Company {
                cik: cik.clone(),
                ticker: Ticker("BBW".into()),
                name: "Build-A-Bear Workshop".into(),
                exchange: Some("NYSE".into()),
                sic: None,
                fiscal_year_end: Some("0130".into()),
                added_at: Utc::now(),
                last_refreshed: None,
            })
            .await
            .unwrap();
        let periods = SqlitePeriodRepo::new(pool.clone());
        for source_year in SOURCE_YEARS {
            let year = app_fiscal_year(*source_year);
            periods
                .upsert_returning_id(&Period {
                    id: 0,
                    cik: cik.clone(),
                    fiscal_year: year,
                    fiscal_quarter: 0,
                    fiscal_year_end: "1231".into(),
                    start_date: NaiveDate::from_ymd_opt(year, 1, 1).unwrap(),
                    end_date: NaiveDate::from_ymd_opt(year, 12, 31).unwrap(),
                    kind: PeriodKind::Annual,
                    is_53_week: false,
                })
                .await
                .unwrap();
        }
        (pool, cik)
    }

    #[test]
    fn manifest_has_expected_coverage_and_anchor_values() {
        assert_eq!(DEFINITIONS.len(), 7);
        assert_eq!(MARGIN_USD.len(), 22);
        assert_eq!(MARGIN_PERCENT.len(), 22);
        assert_eq!(COMPANY_LOCATIONS.len(), 22);
        assert_eq!(SALES_PER_STORE.last(), Some(&(2017, 918_000_000_000)));
        assert_eq!(SALES_PER_SQ_FT.last(), Some(&(2017, 343_000_000)));
        assert_eq!(MARGIN_USD[0], (2004, 149_566_000_000_000));
        assert_eq!(MARGIN_PERCENT[0], (2004, 498_000));
    }

    #[tokio::test]
    async fn sync_is_idempotent_and_preserves_source_tokens() {
        let (pool, cik) = setup().await;
        let periods = SqlitePeriodRepo::new(pool.clone());
        let metrics = SqliteCustomMetricRepo::new(pool);
        assert_eq!(
            sync(&cik, &periods, &metrics)
                .await
                .unwrap()
                .metrics_created,
            7
        );
        assert_eq!(
            sync(&cik, &periods, &metrics)
                .await
                .unwrap()
                .metrics_created,
            0
        );
        assert_eq!(metrics.list(&cik).await.unwrap().len(), 7);
        let margin = metrics
            .get_by_key(&cik, "bbw_retail_gross_margin")
            .await
            .unwrap()
            .unwrap();
        let points = metrics
            .points_aligned(margin.id, PeriodKind::Annual)
            .await
            .unwrap();
        assert_eq!(points.len(), 22);
        assert_eq!(points[0].1.observed_date.as_deref(), Some("company FY2004"));
        assert!(points[0]
            .1
            .source_note
            .as_deref()
            .unwrap()
            .contains("0001193125-09-058733"));
        assert_eq!(points.last().unwrap().0.fiscal_year, 2026);
    }

    #[tokio::test]
    async fn non_bbw_sync_is_a_noop() {
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

    #[tokio::test]
    async fn conflicting_reserved_definition_fails_before_writing_points() {
        let (pool, cik) = setup().await;
        let periods = SqlitePeriodRepo::new(pool.clone());
        let metrics = SqliteCustomMetricRepo::new(pool);
        metrics
            .create(
                &cik,
                &NewCustomMetric {
                    key: "bbw_partner_operated_locations".into(),
                    label: "Conflict".into(),
                    unit_kind: UnitKind::Currency,
                    currency: Some("USD".into()),
                    decimals: Some(0),
                    description: None,
                },
            )
            .await
            .unwrap();

        let error = sync(&cik, &periods, &metrics).await.unwrap_err();
        assert!(matches!(error, RepoError::Integrity(_)));
        assert_eq!(metrics.list(&cik).await.unwrap().len(), 1);
        assert_eq!(metrics.count_points(1).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn missing_required_period_fails_before_writing_metrics() {
        let (pool, cik) = setup().await;
        {
            let mut guard = pool.write().await;
            guard
                .conn()
                .execute(
                    "DELETE FROM period WHERE cik = ?1 AND fiscal_year = 2004",
                    [&cik.0],
                )
                .unwrap();
        }
        let periods = SqlitePeriodRepo::new(pool.clone());
        let metrics = SqliteCustomMetricRepo::new(pool);

        assert!(matches!(
            sync(&cik, &periods, &metrics).await,
            Err(RepoError::Integrity(_))
        ));
        assert!(metrics.list(&cik).await.unwrap().is_empty());
    }
}
