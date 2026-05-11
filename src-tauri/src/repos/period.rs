//! Period repository — M08.

use async_trait::async_trait;

use crate::db::Pool;
use crate::domain::{Cik, Period, PeriodKind};
use crate::errors::RepoError;

#[async_trait]
pub trait PeriodRepo: Send + Sync {
    async fn upsert_returning_id(&self, p: &Period) -> Result<i64, RepoError>;
    async fn get_id(&self, cik: &Cik, fy: i32, fq: u8) -> Result<Option<i64>, RepoError>;
    async fn list_for_cik(&self, cik: &Cik, kind: Option<PeriodKind>) -> Result<Vec<Period>, RepoError>;
}

pub struct SqlitePeriodRepo {
    pool: std::sync::Arc<Pool>,
}

impl SqlitePeriodRepo {
    pub fn new(pool: std::sync::Arc<Pool>) -> Self { Self { pool } }
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
                p.cik.0, p.fiscal_year, p.fiscal_quarter as i64, p.fiscal_year_end,
                p.start_date, p.end_date, p.kind.as_str(), p.is_53_week as i64,
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
        match stmt.query_row(rusqlite::params![cik.0, fy, fq as i64], |r| r.get::<_, i64>(0)) {
            Ok(id) => Ok(Some(id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn list_for_cik(&self, cik: &Cik, kind: Option<PeriodKind>) -> Result<Vec<Period>, RepoError> {
        let g = self.pool.read()?;
        let mut out = Vec::new();
        match kind {
            Some(k) => {
                let q = format!(
                    "SELECT {COLS} FROM period WHERE cik = ?1 AND kind = ?2
                     ORDER BY fiscal_year, fiscal_quarter"
                );
                let mut stmt = g.conn().prepare(&q)?;
                let rows = stmt.query_map(
                    rusqlite::params![cik.0, k.as_str()],
                    map_row,
                )?;
                for r in rows { out.push(r?); }
            }
            None => {
                let q = format!(
                    "SELECT {COLS} FROM period WHERE cik = ?1
                     ORDER BY fiscal_year, fiscal_quarter"
                );
                let mut stmt = g.conn().prepare(&q)?;
                let rows = stmt.query_map([&cik.0], map_row)?;
                for r in rows { out.push(r?); }
            }
        }
        Ok(out)
    }
}
