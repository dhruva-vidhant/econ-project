//! Ingestion event repository — M12.

use async_trait::async_trait;

use crate::db::Pool;
use crate::domain::{AccessionNo, Cik, IngestionEvent, Severity};
use crate::errors::RepoError;

#[async_trait]
pub trait IngestionEventRepo: Send + Sync {
    async fn record(&self, e: &IngestionEvent) -> Result<i64, RepoError>;
    async fn recent(&self, cik: Option<&Cik>, limit: u32) -> Result<Vec<IngestionEvent>, RepoError>;
    async fn user_visible(&self, cik: &Cik) -> Result<Vec<IngestionEvent>, RepoError>;
}

pub struct SqliteIngestionEventRepo { pool: std::sync::Arc<Pool> }
impl SqliteIngestionEventRepo {
    pub fn new(pool: std::sync::Arc<Pool>) -> Self { Self { pool } }
}

fn map_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<IngestionEvent> {
    Ok(IngestionEvent {
        id: r.get(0)?,
        cik: r.get::<_, Option<String>>(1)?.map(Cik),
        accession_no: r.get::<_, Option<String>>(2)?.map(AccessionNo),
        stage: r.get(3)?,
        level: Severity::from_str(&r.get::<_, String>(4)?).unwrap_or(Severity::Info),
        user_visible: r.get::<_, i64>(5)? != 0,
        message: r.get(6)?,
        detail_json: r.get(7)?,
        occurred_at: r.get(8)?,
    })
}

const COLS: &str = "id, cik, accession_no, stage, level, user_visible, message, detail_json, occurred_at";

#[async_trait]
impl IngestionEventRepo for SqliteIngestionEventRepo {
    async fn record(&self, e: &IngestionEvent) -> Result<i64, RepoError> {
        let mut g = self.pool.write().await;
        g.conn().execute(
            "INSERT INTO ingestion_event
              (cik, accession_no, stage, level, user_visible, message, detail_json, occurred_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                e.cik.as_ref().map(|c| &c.0),
                e.accession_no.as_ref().map(|a| &a.0),
                e.stage,
                e.level.as_str(),
                e.user_visible as i64,
                e.message,
                e.detail_json,
                e.occurred_at,
            ],
        )?;
        Ok(g.conn().last_insert_rowid())
    }

    async fn recent(&self, cik: Option<&Cik>, limit: u32) -> Result<Vec<IngestionEvent>, RepoError> {
        let g = self.pool.read()?;
        let q = match cik {
            Some(_) => format!("SELECT {COLS} FROM ingestion_event WHERE cik = ?1 ORDER BY occurred_at DESC LIMIT ?2"),
            None => format!("SELECT {COLS} FROM ingestion_event ORDER BY occurred_at DESC LIMIT ?1"),
        };
        let mut stmt = g.conn().prepare(&q)?;
        let mut out = Vec::new();
        match cik {
            Some(c) => {
                let rows = stmt.query_map(rusqlite::params![c.0, limit as i64], map_row)?;
                for r in rows { out.push(r?); }
            }
            None => {
                let rows = stmt.query_map([limit as i64], map_row)?;
                for r in rows { out.push(r?); }
            }
        }
        Ok(out)
    }

    async fn user_visible(&self, cik: &Cik) -> Result<Vec<IngestionEvent>, RepoError> {
        let g = self.pool.read()?;
        let q = format!(
            "SELECT {COLS} FROM ingestion_event
             WHERE cik = ?1 AND user_visible = 1 ORDER BY occurred_at DESC"
        );
        let mut stmt = g.conn().prepare(&q)?;
        let rows = stmt.query_map([&cik.0], map_row)?;
        let mut out = Vec::new();
        for r in rows { out.push(r?); }
        Ok(out)
    }
}
