//! Period repository — M08.

use async_trait::async_trait;

use crate::db::Pool;
use crate::domain::{Cik, Period, PeriodKind};
use crate::errors::RepoError;

#[async_trait]
pub trait PeriodRepo: Send + Sync {
    async fn upsert_returning_id(&self, p: &Period) -> Result<i64, RepoError>;
    async fn get_id(&self, cik: &Cik, fy: i32, fq: u8) -> Result<Option<i64>, RepoError>;
    async fn list_for_cik(
        &self,
        cik: &Cik,
        kind: Option<PeriodKind>,
    ) -> Result<Vec<Period>, RepoError>;
    async fn reset_normalized_for_reclassification(&self, cik: &Cik) -> Result<(), RepoError>;
}

pub struct SqlitePeriodRepo {
    pool: std::sync::Arc<Pool>,
}

impl SqlitePeriodRepo {
    pub fn new(pool: std::sync::Arc<Pool>) -> Self {
        Self { pool }
    }
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Period> {
    Ok(Period {
        id: row.get(0)?,
        cik: Cik(row.get(1)?),
        fiscal_year: row.get(2)?,
        fiscal_quarter: row.get::<_, i64>(3)? as u8,
        fiscal_year_end: row.get(4)?,
        start_date: row.get(5)?,
        end_date: row.get(6)?,
        kind: PeriodKind::from_str(&row.get::<_, String>(7)?).unwrap_or(PeriodKind::Annual),
        is_53_week: row.get::<_, i64>(8)? != 0,
    })
}

const COLS: &str =
    "id, cik, fiscal_year, fiscal_quarter, fiscal_year_end, start_date, end_date, kind, is_53_week";

#[async_trait]
impl PeriodRepo for SqlitePeriodRepo {
    async fn upsert_returning_id(&self, p: &Period) -> Result<i64, RepoError> {
        let mut g = self.pool.write().await;
        g.conn().execute(
            "INSERT INTO period (cik, fiscal_year, fiscal_quarter, fiscal_year_end,
                start_date, end_date, kind, is_53_week)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(cik, fiscal_year, fiscal_quarter) DO UPDATE SET
                fiscal_year_end=excluded.fiscal_year_end,
                start_date=excluded.start_date,
                end_date=excluded.end_date,
                kind=excluded.kind,
                is_53_week=excluded.is_53_week",
            rusqlite::params![
                p.cik.0,
                p.fiscal_year,
                p.fiscal_quarter as i64,
                p.fiscal_year_end,
                p.start_date,
                p.end_date,
                p.kind.as_str(),
                p.is_53_week as i64,
            ],
        )?;
        let id: i64 = g.conn().query_row(
            "SELECT id FROM period WHERE cik = ?1 AND fiscal_year = ?2 AND fiscal_quarter = ?3",
            rusqlite::params![p.cik.0, p.fiscal_year, p.fiscal_quarter as i64],
            |r| r.get(0),
        )?;
        Ok(id)
    }

    async fn get_id(&self, cik: &Cik, fy: i32, fq: u8) -> Result<Option<i64>, RepoError> {
        let g = self.pool.read()?;
        let mut stmt = g.conn().prepare(
            "SELECT id FROM period WHERE cik = ?1 AND fiscal_year = ?2 AND fiscal_quarter = ?3",
        )?;
        match stmt.query_row(rusqlite::params![cik.0, fy, fq as i64], |r| {
            r.get::<_, i64>(0)
        }) {
            Ok(id) => Ok(Some(id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn list_for_cik(
        &self,
        cik: &Cik,
        kind: Option<PeriodKind>,
    ) -> Result<Vec<Period>, RepoError> {
        let g = self.pool.read()?;
        let mut out = Vec::new();
        match kind {
            Some(k) => {
                let q = format!(
                    "SELECT {COLS} FROM period WHERE cik = ?1 AND kind = ?2
                     ORDER BY fiscal_year, fiscal_quarter"
                );
                let mut stmt = g.conn().prepare(&q)?;
                let rows = stmt.query_map(rusqlite::params![cik.0, k.as_str()], map_row)?;
                for r in rows {
                    out.push(r?);
                }
            }
            None => {
                let q = format!(
                    "SELECT {COLS} FROM period WHERE cik = ?1
                     ORDER BY fiscal_year, fiscal_quarter"
                );
                let mut stmt = g.conn().prepare(&q)?;
                let rows = stmt.query_map([&cik.0], map_row)?;
                for r in rows {
                    out.push(r?);
                }
            }
        }
        Ok(out)
    }

    async fn reset_normalized_for_reclassification(&self, cik: &Cik) -> Result<(), RepoError> {
        let mut g = self.pool.write().await;
        let tx = g.conn().transaction()?;
        tx.execute(
            "UPDATE custom_metric_point SET period_id = NULL
             WHERE period_id IN (SELECT id FROM period WHERE cik = ?1)",
            [&cik.0],
        )?;
        tx.execute(
            "DELETE FROM restatement_resolved_by WHERE restatement_announcement_id IN
             (SELECT id FROM restatement_announcement WHERE cik = ?1)",
            [&cik.0],
        )?;
        tx.execute(
            "DELETE FROM restatement_announcement WHERE cik = ?1",
            [&cik.0],
        )?;
        tx.execute(
            "DELETE FROM amendment_coverage_gap WHERE cik = ?1",
            [&cik.0],
        )?;
        tx.execute("DELETE FROM derived_metric WHERE cik = ?1", [&cik.0])?;
        tx.execute("DELETE FROM normalized_fact WHERE cik = ?1", [&cik.0])?;
        tx.execute("DELETE FROM period WHERE cik = ?1", [&cik.0])?;
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        AccessionNo, Company, Filing, FormType, NormalizedFact, SourceKind, Ticker,
    };
    use crate::repos::company::{CompanyRepo, SqliteCompanyRepo};
    use crate::repos::filing::{FilingRepo, SqliteFilingRepo};
    use crate::repos::normalized_fact::{NormalizedFactRepo, SqliteNormalizedFactRepo};
    use crate::repos::raw_fact::{RawFactRepo, SqliteRawFactRepo};
    use chrono::{NaiveDate, Utc};

    #[tokio::test]
    async fn reclassification_reset_removes_derived_rows_but_keeps_source_data() {
        let dir = tempfile::tempdir().unwrap();
        let pool = std::sync::Arc::new(Pool::open(&dir.path().join("test.sqlite")).unwrap());
        let cik = Cik("0001113809".into());
        SqliteCompanyRepo::new(pool.clone())
            .upsert(&Company {
                cik: cik.clone(),
                ticker: Ticker("BBW".into()),
                name: "Build-A-Bear".into(),
                exchange: None,
                sic: None,
                fiscal_year_end: Some("0130".into()),
                added_at: Utc::now(),
                last_refreshed: None,
            })
            .await
            .unwrap();
        let accession = AccessionNo("test".into());
        SqliteFilingRepo::new(pool.clone())
            .upsert(&Filing {
                accession_no: accession.clone(),
                cik: cik.clone(),
                form_type: FormType::TenK,
                filed_at: NaiveDate::from_ymd_opt(2018, 3, 15).unwrap(),
                period_of_report: Some(NaiveDate::from_ymd_opt(2017, 12, 30).unwrap()),
                is_amendment: false,
                amends: None,
                item_4_02_8k: false,
            })
            .await
            .unwrap();
        let raw_repo = SqliteRawFactRepo::new(pool.clone());
        raw_repo
            .upsert_many(&[crate::domain::RawFact {
                id: 0,
                cik: cik.clone(),
                accession_no: accession.clone(),
                taxonomy: "us-gaap".into(),
                concept: "Revenues".into(),
                unit: "USD".into(),
                value_numeric: 1,
                period_start: Some(NaiveDate::from_ymd_opt(2017, 1, 1).unwrap()),
                period_end: NaiveDate::from_ymd_opt(2017, 12, 30).unwrap(),
                is_instant: false,
                fy: Some(2018),
                fp: Some("FY".into()),
                filed: Some(NaiveDate::from_ymd_opt(2018, 3, 15).unwrap()),
                source_kind: SourceKind::XbrlApi,
                ingested_at: Utc::now(),
            }])
            .await
            .unwrap();
        let raw = raw_repo
            .list_for_filing(&accession)
            .await
            .unwrap()
            .remove(0);
        let periods = SqlitePeriodRepo::new(pool.clone());
        let period_id = periods
            .upsert_returning_id(&Period {
                id: 0,
                cik: cik.clone(),
                fiscal_year: 2018,
                fiscal_quarter: 0,
                fiscal_year_end: "0130".into(),
                start_date: raw.period_start.unwrap(),
                end_date: raw.period_end,
                kind: PeriodKind::Annual,
                is_53_week: false,
            })
            .await
            .unwrap();
        SqliteNormalizedFactRepo::new(pool.clone())
            .insert_primary_with_supersession(&NormalizedFact {
                id: 0,
                cik: cik.clone(),
                metric: crate::domain::Metric::Revenue,
                period_id,
                value: 1,
                unit: "USD".into(),
                source_fact_id: raw.id,
                source_kind: raw.source_kind,
                is_primary: true,
                original_value: None,
                original_unit: None,
                fx_rate_micro: None,
                fx_rate_source: None,
                fx_rate_date: None,
                superseded_by: None,
            })
            .await
            .unwrap();

        periods
            .reset_normalized_for_reclassification(&cik)
            .await
            .unwrap();

        assert!(periods.list_for_cik(&cik, None).await.unwrap().is_empty());
        assert_eq!(raw_repo.list_for_filing(&accession).await.unwrap().len(), 1);
        assert_eq!(
            SqliteFilingRepo::new(pool.clone())
                .list_for_cik(&cik)
                .await
                .unwrap()
                .len(),
            1
        );
    }
}
