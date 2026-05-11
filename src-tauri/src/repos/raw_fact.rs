//! Raw fact repository — M09. Bulk-insert + read API.

use async_trait::async_trait;

use crate::db::Pool;
use crate::domain::{AccessionNo, Cik, RawFact, SourceKind};
use crate::errors::RepoError;

#[async_trait]
pub trait RawFactRepo: Send + Sync {
    async fn upsert_many(&self, facts: &[RawFact]) -> Result<usize, RepoError>;
    async fn list_for_filing(&self, accn: &AccessionNo) -> Result<Vec<RawFact>, RepoError>;
    async fn get(&self, id: i64) -> Result<Option<RawFact>, RepoError>;
}

pub struct SqliteRawFactRepo { pool: std::sync::Arc<Pool> }
impl SqliteRawFactRepo {
    pub fn new(pool: std::sync::Arc<Pool>) -> Self { Self { pool } }
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawFact> {
    Ok(RawFact {
        id: row.get(0)?,
        cik: Cik(row.get(1)?),
        accession_no: AccessionNo(row.get(2)?),
        taxonomy: row.get(3)?,
        concept: row.get(4)?,
        unit: row.get(5)?,
        value_numeric: row.get(6)?,
        period_start: row.get(7)?,
        period_end: row.get(8)?,
        is_instant: row.get::<_, i64>(9)? != 0,
        fy: row.get(10)?,
        fp: row.get(11)?,
        filed: row.get(12)?,
        source_kind: SourceKind::from_str(&row.get::<_, String>(13)?).unwrap_or(SourceKind::XbrlApi),
        ingested_at: row.get(14)?,
    })
}

const COLS: &str =
    "id, cik, accession_no, taxonomy, concept, unit, value_numeric, period_start, period_end,
     is_instant, fy, fp, filed, source_kind, ingested_at";

#[async_trait]
impl RawFactRepo for SqliteRawFactRepo {
    async fn upsert_many(&self, facts: &[RawFact]) -> Result<usize, RepoError> {
        if facts.is_empty() { return Ok(0); }
        let mut g = self.pool.write().await;
        let tx = g.conn().transaction()?;
        let mut inserted = 0;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO raw_fact
                  (cik, accession_no, taxonomy, concept, unit, value_numeric, period_start,
                   period_end, is_instant, fy, fp, filed, source_kind, ingested_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)
                 ON CONFLICT (cik, accession_no, taxonomy, concept, unit, period_start, period_end, fp)
                 DO NOTHING",
            )?;
            for f in facts {
                let n = stmt.execute(rusqlite::params![
                    f.cik.0, f.accession_no.0, f.taxonomy, f.concept, f.unit,
                    f.value_numeric, f.period_start, f.period_end, f.is_instant as i64,
                    f.fy, f.fp, f.filed, f.source_kind.as_str(), f.ingested_at,
                ])?;
                inserted += n;
            }
        }
        tx.commit()?;
        Ok(inserted)
    }

    async fn list_for_filing(&self, accn: &AccessionNo) -> Result<Vec<RawFact>, RepoError> {
        let g = self.pool.read()?;
        let q = format!("SELECT {COLS} FROM raw_fact WHERE accession_no = ?1");
        let mut stmt = g.conn().prepare(&q)?;
        let rows = stmt.query_map([&accn.0], map_row)?;
        let mut out = Vec::new();
        for r in rows { out.push(r?); }
        Ok(out)
    }

    async fn get(&self, id: i64) -> Result<Option<RawFact>, RepoError> {
        let g = self.pool.read()?;
        let q = format!("SELECT {COLS} FROM raw_fact WHERE id = ?1");
        let mut stmt = g.conn().prepare(&q)?;
        match stmt.query_row([id], map_row) {
            Ok(f) => Ok(Some(f)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}
