//! Custom-metric repository — V2 (custom metrics + CSV export).

use async_trait::async_trait;
use chrono::Utc;

use crate::db::Pool;
use crate::domain::{
    Cik, CustomMetric, CustomMetricPatch, CustomMetricPoint, NewCustomMetric, Period, PeriodKind,
    UnitKind,
};
use crate::errors::RepoError;

#[async_trait]
pub trait CustomMetricRepo: Send + Sync {
    async fn list(&self, cik: &Cik) -> Result<Vec<CustomMetric>, RepoError>;
    async fn get(&self, id: i64) -> Result<Option<CustomMetric>, RepoError>;
    async fn get_by_key(&self, cik: &Cik, key: &str) -> Result<Option<CustomMetric>, RepoError>;
    async fn create(&self, cik: &Cik, def: &NewCustomMetric) -> Result<CustomMetric, RepoError>;
    async fn update(&self, id: i64, patch: &CustomMetricPatch) -> Result<CustomMetric, RepoError>;
    async fn delete(&self, id: i64) -> Result<(), RepoError>;
    async fn count_points(&self, id: i64) -> Result<i64, RepoError>;

    /// Upsert by (metric, period). `period_id=None` stores an unaligned point
    /// keyed by `observed_date`. Returns the stored row.
    async fn upsert_point(
        &self,
        metric_id: i64,
        period_id: Option<i64>,
        observed_date: Option<&str>,
        value_micro: i64,
        note: Option<&str>,
    ) -> Result<CustomMetricPoint, RepoError>;
    async fn delete_point(&self, point_id: i64) -> Result<(), RepoError>;

    /// Aligned points for a metric, joined to their `Period`, filtered by
    /// `kind`, sorted by `Period.end_date`.
    async fn points_aligned(
        &self,
        metric_id: i64,
        kind: PeriodKind,
    ) -> Result<Vec<(Period, CustomMetricPoint)>, RepoError>;

    /// All unaligned points for a company's metrics (for re-resolution).
    async fn points_unaligned(&self, cik: &Cik) -> Result<Vec<CustomMetricPoint>, RepoError>;
    async fn realign_point(&self, point_id: i64, period_id: i64) -> Result<(), RepoError>;
}

pub struct SqliteCustomMetricRepo {
    pool: std::sync::Arc<Pool>,
}

impl SqliteCustomMetricRepo {
    pub fn new(pool: std::sync::Arc<Pool>) -> Self {
        Self { pool }
    }
}

const METRIC_COLS: &str =
    "id, cik, key, label, unit_kind, currency, decimals, description, created_at, updated_at";

fn map_metric_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CustomMetric> {
    Ok(CustomMetric {
        id: row.get(0)?,
        cik: Cik(row.get(1)?),
        key: row.get(2)?,
        label: row.get(3)?,
        unit_kind: UnitKind::from_str(&row.get::<_, String>(4)?).unwrap_or(UnitKind::Number),
        currency: row.get(5)?,
        decimals: row.get::<_, i64>(6)? as u8,
        description: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

const POINT_COLS: &str =
    "id, custom_metric_id, period_id, observed_date, value_micro, source_note, imported_at";

fn map_point_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CustomMetricPoint> {
    Ok(CustomMetricPoint {
        id: row.get(0)?,
        custom_metric_id: row.get(1)?,
        period_id: row.get(2)?,
        observed_date: row.get(3)?,
        value_micro: row.get(4)?,
        source_note: row.get(5)?,
        imported_at: row.get(6)?,
    })
}

const PERIOD_COLS: &str =
    "p.id, p.cik, p.fiscal_year, p.fiscal_quarter, p.fiscal_year_end, p.start_date, p.end_date, p.kind, p.is_53_week";

fn map_period_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Period> {
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

#[async_trait]
impl CustomMetricRepo for SqliteCustomMetricRepo {
    async fn list(&self, cik: &Cik) -> Result<Vec<CustomMetric>, RepoError> {
        let g = self.pool.read()?;
        let q = format!("SELECT {METRIC_COLS} FROM custom_metric WHERE cik = ?1 ORDER BY key");
        let mut stmt = g.conn().prepare(&q)?;
        let rows = stmt.query_map([&cik.0], map_metric_row)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    async fn get(&self, id: i64) -> Result<Option<CustomMetric>, RepoError> {
        let g = self.pool.read()?;
        let q = format!("SELECT {METRIC_COLS} FROM custom_metric WHERE id = ?1");
        let mut stmt = g.conn().prepare(&q)?;
        match stmt.query_row([id], map_metric_row) {
            Ok(m) => Ok(Some(m)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn get_by_key(&self, cik: &Cik, key: &str) -> Result<Option<CustomMetric>, RepoError> {
        let g = self.pool.read()?;
        let q = format!("SELECT {METRIC_COLS} FROM custom_metric WHERE cik = ?1 AND key = ?2");
        let mut stmt = g.conn().prepare(&q)?;
        match stmt.query_row(rusqlite::params![cik.0, key], map_metric_row) {
            Ok(m) => Ok(Some(m)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn create(&self, cik: &Cik, def: &NewCustomMetric) -> Result<CustomMetric, RepoError> {
        let now = Utc::now().to_rfc3339();
        let mut g = self.pool.write().await;
        let decimals = def.decimals.unwrap_or(2);
        let id = {
            let conn = g.conn();
            conn.execute(
                "INSERT INTO custom_metric (cik, key, label, unit_kind, currency, decimals, description, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                rusqlite::params![
                    cik.0,
                    def.key,
                    def.label,
                    def.unit_kind.as_str(),
                    def.currency,
                    decimals as i64,
                    def.description,
                    now,
                    now,
                ],
            )?;
            conn.last_insert_rowid()
        };
        drop(g);
        self.get(id).await?.ok_or(RepoError::NotFound)
    }

    async fn update(&self, id: i64, patch: &CustomMetricPatch) -> Result<CustomMetric, RepoError> {
        let existing = self.get(id).await?.ok_or(RepoError::NotFound)?;
        let label = patch.label.clone().unwrap_or(existing.label);
        let unit_kind = patch.unit_kind.unwrap_or(existing.unit_kind);
        let currency = patch.currency.clone().or(existing.currency);
        let decimals = patch.decimals.unwrap_or(existing.decimals);
        let description = patch.description.clone().or(existing.description);
        let now = Utc::now().to_rfc3339();
        let mut g = self.pool.write().await;
        g.conn().execute(
            "UPDATE custom_metric SET label=?1, unit_kind=?2, currency=?3, decimals=?4, description=?5, updated_at=?6
             WHERE id=?7",
            rusqlite::params![label, unit_kind.as_str(), currency, decimals as i64, description, now, id],
        )?;
        drop(g);
        self.get(id).await?.ok_or(RepoError::NotFound)
    }

    async fn delete(&self, id: i64) -> Result<(), RepoError> {
        let mut g = self.pool.write().await;
        let tx = g.conn().transaction()?;
        tx.execute("DELETE FROM custom_metric_point WHERE custom_metric_id = ?1", [id])?;
        tx.execute("DELETE FROM custom_metric WHERE id = ?1", [id])?;
        tx.commit()?;
        Ok(())
    }

    async fn count_points(&self, id: i64) -> Result<i64, RepoError> {
        let g = self.pool.read()?;
        let n: i64 = g.conn().query_row(
            "SELECT COUNT(*) FROM custom_metric_point WHERE custom_metric_id = ?1",
            [id],
            |r| r.get(0),
        )?;
        Ok(n)
    }

    async fn upsert_point(
        &self,
        metric_id: i64,
        period_id: Option<i64>,
        observed_date: Option<&str>,
        value_micro: i64,
        note: Option<&str>,
    ) -> Result<CustomMetricPoint, RepoError> {
        let now = Utc::now().to_rfc3339();
        let mut g = self.pool.write().await;
        let id = {
            let conn = g.conn();
            match period_id {
                Some(pid) => {
                    conn.execute(
                        "INSERT INTO custom_metric_point (custom_metric_id, period_id, observed_date, value_micro, source_note, imported_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                         ON CONFLICT(custom_metric_id, period_id) DO UPDATE SET
                            value_micro=excluded.value_micro,
                            observed_date=excluded.observed_date,
                            source_note=excluded.source_note,
                            imported_at=excluded.imported_at",
                        rusqlite::params![metric_id, pid, observed_date, value_micro, note, now],
                    )?;
                    conn.query_row(
                        "SELECT id FROM custom_metric_point WHERE custom_metric_id = ?1 AND period_id = ?2",
                        rusqlite::params![metric_id, pid],
                        |r| r.get::<_, i64>(0),
                    )?
                }
                None => {
                    conn.execute(
                        "INSERT INTO custom_metric_point (custom_metric_id, period_id, observed_date, value_micro, source_note, imported_at)
                         VALUES (?1, NULL, ?2, ?3, ?4, ?5)
                         ON CONFLICT(custom_metric_id, observed_date) DO UPDATE SET
                            value_micro=excluded.value_micro,
                            source_note=excluded.source_note,
                            imported_at=excluded.imported_at",
                        rusqlite::params![metric_id, observed_date, value_micro, note, now],
                    )?;
                    conn.query_row(
                        "SELECT id FROM custom_metric_point WHERE custom_metric_id = ?1 AND observed_date = ?2",
                        rusqlite::params![metric_id, observed_date],
                        |r| r.get::<_, i64>(0),
                    )?
                }
            }
        };
        drop(g);
        let g = self.pool.read()?;
        let q = format!("SELECT {POINT_COLS} FROM custom_metric_point WHERE id = ?1");
        let mut stmt = g.conn().prepare(&q)?;
        stmt.query_row([id], map_point_row).map_err(Into::into)
    }

    async fn delete_point(&self, point_id: i64) -> Result<(), RepoError> {
        let mut g = self.pool.write().await;
        g.conn().execute("DELETE FROM custom_metric_point WHERE id = ?1", [point_id])?;
        Ok(())
    }

    async fn points_aligned(
        &self,
        metric_id: i64,
        kind: PeriodKind,
    ) -> Result<Vec<(Period, CustomMetricPoint)>, RepoError> {
        let g = self.pool.read()?;
        let q = format!(
            "SELECT {PERIOD_COLS}, {point_cols}
             FROM custom_metric_point cmp
             JOIN period p ON p.id = cmp.period_id
             WHERE cmp.custom_metric_id = ?1 AND p.kind = ?2
             ORDER BY p.end_date",
            point_cols = prefixed(POINT_COLS, "cmp"),
        );
        let mut stmt = g.conn().prepare(&q)?;
        let rows = stmt.query_map(rusqlite::params![metric_id, kind.as_str()], |row| {
            let period = map_period_row(row)?;
            let point = map_point_row_offset(row, 9)?;
            Ok((period, point))
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    async fn points_unaligned(&self, cik: &Cik) -> Result<Vec<CustomMetricPoint>, RepoError> {
        let g = self.pool.read()?;
        let q = format!(
            "SELECT {point_cols}
             FROM custom_metric_point cmp
             JOIN custom_metric cm ON cm.id = cmp.custom_metric_id
             WHERE cm.cik = ?1 AND cmp.period_id IS NULL",
            point_cols = prefixed(POINT_COLS, "cmp"),
        );
        let mut stmt = g.conn().prepare(&q)?;
        let rows = stmt.query_map([&cik.0], map_point_row)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    async fn realign_point(&self, point_id: i64, period_id: i64) -> Result<(), RepoError> {
        let mut g = self.pool.write().await;
        g.conn().execute(
            "UPDATE custom_metric_point SET period_id = ?1 WHERE id = ?2",
            rusqlite::params![period_id, point_id],
        )?;
        Ok(())
    }
}

/// Prefixes each column name in a comma-separated list with `alias.`.
fn prefixed(cols: &str, alias: &str) -> String {
    cols.split(", ")
        .map(|c| format!("{alias}.{c}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Maps `CustomMetricPoint` columns starting at a given column offset in a
/// wider joined row (used by `points_aligned`, which selects period columns
/// first).
fn map_point_row_offset(row: &rusqlite::Row<'_>, offset: usize) -> rusqlite::Result<CustomMetricPoint> {
    Ok(CustomMetricPoint {
        id: row.get(offset)?,
        custom_metric_id: row.get(offset + 1)?,
        period_id: row.get(offset + 2)?,
        observed_date: row.get(offset + 3)?,
        value_micro: row.get(offset + 4)?,
        source_note: row.get(offset + 5)?,
        imported_at: row.get(offset + 6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Company, Ticker};
    use crate::repos::company::{CompanyRepo, SqliteCompanyRepo};
    use crate::repos::period::{PeriodRepo, SqlitePeriodRepo};
    use chrono::{NaiveDate, TimeZone};

    async fn setup() -> std::sync::Arc<Pool> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.sqlite");
        Box::leak(Box::new(dir));
        std::sync::Arc::new(Pool::open(&path).unwrap())
    }

    async fn seed_company(pool: &std::sync::Arc<Pool>) -> Cik {
        let companies = SqliteCompanyRepo::new(pool.clone());
        let cik = Cik("0000000001".into());
        companies
            .upsert(&Company {
                cik: cik.clone(),
                ticker: Ticker("TEST".into()),
                name: "Test Co".into(),
                exchange: None,
                sic: None,
                fiscal_year_end: Some("1231".into()),
                added_at: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
                last_refreshed: None,
            })
            .await
            .unwrap();
        cik
    }

    async fn seed_period(pool: &std::sync::Arc<Pool>, cik: &Cik, fy: i32) -> i64 {
        let periods = SqlitePeriodRepo::new(pool.clone());
        periods
            .upsert_returning_id(&Period {
                id: 0,
                cik: cik.clone(),
                fiscal_year: fy,
                fiscal_quarter: 0,
                fiscal_year_end: "1231".into(),
                start_date: NaiveDate::from_ymd_opt(fy, 1, 1).unwrap(),
                end_date: NaiveDate::from_ymd_opt(fy, 12, 31).unwrap(),
                kind: PeriodKind::Annual,
                is_53_week: false,
            })
            .await
            .unwrap()
    }

    fn new_metric(key: &str) -> NewCustomMetric {
        NewCustomMetric {
            key: key.into(),
            label: "Total Payment Volume".into(),
            unit_kind: UnitKind::Currency,
            currency: Some("USD".into()),
            decimals: Some(2),
            description: None,
        }
    }

    #[tokio::test]
    async fn create_and_list() {
        let pool = setup().await;
        let cik = seed_company(&pool).await;
        let repo = SqliteCustomMetricRepo::new(pool);
        repo.create(&cik, &new_metric("tpv")).await.unwrap();
        let listed = repo.list(&cik).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].key, "tpv");
    }

    #[tokio::test]
    async fn duplicate_key_conflict() {
        let pool = setup().await;
        let cik = seed_company(&pool).await;
        let repo = SqliteCustomMetricRepo::new(pool);
        repo.create(&cik, &new_metric("tpv")).await.unwrap();
        assert!(repo.create(&cik, &new_metric("tpv")).await.is_err());
    }

    #[tokio::test]
    async fn upsert_point_idempotent() {
        let pool = setup().await;
        let cik = seed_company(&pool).await;
        let period_id = seed_period(&pool, &cik, 2024).await;
        let repo = SqliteCustomMetricRepo::new(pool);
        let m = repo.create(&cik, &new_metric("tpv")).await.unwrap();
        repo.upsert_point(m.id, Some(period_id), None, 100_000_000, Some("manual"))
            .await
            .unwrap();
        repo.upsert_point(m.id, Some(period_id), None, 200_000_000, Some("manual"))
            .await
            .unwrap();
        let pts = repo.points_aligned(m.id, PeriodKind::Annual).await.unwrap();
        assert_eq!(pts.len(), 1);
        assert_eq!(pts[0].1.value_micro, 200_000_000);
    }

    #[tokio::test]
    async fn points_aligned_sorted() {
        let pool = setup().await;
        let cik = seed_company(&pool).await;
        let p2023 = seed_period(&pool, &cik, 2023).await;
        let p2024 = seed_period(&pool, &cik, 2024).await;
        let repo = SqliteCustomMetricRepo::new(pool);
        let m = repo.create(&cik, &new_metric("tpv")).await.unwrap();
        repo.upsert_point(m.id, Some(p2024), None, 2, None).await.unwrap();
        repo.upsert_point(m.id, Some(p2023), None, 1, None).await.unwrap();
        let pts = repo.points_aligned(m.id, PeriodKind::Annual).await.unwrap();
        assert_eq!(pts.len(), 2);
        assert_eq!(pts[0].0.fiscal_year, 2023);
        assert_eq!(pts[1].0.fiscal_year, 2024);
    }

    #[tokio::test]
    async fn delete_metric_cascades_points() {
        let pool = setup().await;
        let cik = seed_company(&pool).await;
        let period_id = seed_period(&pool, &cik, 2024).await;
        let repo = SqliteCustomMetricRepo::new(pool.clone());
        let m = repo.create(&cik, &new_metric("tpv")).await.unwrap();
        repo.upsert_point(m.id, Some(period_id), None, 1, None).await.unwrap();
        repo.delete(m.id).await.unwrap();
        assert!(repo.get(m.id).await.unwrap().is_none());
        let companies = SqliteCompanyRepo::new(pool);
        assert!(companies.get_by_cik(&cik).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn count_points() {
        let pool = setup().await;
        let cik = seed_company(&pool).await;
        let p1 = seed_period(&pool, &cik, 2023).await;
        let p2 = seed_period(&pool, &cik, 2024).await;
        let repo = SqliteCustomMetricRepo::new(pool);
        let m = repo.create(&cik, &new_metric("tpv")).await.unwrap();
        repo.upsert_point(m.id, Some(p1), None, 1, None).await.unwrap();
        repo.upsert_point(m.id, Some(p2), None, 2, None).await.unwrap();
        assert_eq!(repo.count_points(m.id).await.unwrap(), 2);
    }

    #[tokio::test]
    async fn realign_updates_period() {
        let pool = setup().await;
        let cik = seed_company(&pool).await;
        let period_id = seed_period(&pool, &cik, 2024).await;
        let repo = SqliteCustomMetricRepo::new(pool);
        let m = repo.create(&cik, &new_metric("tpv")).await.unwrap();
        let pt = repo
            .upsert_point(m.id, None, Some("2024-06-01"), 1, None)
            .await
            .unwrap();
        let unaligned = repo.points_unaligned(&cik).await.unwrap();
        assert_eq!(unaligned.len(), 1);
        repo.realign_point(pt.id, period_id).await.unwrap();
        let unaligned_after = repo.points_unaligned(&cik).await.unwrap();
        assert_eq!(unaligned_after.len(), 0);
        let aligned = repo.points_aligned(m.id, PeriodKind::Annual).await.unwrap();
        assert_eq!(aligned.len(), 1);
    }
}
