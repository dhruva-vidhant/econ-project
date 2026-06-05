//! Derived metric repository — M11.

use async_trait::async_trait;

use crate::db::Pool;
use crate::domain::{Cik, DerivedMetric, Period, PeriodKind};
use crate::errors::RepoError;

#[async_trait]
pub trait DerivedMetricRepo: Send + Sync {
    async fn upsert(&self, d: &DerivedMetric) -> Result<(), RepoError>;
    async fn get(&self, cik: &Cik, formula: &str, period_id: i64) -> Result<Option<DerivedMetric>, RepoError>;
    async fn series(&self, cik: &Cik, formula: &str, kind: PeriodKind) -> Result<Vec<(Period, DerivedMetric)>, RepoError>;
}

pub struct SqliteDerivedMetricRepo { pool: std::sync::Arc<Pool> }
impl SqliteDerivedMetricRepo {
    pub fn new(pool: std::sync::Arc<Pool>) -> Self { Self { pool } }
}

#[async_trait]
impl DerivedMetricRepo for SqliteDerivedMetricRepo {
    async fn upsert(&self, d: &DerivedMetric) -> Result<(), RepoError> {
        let mut g = self.pool.write().await;
        g.conn().execute(
            "INSERT INTO derived_metric (cik, formula_id, period_id, value, is_complete, computed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(cik, formula_id, period_id) DO UPDATE SET
                value = excluded.value, is_complete = excluded.is_complete,
                computed_at = excluded.computed_at",
            rusqlite::params![
                d.cik.0, d.formula_id, d.period_id, d.value, d.is_complete as i64,
                chrono::Utc::now(),
            ],
        )?;
        Ok(())
    }

    async fn get(&self, cik: &Cik, formula: &str, period_id: i64) -> Result<Option<DerivedMetric>, RepoError> {
        let g = self.pool.read()?;
        let mut stmt = g.conn().prepare(
            "SELECT id, cik, formula_id, period_id, value, is_complete
             FROM derived_metric WHERE cik = ?1 AND formula_id = ?2 AND period_id = ?3"
        )?;
        match stmt.query_row(rusqlite::params![cik.0, formula, period_id], |r| {
            Ok(DerivedMetric {
                id: r.get(0)?,
                cik: Cik(r.get(1)?),
                formula_id: r.get(2)?,
                period_id: r.get(3)?,
                value: r.get(4)?,
                is_complete: r.get::<_, i64>(5)? != 0,
            })
        }) {
            Ok(d) => Ok(Some(d)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn series(&self, cik: &Cik, formula: &str, kind: PeriodKind) -> Result<Vec<(Period, DerivedMetric)>, RepoError> {
        let g = self.pool.read()?;
        let mut stmt = g.conn().prepare(
            "SELECT p.id, p.cik, p.fiscal_year, p.fiscal_quarter, p.fiscal_year_end,
                    p.start_date, p.end_date, p.kind, p.is_53_week,
                    d.id, d.cik, d.formula_id, d.period_id, d.value, d.is_complete
             FROM derived_metric d
             JOIN period p ON p.id = d.period_id
             WHERE d.cik = ?1 AND d.formula_id = ?2 AND p.kind = ?3
             ORDER BY p.end_date"
        )?;
        let rows = stmt.query_map(rusqlite::params![cik.0, formula, kind.as_str()], |r| {
            Ok((
                Period {
                    id: r.get(0)?,
                    cik: Cik(r.get(1)?),
                    fiscal_year: r.get(2)?,
                    fiscal_quarter: r.get::<_, i64>(3)? as u8,
                    fiscal_year_end: r.get(4)?,
                    start_date: r.get(5)?,
                    end_date: r.get(6)?,
                    kind: PeriodKind::from_str(&r.get::<_, String>(7)?).unwrap_or(PeriodKind::Annual),
                    is_53_week: r.get::<_, i64>(8)? != 0,
                },
                DerivedMetric {
                    id: r.get(9)?,
                    cik: Cik(r.get(10)?),
                    formula_id: r.get(11)?,
                    period_id: r.get(12)?,
                    value: r.get(13)?,
                    is_complete: r.get::<_, i64>(14)? != 0,
                },
            ))
        })?;
        let mut out = Vec::new();
        for r in rows { out.push(r?); }
        Ok(out)
    }
}
