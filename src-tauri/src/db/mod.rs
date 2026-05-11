//! Database module — M02 (schema + migrations) + M05 (connection pool).
//!
//! Single writer + read pool design (architecture §12.2). All connections
//! are configured with WAL mode, NORMAL synchronous, foreign_keys=ON, and
//! the §12.1 PRAGMAs.

use std::path::Path;
use std::sync::Arc;

use r2d2::Pool as R2d2Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;
use tokio::sync::{Mutex, OwnedMutexGuard};

use crate::errors::RepoError;

const INITIAL_SCHEMA: &str = include_str!("./migrations/V1__initial.sql");

/// Connection pool: one writer (mutex-protected) + a read pool.
pub struct Pool {
    /// Single writer connection, serialized via tokio Mutex.
    writer: Arc<Mutex<Connection>>,
    /// Read connections via r2d2.
    readers: R2d2Pool<SqliteConnectionManager>,
}

pub struct WriteGuard {
    inner: OwnedMutexGuard<Connection>,
}

impl WriteGuard {
    pub fn conn(&mut self) -> &mut Connection { &mut self.inner }
}

pub struct ReadGuard {
    inner: r2d2::PooledConnection<SqliteConnectionManager>,
}

impl ReadGuard {
    pub fn conn(&self) -> &Connection { &self.inner }
}

impl Pool {
    pub fn open(path: &Path) -> Result<Self, RepoError> {
        // Writer connection
        let writer = Connection::open(path)?;
        configure_pragmas(&writer)?;
        // V1 uses a single, additive migration applied via execute_batch.
        // (See architecture §6.3 commentary: V1 ships only additive migrations.)
        writer
            .execute_batch(INITIAL_SCHEMA)
            .map_err(|e| RepoError::Migration(e.to_string()))?;

        // Reader pool
        let manager = SqliteConnectionManager::file(path).with_init(|c| {
            // r2d2's with_init accepts a closure returning rusqlite::Result.
            c.execute_batch(
                "PRAGMA journal_mode = WAL;
                 PRAGMA synchronous = NORMAL;
                 PRAGMA temp_store = MEMORY;
                 PRAGMA mmap_size = 268435456;
                 PRAGMA foreign_keys = ON;",
            )
        });
        let readers = R2d2Pool::builder()
            .max_size(4)
            .build(manager)
            .map_err(|e| RepoError::Pool(e.to_string()))?;

        // Integrity check on open
        let row: String = writer
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        if row != "ok" {
            return Err(RepoError::Integrity(row));
        }

        Ok(Pool {
            writer: Arc::new(Mutex::new(writer)),
            readers,
        })
    }

    pub async fn write(&self) -> WriteGuard {
        let inner = self.writer.clone().lock_owned().await;
        WriteGuard { inner }
    }

    pub fn read(&self) -> Result<ReadGuard, RepoError> {
        let inner = self
            .readers
            .get()
            .map_err(|e| RepoError::Pool(e.to_string()))?;
        Ok(ReadGuard { inner })
    }

    pub async fn integrity_check(&self) -> Result<(), RepoError> {
        let mut g = self.write().await;
        let row: String = g.conn().query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        if row != "ok" { Err(RepoError::Integrity(row)) } else { Ok(()) }
    }
}

fn configure_pragmas(conn: &Connection) -> Result<(), RepoError> {
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA temp_store = MEMORY;
        PRAGMA mmap_size = 268435456;
        PRAGMA foreign_keys = ON;
        ",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_temp() -> Pool {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.sqlite");
        // Leak the tempdir for the duration of the test.
        Box::leak(Box::new(dir));
        Pool::open(&path).expect("open temp db")
    }

    #[tokio::test]
    async fn opens_and_runs_migrations() {
        let pool = open_temp();
        // Tables exist
        let g = pool.read().unwrap();
        let names: Vec<String> = g
            .conn()
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        for required in [
            "amendment_coverage_gap",
            "company",
            "derived_metric",
            "filing",
            "fx_rate",
            "historical_price",
            "ingestion_event",
            "normalized_fact",
            "period",
            "raw_fact",
            "restatement_announcement",
            "restatement_resolved_by",
        ] {
            assert!(names.iter().any(|n| n == required), "missing table: {required}");
        }
    }

    #[tokio::test]
    async fn integrity_check_passes() {
        let pool = open_temp();
        pool.integrity_check().await.unwrap();
    }
}
