//! Custom-metric read path — parallel to `series.rs` but trivial: no
//! derivation, just a pass-through of stored points (techspec §8).

use crate::domain::{CustomSeriesPoint, PeriodKind};
use crate::errors::AppError;
use crate::repos::custom_metric::CustomMetricRepo;

pub async fn custom_metric_series(
    repo: &dyn CustomMetricRepo,
    metric_id: i64,
    kind: PeriodKind,
) -> Result<Vec<CustomSeriesPoint>, AppError> {
    let metric = repo
        .get(metric_id)
        .await?
        .ok_or_else(|| AppError::not_found("custom metric"))?;
    let pts = repo.points_aligned(metric_id, kind).await?;
    Ok(pts
        .into_iter()
        .map(|(period, p)| CustomSeriesPoint {
            period,
            value_micro: p.value_micro,
            unit_kind: metric.unit_kind,
            point_id: p.id,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Pool;
    use crate::domain::{Cik, Company, NewCustomMetric, Ticker, UnitKind};
    use crate::repos::company::{CompanyRepo, SqliteCompanyRepo};
    use crate::repos::custom_metric::SqliteCustomMetricRepo;
    use crate::repos::period::{PeriodRepo, SqlitePeriodRepo};
    use chrono::{NaiveDate, TimeZone, Utc};

    async fn setup() -> std::sync::Arc<Pool> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.sqlite");
        Box::leak(Box::new(dir));
        std::sync::Arc::new(Pool::open(&path).unwrap())
    }

    async fn seed_company(pool: &std::sync::Arc<Pool>) -> Cik {
        let companies = SqliteCompanyRepo::new(pool.clone());
        let cik = Cik("0000000001".into());
        companies
            .upsert(&Company {
                cik: cik.clone(),
                ticker: Ticker("TEST".into()),
                name: "Test Co".into(),
                exchange: None,
                sic: None,
                fiscal_year_end: Some("1231".into()),
                added_at: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
                last_refreshed: None,
            })
            .await
            .unwrap();
        cik
    }

    async fn seed_period(pool: &std::sync::Arc<Pool>, cik: &Cik, fy: i32, kind: PeriodKind, fq: u8) -> i64 {
        let periods = SqlitePeriodRepo::new(pool.clone());
        periods
            .upsert_returning_id(&crate::domain::Period {
                id: 0,
                cik: cik.clone(),
                fiscal_year: fy,
                fiscal_quarter: fq,
                fiscal_year_end: "1231".into(),
                start_date: NaiveDate::from_ymd_opt(fy, 1, 1).unwrap(),
                end_date: NaiveDate::from_ymd_opt(fy, 12, 31).unwrap(),
                kind,
                is_53_week: false,
            })
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn custom_series_excludes_unaligned() {
        let pool = setup().await;
        let cik = seed_company(&pool).await;
        let p2024 = seed_period(&pool, &cik, 2024, PeriodKind::Annual, 0).await;
        let repo = SqliteCustomMetricRepo::new(pool);
        let m = repo
            .create(
                &cik,
                &NewCustomMetric {
                    key: "tpv".into(),
                    label: "TPV".into(),
                    unit_kind: UnitKind::Currency,
                    currency: Some("USD".into()),
                    decimals: Some(2),
                    description: None,
                },
            )
            .await
            .unwrap();
        repo.upsert_point(m.id, Some(p2024), None, 100, None).await.unwrap();
        repo.upsert_point(m.id, None, Some("2019-01-01"), 999, None).await.unwrap();
        let series = custom_metric_series(&repo, m.id, PeriodKind::Annual).await.unwrap();
        assert_eq!(series.len(), 1);
        assert_eq!(series[0].value_micro, 100);
    }

    #[tokio::test]
    async fn custom_series_kind_filter() {
        let pool = setup().await;
        let cik = seed_company(&pool).await;
        let p_annual = seed_period(&pool, &cik, 2024, PeriodKind::Annual, 0).await;
        let p_q1 = seed_period(&pool, &cik, 2024, PeriodKind::Quarterly, 1).await;
        let repo = SqliteCustomMetricRepo::new(pool);
        let m = repo
            .create(
                &cik,
                &NewCustomMetric {
                    key: "tpv".into(),
                    label: "TPV".into(),
                    unit_kind: UnitKind::Currency,
                    currency: Some("USD".into()),
                    decimals: Some(2),
                    description: None,
                },
            )
            .await
            .unwrap();
        repo.upsert_point(m.id, Some(p_annual), None, 1, None).await.unwrap();
        repo.upsert_point(m.id, Some(p_q1), None, 2, None).await.unwrap();
        let annual = custom_metric_series(&repo, m.id, PeriodKind::Annual).await.unwrap();
        let quarterly = custom_metric_series(&repo, m.id, PeriodKind::Quarterly).await.unwrap();
        assert_eq!(annual.len(), 1);
        assert_eq!(quarterly.len(), 1);
        assert_eq!(annual[0].value_micro, 1);
        assert_eq!(quarterly[0].value_micro, 2);
    }
}
