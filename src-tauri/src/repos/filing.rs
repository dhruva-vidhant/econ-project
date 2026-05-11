//! Filing repository — M07.

use async_trait::async_trait;

use crate::db::Pool;
use crate::domain::{AccessionNo, Cik, Filing, FormType};
use crate::errors::RepoError;

#[async_trait]
pub trait FilingRepo: Send + Sync {
    async fn upsert(&self, f: &Filing) -> Result<(), RepoError>;
    async fn get(&self, accn: &AccessionNo) -> Result<Option<Filing>, RepoError>;
    async fn list_for_cik(&self, cik: &Cik) -> Result<Vec<Filing>, RepoError>;
    async fn list_unresolved_4_02(&self, cik: &Cik) -> Result<Vec<Filing>, RepoError>;
}

pub struct SqliteFilingRepo {
    pool: std::sync::Arc<Pool>,
}

impl SqliteFilingRepo {
    pub fn new(pool: std::sync::Arc<Pool>) -> Self { Self { pool } }
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Filing> {
    let form_str: String = row.get(2)?;
    let amends_opt: Option<String> = row.get(6)?;
    Ok(Filing {
        accession_no: AccessionNo(row.get(0)?),
        cik: Cik(row.get(1)?),
        form_type: FormType::from_str(&form_str),
        filed_at: row.get(3)?,
        period_of_report: row.get(4)?,
        is_amendment: row.get::<_, i64>(5)? != 0,
        amends: amends_opt.map(AccessionNo),
        item_4_02_8k: row.get::<_, i64>(7)? != 0,
    })
}

const COLS: &str =
    "accession_no, cik, form_type, filed_at, period_of_report, is_amendment, amends, item_4_02_8k";

#[async_trait]
impl FilingRepo for SqliteFilingRepo {
    async fn upsert(&self, f: &Filing) -> Result<(), RepoError> {
        let mut g = self.pool.write().await;
        g.conn().execute(
            "INSERT INTO filing (accession_no, cik, form_type, filed_at, period_of_report,
                is_amendment, amends, item_4_02_8k)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(accession_no) DO UPDATE SET
                form_type=excluded.form_type,
                filed_at=excluded.filed_at,
                period_of_report=excluded.period_of_report,
                is_amendment=excluded.is_amendment,
                amends=excluded.amends,
                item_4_02_8k=excluded.item_4_02_8k",
            rusqlite::params![
                f.accession_no.0,
                f.cik.0,
                f.form_type.as_str(),
                f.filed_at,
                f.period_of_report,
                f.is_amendment as i64,
                f.amends.as_ref().map(|a| &a.0),
                f.item_4_02_8k as i64,
            ],
        )?;
        Ok(())
    }

    async fn get(&self, accn: &AccessionNo) -> Result<Option<Filing>, RepoError> {
        let g = self.pool.read()?;
        let q = format!("SELECT {COLS} FROM filing WHERE accession_no = ?1");
        let mut stmt = g.conn().prepare(&q)?;
        match stmt.query_row([&accn.0], map_row) {
            Ok(f) => Ok(Some(f)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn list_for_cik(&self, cik: &Cik) -> Result<Vec<Filing>, RepoError> {
        let g = self.pool.read()?;
        let q = format!("SELECT {COLS} FROM filing WHERE cik = ?1 ORDER BY filed_at DESC");
        let mut stmt = g.conn().prepare(&q)?;
        let rows = stmt.query_map([&cik.0], map_row)?;
        let mut out = Vec::new();
        for r in rows { out.push(r?); }
        Ok(out)
    }

    async fn list_unresolved_4_02(&self, cik: &Cik) -> Result<Vec<Filing>, RepoError> {
        let g = self.pool.read()?;
        let q = format!(
            "SELECT {COLS} FROM filing f
             WHERE f.cik = ?1 AND f.item_4_02_8k = 1
               AND NOT EXISTS (
                 SELECT 1 FROM restatement_announcement ra
                 JOIN restatement_resolved_by r ON r.restatement_announcement_id = ra.id
                 WHERE ra.accession_no = f.accession_no
               )
             ORDER BY f.filed_at DESC"
        );
        let mut stmt = g.conn().prepare(&q)?;
        let rows = stmt.query_map([&cik.0], map_row)?;
        let mut out = Vec::new();
        for r in rows { out.push(r?); }
        Ok(out)
    }
}
