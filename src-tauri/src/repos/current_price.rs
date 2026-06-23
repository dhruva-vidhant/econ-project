//! Current spot price repository.
//!
//! Stores exactly one live spot price per company for computing
//! current_market_cap and current_free_cash_flow_yield. The ingestion pipeline
//! fetches and upserts the spot price at refresh time; read paths derive the
//! live metrics from the persisted price without network I/O.

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::db::Pool;
use crate::domain::Cik;
use crate::errors::RepoError;

#[derive(Debug, Clone, PartialEq)]
pub struct CurrentPrice {
    pub price_micro: i64,
    pub as_of: DateTime<Utc>,
    pub ticker: String,
}

#[async_trait]
pub trait CurrentPriceRepo: Send + Sync {
    /// Insert or update the spot price for a company. Idempotent on re-ingest.
    async fn upsert(
        &self,
        cik: &Cik,
        ticker: &str,
        price_micro: i64,
        as_of: DateTime<Utc>,
        source: &str,
    ) -> Result<(), RepoError>;

    /// Fetch the stored spot price for a company.
    async fn get(&self, cik: &Cik) -> Result<Option<CurrentPrice>, RepoError>;
}

pub struct SqliteCurrentPriceRepo {
    pool: std::sync::Arc<Pool>,
}

impl SqliteCurrentPriceRepo {
    pub fn new(pool: std::sync::Arc<Pool>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CurrentPriceRepo for SqliteCurrentPriceRepo {
    async fn upsert(
        &self,
        cik: &Cik,
        ticker: &str,
        price_micro: i64,
        as_of: DateTime<Utc>,
        source: &str,
    ) -> Result<(), RepoError> {
        let mut g = self.pool.write().await;
        g.conn().execute(
            "INSERT INTO current_price (cik, ticker, price_micro, as_of, source)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(cik) DO UPDATE SET
                ticker = excluded.ticker, price_micro = excluded.price_micro,
                as_of = excluded.as_of, source = excluded.source",
            rusqlite::params![cik.0, ticker, price_micro, as_of, source],
        )?;
        Ok(())
    }

    async fn get(&self, cik: &Cik) -> Result<Option<CurrentPrice>, RepoError> {
        let g = self.pool.read()?;
        let result = g.conn().query_row(
            "SELECT ticker, price_micro, as_of FROM current_price WHERE cik = ?1",
            rusqlite::params![cik.0],
            |r| {
                Ok(CurrentPrice {
                    ticker: r.get(0)?,
                    price_micro: r.get(1)?,
                    as_of: r.get(2)?,
                })
            },
        );
        match result {
            Ok(p) => Ok(Some(p)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tempfile::NamedTempFile;
    use crate::domain::{Company, Ticker};
    use crate::repos::company::{CompanyRepo, SqliteCompanyRepo};

    fn temp_pool() -> std::sync::Arc<Pool> {
        let tmp = NamedTempFile::new().unwrap();
        std::sync::Arc::new(Pool::open(tmp.path()).unwrap())
    }

    #[tokio::test]
    async fn upsert_then_get() {
        let pool = temp_pool();
        let companies = SqliteCompanyRepo::new(pool.clone());
        let repo = SqliteCurrentPriceRepo::new(pool);
        let cik = Cik("0001234567".into());
        let now = Utc::now();

        // Insert a company so the foreign-key constraint is satisfied.
        companies.upsert(&Company {
            cik: cik.clone(),
            ticker: Ticker("AAPL".into()),
            name: "Apple Inc.".into(),
            exchange: Some("NASDAQ".into()),
            sic: None,
            fiscal_year_end: Some("0930".into()),
            added_at: now,
            last_refreshed: None,
        }).await.unwrap();

        assert_eq!(repo.get(&cik).await.unwrap(), None);

        repo.upsert(&cik, "AAPL", 180_500_000, now, "yahoo")
            .await
            .unwrap();

        let price = repo.get(&cik).await.unwrap().unwrap();
        assert_eq!(price.ticker, "AAPL");
        assert_eq!(price.price_micro, 180_500_000);
    }

    #[tokio::test]
    async fn upsert_is_idempotent() {
        let pool = temp_pool();
        let companies = SqliteCompanyRepo::new(pool.clone());
        let repo = SqliteCurrentPriceRepo::new(pool);
        let cik = Cik("0001234567".into());
        let now = Utc::now();

        companies.upsert(&Company {
            cik: cik.clone(),
            ticker: Ticker("AAPL".into()),
            name: "Apple Inc.".into(),
            exchange: Some("NASDAQ".into()),
            sic: None,
            fiscal_year_end: Some("0930".into()),
            added_at: now,
            last_refreshed: None,
        }).await.unwrap();

        repo.upsert(&cik, "AAPL", 180_500_000, now, "yahoo")
            .await
            .unwrap();
        repo.upsert(&cik, "AAPL", 181_000_000, now, "yahoo")
            .await
            .unwrap();

        let price = repo.get(&cik).await.unwrap().unwrap();
        assert_eq!(price.price_micro, 181_000_000);
    }
}
