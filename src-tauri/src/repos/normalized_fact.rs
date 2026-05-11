//! Normalized fact repository — M10. Supersession-aware.

use async_trait::async_trait;

use crate::db::Pool;
use crate::domain::{Cik, Metric, NormalizedFact, Period, PeriodKind, SourceKind};
use crate::errors::RepoError;

#[async_trait]
pub trait NormalizedFactRepo: Send + Sync {
    /// Insert a new primary value. If a previous primary exists for
    /// (cik, metric, period_id), update its `superseded_by` → new id atomically.
    async fn insert_primary_with_supersession(&self, n: &NormalizedFact) -> Result<i64, RepoError>;
    async fn insert_alternate(&self, n: &NormalizedFact) -> Result<i64, RepoError>;
    async fn current_value(&self, cik: &Cik, metric: Metric, period_id: i64) -> Result<Option<NormalizedFact>, RepoError>;
    async fn current_series(&self, cik: &Cik, metric: Metric, kind: PeriodKind) -> Result<Vec<(Period, NormalizedFact)>, RepoError>;
    async fn supersession_chain(&self, id: i64) -> Result<Vec<NormalizedFact>, RepoError>;
}

pub struct SqliteNormalizedFactRepo { pool: std::sync::Arc<Pool> }
impl SqliteNormalizedFactRepo {
    pub fn new(pool: std::sync::Arc<Pool>) -> Self { Self { pool } }
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<NormalizedFact> {
    Ok(NormalizedFact {
        id: row.get(0)?,
        cik: Cik(row.get(1)?),
        metric: Metric::from_str(&row.get::<_, String>(2)?).unwrap_or(Metric::Revenue),
        period_id: row.get(3)?,
        value: row.get(4)?,
        unit: row.get(5)?,
        source_fact_id: row.get(6)?,
        source_kind: SourceKind::from_str(&row.get::<_, String>(7)?).unwrap_or(SourceKind::XbrlApi),
        is_primary: row.get::<_, i64>(8)? != 0,
        original_value: row.get(9)?,
        original_unit: row.get(10)?,
        fx_rate_micro: row.get(11)?,
        fx_rate_source: row.get(12)?,
        fx_rate_date: row.get(13)?,
        superseded_by: row.get(14)?,
    })
}

const COLS: &str =
    "id, cik, metric, period_id, value, unit, source_fact_id, source_kind, is_primary,
     original_value, original_unit, fx_rate_micro, fx_rate_source, fx_rate_date, superseded_by";

#[async_trait]
impl NormalizedFactRepo for SqliteNormalizedFactRepo {
    async fn insert_primary_with_supersession(&self, n: &NormalizedFact) -> Result<i64, RepoError> {
        let mut g = self.pool.write().await;
        let tx = g.conn().transaction()?;
        // Find current primary, if any.
        let prev: Option<i64> = tx
            .query_row(
                "SELECT id FROM normalized_fact
                 WHERE cik = ?1 AND metric = ?2 AND period_id = ?3
                   AND is_primary = 1 AND superseded_by IS NULL",
                rusqlite::params![n.cik.0, n.metric.as_str(), n.period_id],
                |r| r.get(0),
            )
            .ok();
        // Insert the new primary.
        tx.execute(
            "INSERT INTO normalized_fact
              (cik, metric, period_id, value, unit, source_fact_id, source_kind, is_primary,
               original_value, original_unit, fx_rate_micro, fx_rate_source, fx_rate_date,
               superseded_by, ingested_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,1,?8,?9,?10,?11,?12,NULL,?13)",
            rusqlite::params![
                n.cik.0, n.metric.as_str(), n.period_id, n.value, n.unit,
                n.source_fact_id, n.source_kind.as_str(),
                n.original_value, n.original_unit, n.fx_rate_micro, n.fx_rate_source,
                n.fx_rate_date, chrono::Utc::now(),
            ],
        )?;
        let new_id: i64 = tx.last_insert_rowid();
        // Point the previous primary at the new one.
        if let Some(prev_id) = prev {
            tx.execute(
                "UPDATE normalized_fact SET superseded_by = ?1 WHERE id = ?2",
                rusqlite::params![new_id, prev_id],
            )?;
        }
        tx.commit()?;
        Ok(new_id)
    }

    async fn insert_alternate(&self, n: &NormalizedFact) -> Result<i64, RepoError> {
        let mut g = self.pool.write().await;
        g.conn().execute(
            "INSERT INTO normalized_fact
              (cik, metric, period_id, value, unit, source_fact_id, source_kind, is_primary,
               original_value, original_unit, fx_rate_micro, fx_rate_source, fx_rate_date,
               superseded_by, ingested_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,0,?8,?9,?10,?11,?12,NULL,?13)",
            rusqlite::params![
                n.cik.0, n.metric.as_str(), n.period_id, n.value, n.unit,
                n.source_fact_id, n.source_kind.as_str(),
                n.original_value, n.original_unit, n.fx_rate_micro, n.fx_rate_source,
                n.fx_rate_date, chrono::Utc::now(),
            ],
        )?;
        Ok(g.conn().last_insert_rowid())
    }

    async fn current_value(&self, cik: &Cik, metric: Metric, period_id: i64) -> Result<Option<NormalizedFact>, RepoError> {
        let g = self.pool.read()?;
        let q = format!(
            "SELECT {COLS} FROM normalized_fact
             WHERE cik = ?1 AND metric = ?2 AND period_id = ?3
               AND is_primary = 1 AND superseded_by IS NULL"
        );
        let mut stmt = g.conn().prepare(&q)?;
        match stmt.query_row(rusqlite::params![cik.0, metric.as_str(), period_id], map_row) {
            Ok(f) => Ok(Some(f)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn current_series(&self, cik: &Cik, metric: Metric, kind: PeriodKind) -> Result<Vec<(Period, NormalizedFact)>, RepoError> {
        let g = self.pool.read()?;
        let q = format!(
            "SELECT
              p.id, p.cik, p.fiscal_year, p.fiscal_quarter, p.fiscal_year_end,
              p.start_date, p.end_date, p.kind, p.is_53_week,
              {COLS}
             FROM normalized_fact n
             JOIN period p ON p.id = n.period_id
             WHERE n.cik = ?1 AND n.metric = ?2 AND p.kind = ?3
               AND n.is_primary = 1 AND n.superseded_by IS NULL
             ORDER BY p.start_date"
        );
        let mut stmt = g.conn().prepare(&q)?;
        let rows = stmt.query_map(
            rusqlite::params![cik.0, metric.as_str(), kind.as_str()],
            |row| {
                let p = Period {
                    id: row.get(0)?,
                    cik: Cik(row.get(1)?),
                    fiscal_year: row.get(2)?,
                    fiscal_quarter: row.get::<_, i64>(3)? as u8,
                    fiscal_year_end: row.get(4)?,
                    start_date: row.get(5)?,
                    end_date: row.get(6)?,
                    kind: PeriodKind::from_str(&row.get::<_, String>(7)?).unwrap_or(PeriodKind::Annual),
                    is_53_week: row.get::<_, i64>(8)? != 0,
                };
                // Skip 9 columns (period), then map normalized_fact at offset 9.
                let n = NormalizedFact {
                    id: row.get(9)?,
                    cik: Cik(row.get(10)?),
                    metric: Metric::from_str(&row.get::<_, String>(11)?).unwrap_or(Metric::Revenue),
                    period_id: row.get(12)?,
                    value: row.get(13)?,
                    unit: row.get(14)?,
                    source_fact_id: row.get(15)?,
                    source_kind: SourceKind::from_str(&row.get::<_, String>(16)?).unwrap_or(SourceKind::XbrlApi),
                    is_primary: row.get::<_, i64>(17)? != 0,
                    original_value: row.get(18)?,
                    original_unit: row.get(19)?,
                    fx_rate_micro: row.get(20)?,
                    fx_rate_source: row.get(21)?,
                    fx_rate_date: row.get(22)?,
                    superseded_by: row.get(23)?,
                };
                Ok((p, n))
            },
        )?;
        let mut out = Vec::new();
        for r in rows { out.push(r?); }
        Ok(out)
    }

    async fn supersession_chain(&self, id: i64) -> Result<Vec<NormalizedFact>, RepoError> {
        // Walk backward by repeatedly finding the row whose superseded_by points at id.
        let g = self.pool.read()?;
        let mut chain = Vec::new();
        let mut current_id = id;
        loop {
            let q = format!("SELECT {COLS} FROM normalized_fact WHERE superseded_by = ?1");
            let mut stmt = g.conn().prepare(&q)?;
            match stmt.query_row([current_id], map_row) {
                Ok(prev) => { current_id = prev.id; chain.push(prev); }
                Err(rusqlite::Error::QueryReturnedNoRows) => break,
                Err(e) => return Err(e.into()),
            }
        }
        chain.reverse(); // oldest → newest
        Ok(chain)
    }
}
