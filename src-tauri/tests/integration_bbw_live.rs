//! Live BBW regression for a genuine historical fiscal-calendar change.
//!
//! Run with:
//!   cargo test --test integration_bbw_live -- --ignored --nocapture

use std::sync::Arc;

use econ_project_lib::db::Pool;
use econ_project_lib::domain::{Metric, PeriodKind, SourceKind, Ticker};
use econ_project_lib::pipeline::{ingest_company, IngestionDeps};
use econ_project_lib::repos::company::SqliteCompanyRepo;
use econ_project_lib::repos::current_price::SqliteCurrentPriceRepo;
use econ_project_lib::repos::derived_metric::SqliteDerivedMetricRepo;
use econ_project_lib::repos::filing::SqliteFilingRepo;
use econ_project_lib::repos::historical_price::SqliteHistoricalPriceRepo;
use econ_project_lib::repos::ingestion_event::SqliteIngestionEventRepo;
use econ_project_lib::repos::normalized_fact::SqliteNormalizedFactRepo;
use econ_project_lib::repos::period::SqlitePeriodRepo;
use econ_project_lib::repos::raw_fact::SqliteRawFactRepo;
use econ_project_lib::sources::market_data::YahooMarketData;
use econ_project_lib::sources::sec_client::SecClient;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn bbw_recovers_pre_2019_annuals_and_labels_quarters_by_historical_calendar() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let pool = Arc::new(Pool::open(tmp.path()).unwrap());
    let deps = IngestionDeps {
        sec: Arc::new(
            SecClient::new("EconProject-BBW-Live/0.1 contact@econproject.example", 5).unwrap(),
        ),
        market_data: Arc::new(YahooMarketData::new().unwrap()),
        companies: Arc::new(SqliteCompanyRepo::new(pool.clone())),
        filings: Arc::new(SqliteFilingRepo::new(pool.clone())),
        periods: Arc::new(SqlitePeriodRepo::new(pool.clone())),
        raw_facts: Arc::new(SqliteRawFactRepo::new(pool.clone())),
        normalized_facts: Arc::new(SqliteNormalizedFactRepo::new(pool.clone())),
        derived_metrics: Arc::new(SqliteDerivedMetricRepo::new(pool.clone())),
        prices: Arc::new(SqliteHistoricalPriceRepo::new(pool.clone())),
        current_prices: Arc::new(SqliteCurrentPriceRepo::new(pool.clone())),
        events: Arc::new(SqliteIngestionEventRepo::new(pool.clone())),
    };

    let (company, _) = ingest_company(&deps, &Ticker("BBW".into())).await.unwrap();
    let annual = deps
        .normalized_facts
        .current_series(&company.cik, Metric::Revenue, PeriodKind::Annual)
        .await
        .unwrap();
    assert!(annual
        .iter()
        .any(|(period, _)| period.end_date.year() < 2019));
    assert!(annual.iter().any(|(period, _)| {
        period.end_date == chrono::NaiveDate::from_ymd_opt(2005, 1, 1).unwrap()
            && period.fiscal_year == 2004
    }));
    let (_, fy2004_revenue) = annual
        .iter()
        .find(|(period, _)| period.fiscal_year == 2004)
        .expect("BBW FY2004 revenue missing from first public annual report");
    assert_eq!(fy2004_revenue.value, 301_662_000_000_000);
    assert_eq!(fy2004_revenue.source_kind, SourceKind::SecFilingHtml);
    for fiscal_year in 2004..=2008 {
        assert!(
            annual
                .iter()
                .any(|(period, _)| period.fiscal_year == fiscal_year),
            "BBW annual revenue missing for FY{fiscal_year}"
        );
    }
    assert!(annual.iter().any(|(period, _)| {
        period.end_date == chrono::NaiveDate::from_ymd_opt(2017, 12, 30).unwrap()
            && period.fiscal_year == 2017
            && period.fiscal_year_end.starts_with("12")
    }));
    assert!(annual.iter().any(|(period, _)| {
        period.end_date == chrono::NaiveDate::from_ymd_opt(2019, 2, 2).unwrap()
            && period.fiscal_year == 2019
            && period.fiscal_year_end == "0130"
    }));

    let quarterly = deps
        .normalized_facts
        .current_series(&company.cik, Metric::Revenue, PeriodKind::Quarterly)
        .await
        .unwrap();
    let (_, fy2004_q3_revenue) = quarterly
        .iter()
        .find(|(period, _)| period.fiscal_year == 2004 && period.fiscal_quarter == 3)
        .expect("BBW FY2004 Q3 revenue missing from its first public 10-Q");
    assert_eq!(fy2004_q3_revenue.source_kind, SourceKind::SecFilingHtml);
    for fiscal_year in 2005..=2009 {
        for fiscal_quarter in 1..=4 {
            assert!(
                quarterly.iter().any(|(period, _)| {
                    period.fiscal_year == fiscal_year && period.fiscal_quarter == fiscal_quarter
                }),
                "BBW quarterly revenue missing for FY{fiscal_year} Q{fiscal_quarter}"
            );
        }
    }
    let (_, fy2009_q2_revenue) = quarterly
        .iter()
        .find(|(period, _)| period.fiscal_year == 2009 && period.fiscal_quarter == 2)
        .expect("BBW FY2009 Q2 revenue missing");
    assert_eq!(fy2009_q2_revenue.value, 82_404_000_000_000);
    assert!(quarterly
        .iter()
        .filter(|(period, _)| period.fiscal_year == 2017)
        .all(|(period, _)| { period.fiscal_year_end.starts_with("12") }));
}

use chrono::Datelike;
