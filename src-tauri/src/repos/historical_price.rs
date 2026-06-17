//! Historical price repository — M12b.
//!
//! Stores one end-of-day USD close per `(cik, date)`. The ingestion pipeline
//! writes one row per distinct period end-date (resolved to the nearest prior
//! trading day); the read path looks those closes up by `period.end_date` to
//! derive market cap. `close_micro` is USD × 1,000,000 (per architecture §6.2).

use std::collections::BTreeMap;

use async_trait::async_trait;
use chrono::NaiveDate;

use crate::db::Pool;
use crate::domain::Cik;
use crate::errors::RepoError;

#[async_trait]
pub trait HistoricalPriceRepo: Send + Sync {
    /// Insert or refresh the close for `(cik, date)`. Idempotent on re-ingest.
    async fn upsert(
        &self,
        cik: &Cik,
        date: NaiveDate,
        ticker: &str,
        close_micro: i64,
        source: &str,
    ) -> Result<(), RepoError>;

    /// All stored closes for a company, keyed by date (ascending).
    async fn map_for(&self, cik: &Cik) -> Result<BTreeMap<NaiveDate, i64>, RepoError>;
}

pub struct SqliteHistoricalPriceRepo {
    pool: std::sync::Arc<Pool>,
}

impl SqliteHistoricalPriceRepo {
    pub fn new(pool: std::sync::Arc<Pool>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HistoricalPriceRepo for SqliteHistoricalPriceRepo {
    async fn upsert(
        &self,
        cik: &Cik,
        date: NaiveDate,
        ticker: &str,
        close_micro: i64,
        source: &str,
    ) -> Result<(), RepoError> {
        let mut g = self.pool.write().await;
        g.conn().execute(
            "INSERT INTO historical_price (cik, date, ticker, close_micro, source, ingested_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(cik, date) DO UPDATE SET
                ticker = excluded.ticker, close_micro = excluded.close_micro,
                source = excluded.source, ingested_at = excluded.ingested_at",
            rusqlite::params![cik.0, date, ticker, close_micro, source, chrono::Utc::now()],
        )?;
        Ok(())
    }

    async fn map_for(&self, cik: &Cik) -> Result<BTreeMap<NaiveDate, i64>, RepoError> {
        let g = self.pool.read()?;
        let mut stmt = g
            .conn()
            .prepare("SELECT date, close_micro FROM historical_price WHERE cik = ?1 ORDER BY date")?;
        let rows = stmt.query_map(rusqlite::params![cik.0], |r| {
            Ok((r.get::<_, NaiveDate>(0)?, r.get::<_, i64>(1)?))
        })?;
        let mut out = BTreeMap::new();
        for r in rows {
            let (d, v) = r?;
            out.insert(d, v);
        }
        Ok(out)
    }
}
