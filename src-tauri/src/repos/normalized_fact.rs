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

        // Idempotent re-ingestion: if a normalized_fact already exists for
        // exactly this (cik, metric, period_id, source_fact_id), the same
        // raw fact is mapping to the same period — there is nothing new
        // to record. Returning the existing id avoids a UNIQUE violation
        // on the (cik, metric, period_id, source_fact_id) constraint and
        // makes second-and-subsequent ingests no-ops at this layer.
        let exact_existing: Option<i64> = tx
            .query_row(
                "SELECT id FROM normalized_fact
                 WHERE cik = ?1 AND metric = ?2 AND period_id = ?3
                   AND source_fact_id = ?4",
                rusqlite::params![n.cik.0, n.metric.as_str(), n.period_id, n.source_fact_id],
                |r| r.get(0),
            )
            .ok();
        if let Some(existing_id) = exact_existing {
            tx.commit()?;
            return Ok(existing_id);
        }

        // Find the current primary for (cik, metric, period_id), if any.
        let prev: Option<i64> = tx
            .query_row(
                "SELECT id FROM normalized_fact
                 WHERE cik = ?1 AND metric = ?2 AND period_id = ?3
                   AND is_primary = 1 AND superseded_by IS NULL",
                rusqlite::params![n.cik.0, n.metric.as_str(), n.period_id],
                |r| r.get(0),
            )
            .ok();

        // The partial unique index `idx_norm_primary_current` is keyed on
        // (cik, metric, period_id) WHERE is_primary=1 AND superseded_by IS NULL.
        // If we INSERT the new row before clearing the old one out of that
        // index, the constraint fires and the INSERT fails.
        //
        // SQLite checks UNIQUE constraints per-statement (no deferred mode
        // for partial indexes), so we cannot fix it up post-INSERT in the
        // same transaction. Demote the previous primary first, INSERT the
        // new primary, then restore the previous row's `is_primary=1`
        // alongside its `superseded_by` link — neither flag alone puts it
        // back into the partial index.
        if let Some(prev_id) = prev {
            tx.execute(
                "UPDATE normalized_fact SET is_primary = 0 WHERE id = ?1",
                rusqlite::params![prev_id],
            )?;
        }

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

        if let Some(prev_id) = prev {
            tx.execute(
                "UPDATE normalized_fact
                 SET is_primary = 1, superseded_by = ?1
                 WHERE id = ?2",
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
        let q =
            "SELECT
              p.id, p.cik, p.fiscal_year, p.fiscal_quarter, p.fiscal_year_end,
              p.start_date, p.end_date, p.kind, p.is_53_week,
              n.id, n.cik, n.metric, n.period_id, n.value, n.unit, n.source_fact_id,
              n.source_kind, n.is_primary, n.original_value, n.original_unit,
              n.fx_rate_micro, n.fx_rate_source, n.fx_rate_date, n.superseded_by
             FROM normalized_fact n
             JOIN period p ON p.id = n.period_id
             WHERE n.cik = ?1 AND n.metric = ?2 AND p.kind = ?3
               AND n.is_primary = 1 AND n.superseded_by IS NULL
             ORDER BY p.start_date".to_string();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{AccessionNo, Filing, FormType, Period, PeriodKind, RawFact};
    use crate::repos::company::{CompanyRepo, SqliteCompanyRepo};
    use crate::repos::filing::{FilingRepo, SqliteFilingRepo};
    use crate::repos::period::{PeriodRepo, SqlitePeriodRepo};
    use crate::repos::raw_fact::{RawFactRepo, SqliteRawFactRepo};
    use chrono::{NaiveDate, Utc};

    async fn setup() -> std::sync::Arc<Pool> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.sqlite");
        Box::leak(Box::new(dir));
        std::sync::Arc::new(Pool::open(&path).unwrap())
    }

    async fn seed_company(pool: &std::sync::Arc<Pool>) -> Cik {
        let repo = SqliteCompanyRepo::new(pool.clone());
        let cik = Cik("0000320193".into());
        let c = crate::domain::Company {
            cik: cik.clone(),
            ticker: crate::domain::Ticker("AAPL".into()),
            name: "Apple Inc.".into(),
            exchange: None, sic: None, fiscal_year_end: Some("0930".into()),
            added_at: Utc::now(), last_refreshed: None,
        };
        repo.upsert(&c).await.unwrap();
        cik
    }

    async fn seed_filing(pool: &std::sync::Arc<Pool>, cik: &Cik, accn: &str) -> AccessionNo {
        let repo = SqliteFilingRepo::new(pool.clone());
        let accn = AccessionNo(accn.to_string());
        let f = Filing {
            accession_no: accn.clone(),
            cik: cik.clone(),
            form_type: FormType::TenK,
            filed_at: NaiveDate::from_ymd_opt(2024, 11, 1).unwrap(),
            period_of_report: Some(NaiveDate::from_ymd_opt(2024, 9, 30).unwrap()),
            is_amendment: false,
            amends: None,
            item_4_02_8k: false,
        };
        repo.upsert(&f).await.unwrap();
        accn
    }

    async fn seed_period(pool: &std::sync::Arc<Pool>, cik: &Cik, fy: i32, fq: u8) -> i64 {
        let repo = SqlitePeriodRepo::new(pool.clone());
        let p = Period {
            id: 0, cik: cik.clone(),
            fiscal_year: fy, fiscal_quarter: fq,
            fiscal_year_end: "0930".into(),
            start_date: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            end_date: NaiveDate::from_ymd_opt(2024, 9, 30).unwrap(),
            kind: if fq == 0 { PeriodKind::Annual } else { PeriodKind::Quarterly },
            is_53_week: false,
        };
        repo.upsert_returning_id(&p).await.unwrap()
    }

    async fn seed_raw_fact(
        pool: &std::sync::Arc<Pool>,
        cik: &Cik,
        accn: &AccessionNo,
        fp: &str,
        value: i64,
    ) -> i64 {
        let repo = SqliteRawFactRepo::new(pool.clone());
        let f = RawFact {
            id: 0,
            cik: cik.clone(),
            accession_no: accn.clone(),
            taxonomy: "us-gaap".into(),
            concept: "Revenues".into(),
            unit: "USD".into(),
            value_numeric: value,
            period_start: Some(NaiveDate::from_ymd_opt(2024, 1, 1).unwrap()),
            period_end: NaiveDate::from_ymd_opt(2024, 9, 30).unwrap(),
            is_instant: false,
            fy: Some(2024),
            fp: Some(fp.into()),
            filed: Some(NaiveDate::from_ymd_opt(2024, 11, 1).unwrap()),
            source_kind: SourceKind::XbrlApi,
            ingested_at: Utc::now(),
        };
        repo.upsert_many(&[f]).await.unwrap();
        // The raw_fact's auto-incremented id is the only row in the table.
        let g = pool.read().unwrap();
        g.conn()
            .query_row(
                "SELECT id FROM raw_fact WHERE accession_no = ?1 AND fp = ?2",
                rusqlite::params![accn.0, fp],
                |r| r.get::<_, i64>(0),
            )
            .unwrap()
    }

    fn nf(cik: &Cik, period_id: i64, source_fact_id: i64, value: i64) -> NormalizedFact {
        NormalizedFact {
            id: 0,
            cik: cik.clone(),
            metric: Metric::Revenue,
            period_id,
            value,
            unit: "USD".into(),
            source_fact_id,
            source_kind: SourceKind::XbrlApi,
            is_primary: true,
            original_value: None, original_unit: None,
            fx_rate_micro: None, fx_rate_source: None, fx_rate_date: None,
            superseded_by: None,
        }
    }

    #[tokio::test]
    async fn re_inserting_same_source_fact_is_idempotent() {
        // Re-ingesting the exact same raw fact (same id) for the same period
        // must not fail and must not create a duplicate row.
        let pool = setup().await;
        let cik = seed_company(&pool).await;
        let accn = seed_filing(&pool, &cik, "0000320193-24-000001").await;
        let period_id = seed_period(&pool, &cik, 2024, 0).await;
        let raw_id = seed_raw_fact(&pool, &cik, &accn, "FY", 1_000).await;

        let repo = SqliteNormalizedFactRepo::new(pool.clone());
        let first = repo
            .insert_primary_with_supersession(&nf(&cik, period_id, raw_id, 1_000))
            .await
            .unwrap();
        let second = repo
            .insert_primary_with_supersession(&nf(&cik, period_id, raw_id, 1_000))
            .await
            .unwrap();
        assert_eq!(first, second, "second call should return the existing id");

        let count: i64 = pool
            .read()
            .unwrap()
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM normalized_fact WHERE cik = ?1 AND metric = ?2 AND period_id = ?3",
                rusqlite::params![cik.0, "revenue", period_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn supersession_keeps_partial_unique_index_satisfied() {
        // Two different raw facts (e.g., original 10-K and an amendment)
        // map to the same (cik, metric, period_id). Inserting the second
        // must succeed: the partial unique index on
        // (cik, metric, period_id) WHERE is_primary=1 AND superseded_by IS NULL
        // would fire if both rows were marked current at any point.
        let pool = setup().await;
        let cik = seed_company(&pool).await;
        let accn1 = seed_filing(&pool, &cik, "0000320193-24-000001").await;
        let accn2 = seed_filing(&pool, &cik, "0000320193-24-000002").await;
        let period_id = seed_period(&pool, &cik, 2024, 0).await;
        let raw_a = seed_raw_fact(&pool, &cik, &accn1, "FY", 1_000).await;
        let raw_b = seed_raw_fact(&pool, &cik, &accn2, "FY", 1_100).await;

        let repo = SqliteNormalizedFactRepo::new(pool.clone());
        let id_a = repo
            .insert_primary_with_supersession(&nf(&cik, period_id, raw_a, 1_000))
            .await
            .unwrap();
        let id_b = repo
            .insert_primary_with_supersession(&nf(&cik, period_id, raw_b, 1_100))
            .await
            .unwrap();
        assert_ne!(id_a, id_b);

        let current = repo
            .current_value(&cik, Metric::Revenue, period_id)
            .await
            .unwrap()
            .expect("current value should be the second insert");
        assert_eq!(current.id, id_b);
        assert_eq!(current.value, 1_100);

        // The previous row stays is_primary=1 with superseded_by pointing
        // at the new row, so the supersession chain walk finds it.
        let chain = repo.supersession_chain(id_b).await.unwrap();
        assert_eq!(chain.len(), 1);
        assert_eq!(chain[0].id, id_a);
        assert_eq!(chain[0].superseded_by, Some(id_b));
    }

    #[tokio::test]
    async fn three_way_supersession_chain() {
        // A → B → C: each newer raw fact supersedes the prior current.
        let pool = setup().await;
        let cik = seed_company(&pool).await;
        let accn = seed_filing(&pool, &cik, "0000320193-24-000001").await;
        let period_id = seed_period(&pool, &cik, 2024, 0).await;
        let raw_a = seed_raw_fact(&pool, &cik, &accn, "Q1", 100).await;
        let raw_b = seed_raw_fact(&pool, &cik, &accn, "Q2", 200).await;
        let raw_c = seed_raw_fact(&pool, &cik, &accn, "Q3", 300).await;

        let repo = SqliteNormalizedFactRepo::new(pool.clone());
        let a = repo.insert_primary_with_supersession(&nf(&cik, period_id, raw_a, 100)).await.unwrap();
        let b = repo.insert_primary_with_supersession(&nf(&cik, period_id, raw_b, 200)).await.unwrap();
        let c = repo.insert_primary_with_supersession(&nf(&cik, period_id, raw_c, 300)).await.unwrap();

        let current = repo.current_value(&cik, Metric::Revenue, period_id).await.unwrap().unwrap();
        assert_eq!(current.id, c);

        let chain = repo.supersession_chain(c).await.unwrap();
        assert_eq!(chain.len(), 2);
        assert_eq!(chain[0].id, a, "oldest first");
        assert_eq!(chain[1].id, b);
    }
}
