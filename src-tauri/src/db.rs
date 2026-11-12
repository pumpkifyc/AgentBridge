use crate::imports::ImportRecord;
use rusqlite::{Connection, OptionalExtension, params};
use std::path::{Path, PathBuf};

const MIGRATION: &str = "
CREATE TABLE IF NOT EXISTS imports (
    id TEXT PRIMARY KEY,
    source_title TEXT NOT NULL,
    local_thread_id TEXT NOT NULL,
    project_path TEXT NOT NULL,
    imported_at INTEGER NOT NULL,
    snapshot_path TEXT NOT NULL,
    status TEXT NOT NULL,
    error TEXT,
    continuation_prompt TEXT,
    retry_count INTEGER NOT NULL DEFAULT 0,
    last_retried_at INTEGER
);
CREATE INDEX IF NOT EXISTS imports_imported_at_idx ON imports (imported_at DESC);
";

#[derive(Debug, thiserror::Error)]
pub enum DatabaseError {
    #[error("打开本地导入记录失败：{0}")]
    Open(String),
    #[error("读取或写入本地导入记录失败：{0}")]
    Query(String),
}

#[derive(Debug, Clone)]
pub struct ImportDatabase {
    path: PathBuf,
}

impl ImportDatabase {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DatabaseError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| DatabaseError::Open(error.to_string()))?;
        }
        let connection = Connection::open(&path)
            .map_err(|error| DatabaseError::Open(error.to_string()))?;
        connection
            .execute_batch(MIGRATION)
            .map_err(|error| DatabaseError::Open(error.to_string()))?;
        Ok(Self { path })
    }

    fn connect(&self) -> Result<Connection, DatabaseError> {
        Connection::open(&self.path)
            .map_err(|error| DatabaseError::Query(error.to_string()))
    }

    pub fn upsert(&self, record: &ImportRecord) -> Result<(), DatabaseError> {
        let connection = self.connect()?;
        connection
            .execute(
                "INSERT INTO imports (
                    id, source_title, local_thread_id, project_path, imported_at,
                    snapshot_path, status, error, continuation_prompt, retry_count, last_retried_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                 ON CONFLICT(id) DO UPDATE SET
                    source_title = excluded.source_title,
                    local_thread_id = excluded.local_thread_id,
                    project_path = excluded.project_path,
                    imported_at = excluded.imported_at,
                    snapshot_path = excluded.snapshot_path,
                    status = excluded.status,
                    error = excluded.error,
                    continuation_prompt = excluded.continuation_prompt",
                params![
                    record.id,
                    record.source_title,
                    record.local_thread_id,
                    record.project_path,
                    record.imported_at,
                    record.snapshot_path,
                    record.status,
                    record.error,
                    record.continuation_prompt,
                    record.retry_count,
                    record.last_retried_at,
                ],
            )
            .map_err(|error| DatabaseError::Query(error.to_string()))?;
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<ImportRecord>, DatabaseError> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT id, source_title, local_thread_id, project_path, imported_at,
                        snapshot_path, status, error, continuation_prompt, retry_count, last_retried_at
                 FROM imports ORDER BY imported_at DESC",
            )
            .map_err(|error| DatabaseError::Query(error.to_string()))?;
        let rows = statement
            .query_map([], |row| {
                Ok(ImportRecord {
                    id: row.get(0)?,
                    source_title: row.get(1)?,
                    local_thread_id: row.get(2)?,
                    project_path: row.get(3)?,
                    imported_at: row.get(4)?,
                    snapshot_path: row.get(5)?,
                    status: row.get(6)?,
                    error: row.get(7)?,
                    continuation_prompt: row.get(8)?,
                    retry_count: row.get(9)?,
                    last_retried_at: row.get(10)?,
                })
            })
            .map_err(|error| DatabaseError::Query(error.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| DatabaseError::Query(error.to_string()))?;
        Ok(rows)
    }

    pub fn get(&self, id: &str) -> Result<Option<ImportRecord>, DatabaseError> {
        Ok(self.list()?.into_iter().find(|record| record.id == id))
    }

    pub fn mark_retried(&self, id: &str, at: i64) -> Result<(), DatabaseError> {
        let connection = self.connect()?;
        let changed = connection
            .execute(
                "UPDATE imports SET retry_count = retry_count + 1, last_retried_at = ?2 WHERE id = ?1",
                params![id, at],
            )
            .map_err(|error| DatabaseError::Query(error.to_string()))?;
        if changed == 0 {
            return Err(DatabaseError::Query("找不到导入记录".into()));
        }
        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<(), DatabaseError> {
        let connection = self.connect()?;
        let changed = connection
            .execute("DELETE FROM imports WHERE id = ?1", params![id])
            .map_err(|error| DatabaseError::Query(error.to_string()))?;
        if changed == 0 {
            return Err(DatabaseError::Query("找不到导入记录".into()));
        }
        Ok(())
    }

    pub fn find_by_thread(&self, thread_id: &str) -> Result<Option<String>, DatabaseError> {
        let connection = self.connect()?;
        connection
            .query_row(
                "SELECT continuation_prompt FROM imports WHERE local_thread_id = ?1",
                params![thread_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(|error| DatabaseError::Query(error.to_string()))
            .map(Option::flatten)
    }
}
