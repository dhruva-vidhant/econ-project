//! Company repository — M06.

use async_trait::async_trait;
use chrono::Utc;

use crate::db::Pool;
use crate::domain::{Cik, Company, Ticker};
use crate::errors::RepoError;

#[async_trait]
pub trait CompanyRepo: Send + Sync {
    async fn upsert(&self, c: &Company) -> Result<(), RepoError>;
    async fn get_by_cik(&self, cik: &Cik) -> Result<Option<Company>, RepoError>;
    async fn get_by_ticker(&self, t: &Ticker) -> Result<Option<Company>, RepoError>;
    async fn list_saved(&self) -> Result<Vec<Company>, RepoError>;
    async fn remove(&self, cik: &Cik, drop_cache: bool) -> Result<(), RepoError>;
    async fn touch_refreshed(&self, cik: &Cik) -> Result<(), RepoError>;
}

pub struct SqliteCompanyRepo {
    pool: std::sync::Arc<Pool>,
}

impl SqliteCompanyRepo {
    pub fn new(pool: std::sync::Arc<Pool>) -> Self { Self { pool } }
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Company> {
    Ok(Company {
        cik: Cik(row.get(0)?),
        ticker: Ticker(row.get(1)?),
        name: row.get(2)?,
        exchange: row.get(3)?,
        sic: row.get(4)?,
        fiscal_year_end: row.get(5)?,
        added_at: row.get::<_, chrono::DateTime<Utc>>(6)?,
        last_refreshed: row.get(7)?,
    })
}

const COLS: &str =
    "cik, ticker, name, exchange, sic, fiscal_year_end, added_at, last_refreshed";

#[async_trait]
impl CompanyRepo for SqliteCompanyRepo {
    async fn upsert(&self, c: &Company) -> Result<(), RepoError> {
        let mut g = self.pool.write().await;
        g.conn().execute(
            "INSERT INTO company (cik, ticker, name, exchange, sic, fiscal_year_end, added_at, last_refreshed)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(cik) DO UPDATE SET
                ticker=excluded.ticker,
                name=excluded.name,
                exchange=excluded.exchange,
                sic=excluded.sic,
                fiscal_year_end=excluded.fiscal_year_end,
                last_refreshed=excluded.last_refreshed",
            rusqlite::params![
                c.cik.0,
                c.ticker.0,
                c.name,
                c.exchange,
                c.sic,
                c.fiscal_year_end,
                c.added_at,
                c.last_refreshed,
            ],
        )?;
        Ok(())
    }

    async fn get_by_cik(&self, cik: &Cik) -> Result<Option<Company>, RepoError> {
        let g = self.pool.read()?;
        let q = format!("SELECT {COLS} FROM company WHERE cik = ?1");
        let mut stmt = g.conn().prepare(&q)?;
        let rs = stmt.query_row([&cik.0], map_row);
        match rs {
            Ok(c) => Ok(Some(c)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn get_by_ticker(&self, t: &Ticker) -> Result<Option<Company>, RepoError> {
        let g = self.pool.read()?;
        let q = format!("SELECT {COLS} FROM company WHERE ticker = ?1");
        let mut stmt = g.conn().prepare(&q)?;
        let rs = stmt.query_row([&t.0], map_row);
        match rs {
            Ok(c) => Ok(Some(c)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn list_saved(&self) -> Result<Vec<Company>, RepoError> {
        let g = self.pool.read()?;
        let q = format!("SELECT {COLS} FROM company ORDER BY ticker");
        let mut stmt = g.conn().prepare(&q)?;
        let rows = stmt.query_map([], map_row)?;
        let mut out = Vec::new();
        for r in rows { out.push(r?); }
        Ok(out)
    }

    async fn remove(&self, cik: &Cik, drop_cache: bool) -> Result<(), RepoError> {
        let mut g = self.pool.write().await;
        if drop_cache {
            // Drop dependent rows in FK-safe order.
            let tx = g.conn().transaction()?;
            tx.execute("DELETE FROM ingestion_event WHERE cik = ?1", [&cik.0])?;
            tx.execute("DELETE FROM derived_metric WHERE cik = ?1", [&cik.0])?;
            tx.execute("DELETE FROM normalized_fact WHERE cik = ?1", [&cik.0])?;
            tx.execute("DELETE FROM raw_fact WHERE cik = ?1", [&cik.0])?;
            tx.execute("DELETE FROM amendment_coverage_gap WHERE cik = ?1", [&cik.0])?;
            tx.execute(
                "DELETE FROM restatement_resolved_by
                 WHERE restatement_announcement_id IN
                   (SELECT id FROM restatement_announcement WHERE cik = ?1)",
                [&cik.0],
            )?;
            tx.execute("DELETE FROM restatement_announcement WHERE cik = ?1", [&cik.0])?;
            tx.execute("DELETE FROM historical_price WHERE cik = ?1", [&cik.0])?;
            tx.execute("DELETE FROM period WHERE cik = ?1", [&cik.0])?;
            tx.execute("DELETE FROM filing WHERE cik = ?1", [&cik.0])?;
            tx.execute("DELETE FROM company WHERE cik = ?1", [&cik.0])?;
            tx.commit()?;
        } else {
            g.conn().execute("DELETE FROM company WHERE cik = ?1", [&cik.0])?;
        }
        Ok(())
    }

    async fn touch_refreshed(&self, cik: &Cik) -> Result<(), RepoError> {
        let mut g = self.pool.write().await;
        g.conn().execute(
            "UPDATE company SET last_refreshed = ?1 WHERE cik = ?2",
            rusqlite::params![Utc::now(), &cik.0],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    async fn setup() -> std::sync::Arc<Pool> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.sqlite");
        Box::leak(Box::new(dir));
        std::sync::Arc::new(Pool::open(&path).unwrap())
    }

    fn sample_company() -> Company {
        Company {
            cik: Cik("0000320193".into()),
            ticker: Ticker("AAPL".into()),
            name: "Apple Inc.".into(),
            exchange: Some("Nasdaq".into()),
            sic: Some("3571".into()),
            fiscal_year_end: Some("0926".into()),
            added_at: Utc.with_ymd_and_hms(2024, 5, 10, 12, 0, 0).unwrap(),
            last_refreshed: None,
        }
    }

    #[tokio::test]
    async fn upsert_and_get_by_cik() {
        let pool = setup().await;
        let repo = SqliteCompanyRepo::new(pool);
        let c = sample_company();
        repo.upsert(&c).await.unwrap();
        let got = repo.get_by_cik(&c.cik).await.unwrap().unwrap();
        assert_eq!(got.ticker.0, "AAPL");
    }

    #[tokio::test]
    async fn list_saved_returns_inserted() {
        let pool = setup().await;
        let repo = SqliteCompanyRepo::new(pool);
        repo.upsert(&sample_company()).await.unwrap();
        let v = repo.list_saved().await.unwrap();
        assert_eq!(v.len(), 1);
    }

    #[tokio::test]
    async fn get_by_ticker_returns_none_for_unknown() {
        let pool = setup().await;
        let repo = SqliteCompanyRepo::new(pool);
        let v = repo.get_by_ticker(&Ticker("XYZ".into())).await.unwrap();
        assert!(v.is_none());
    }

    #[tokio::test]
    async fn remove_with_drop_cache_clears_company() {
        let pool = setup().await;
        let repo = SqliteCompanyRepo::new(pool);
        let c = sample_company();
        repo.upsert(&c).await.unwrap();
        repo.remove(&c.cik, true).await.unwrap();
        assert!(repo.get_by_cik(&c.cik).await.unwrap().is_none());
    }
}
