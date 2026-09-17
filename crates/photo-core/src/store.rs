use crate::{Batch, BatchItem, BatchStage, JobStatus};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS batches (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    auto_qa INTEGER NOT NULL,
    stop_on_error INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS batch_items (
    id TEXT PRIMARY KEY NOT NULL,
    batch_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    source_path TEXT NOT NULL,
    stage TEXT NOT NULL,
    status TEXT NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    FOREIGN KEY(batch_id) REFERENCES batches(id) ON DELETE CASCADE,
    UNIQUE(batch_id, position)
);

CREATE INDEX IF NOT EXISTS idx_batch_items_batch_id
    ON batch_items(batch_id);
CREATE INDEX IF NOT EXISTS idx_batch_items_status
    ON batch_items(status);
"#;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid uuid in project database: {0}")]
    Uuid(#[from] uuid::Error),
    #[error("invalid batch stage in project database: {0}")]
    InvalidStage(String),
    #[error("invalid job status in project database: {0}")]
    InvalidStatus(String),
    #[error("batch not found: {0}")]
    BatchNotFound(Uuid),
    #[error("batch item not found: {0}")]
    ItemNotFound(Uuid),
}

#[derive(Debug, Clone)]
pub struct BatchStore {
    path: PathBuf,
}

impl BatchStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }

        let store = Self { path };
        let conn = store.connect()?;
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")?;
        conn.execute_batch(SCHEMA)?;
        Ok(store)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn connect(&self) -> Result<Connection, StoreError> {
        let conn = Connection::open(&self.path)?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        Ok(conn)
    }

    pub fn create_batch(&self, batch: &Batch) -> Result<(), StoreError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO batches (id, name, auto_qa, stop_on_error) VALUES (?1, ?2, ?3, ?4)",
            params![
                batch.id.to_string(),
                batch.name,
                bool_to_int(batch.auto_qa),
                bool_to_int(batch.stop_on_error)
            ],
        )?;

        for (position, item) in batch.items.iter().enumerate() {
            tx.execute(
                "INSERT INTO batch_items
                 (id, batch_id, position, source_path, stage, status, attempts, last_error)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    item.id.to_string(),
                    batch.id.to_string(),
                    position as i64,
                    item.source_path,
                    stage_to_db(item.stage),
                    status_to_db(item.status),
                    item.attempts as i64,
                    item.last_error
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    pub fn save_batch_metadata(&self, batch: &Batch) -> Result<(), StoreError> {
        let conn = self.connect()?;
        let changed = conn.execute(
            "UPDATE batches SET name = ?2, auto_qa = ?3, stop_on_error = ?4 WHERE id = ?1",
            params![
                batch.id.to_string(),
                batch.name,
                bool_to_int(batch.auto_qa),
                bool_to_int(batch.stop_on_error)
            ],
        )?;
        if changed == 0 {
            return Err(StoreError::BatchNotFound(batch.id));
        }
        Ok(())
    }

    pub fn update_item(&self, batch_id: Uuid, item: &BatchItem) -> Result<(), StoreError> {
        let conn = self.connect()?;
        let changed = conn.execute(
            "UPDATE batch_items
             SET source_path = ?3, stage = ?4, status = ?5, attempts = ?6, last_error = ?7
             WHERE id = ?1 AND batch_id = ?2",
            params![
                item.id.to_string(),
                batch_id.to_string(),
                item.source_path,
                stage_to_db(item.stage),
                status_to_db(item.status),
                item.attempts as i64,
                item.last_error
            ],
        )?;
        if changed == 0 {
            return Err(StoreError::ItemNotFound(item.id));
        }
        Ok(())
    }

    pub fn load_batch(&self, batch_id: Uuid) -> Result<Batch, StoreError> {
        let conn = self.connect()?;
        let row = conn
            .query_row(
                "SELECT name, auto_qa, stop_on_error FROM batches WHERE id = ?1",
                [batch_id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)? != 0,
                        row.get::<_, i64>(2)? != 0,
                    ))
                },
            )
            .optional()?;

        let Some((name, auto_qa, stop_on_error)) = row else {
            return Err(StoreError::BatchNotFound(batch_id));
        };

        let mut stmt = conn.prepare(
            "SELECT id, source_path, stage, status, attempts, last_error
             FROM batch_items WHERE batch_id = ?1 ORDER BY position ASC",
        )?;
        let rows = stmt.query_map([batch_id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })?;

        let mut items = Vec::new();
        for row in rows {
            let (id, source_path, stage, status, attempts, last_error) = row?;
            items.push(BatchItem {
                id: Uuid::parse_str(&id)?,
                source_path,
                stage: stage_from_db(&stage)?,
                status: status_from_db(&status)?,
                attempts: attempts.max(0) as u32,
                last_error,
            });
        }

        Ok(Batch {
            id: batch_id,
            name,
            items,
            auto_qa,
            stop_on_error,
        })
    }

    pub fn list_batches(&self) -> Result<Vec<Batch>, StoreError> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare("SELECT id FROM batches ORDER BY rowid DESC")?;
        let ids = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        drop(conn);

        ids.into_iter()
            .map(|id| Ok(self.load_batch(Uuid::parse_str(&id)?)?))
            .collect()
    }

    pub fn recover_interrupted_jobs(&self) -> Result<usize, StoreError> {
        let conn = self.connect()?;
        let changed = conn.execute(
            "UPDATE batch_items SET status = 'PENDING' WHERE status = 'RUNNING'",
            [],
        )?;
        Ok(changed)
    }
}

fn bool_to_int(value: bool) -> i64 {
    if value { 1 } else { 0 }
}

fn stage_to_db(stage: BatchStage) -> &'static str {
    match stage {
        BatchStage::Import => "IMPORT",
        BatchStage::Analyze => "ANALYZE",
        BatchStage::ApplyPreset => "APPLY_PRESET",
        BatchStage::Retouch => "RETOUCH",
        BatchStage::Qa => "QA",
        BatchStage::Export => "EXPORT",
        BatchStage::Done => "DONE",
    }
}

fn stage_from_db(value: &str) -> Result<BatchStage, StoreError> {
    match value {
        "IMPORT" => Ok(BatchStage::Import),
        "ANALYZE" => Ok(BatchStage::Analyze),
        "APPLY_PRESET" => Ok(BatchStage::ApplyPreset),
        "RETOUCH" => Ok(BatchStage::Retouch),
        "QA" => Ok(BatchStage::Qa),
        "EXPORT" => Ok(BatchStage::Export),
        "DONE" => Ok(BatchStage::Done),
        other => Err(StoreError::InvalidStage(other.to_string())),
    }
}

fn status_to_db(status: JobStatus) -> &'static str {
    match status {
        JobStatus::Pending => "PENDING",
        JobStatus::Running => "RUNNING",
        JobStatus::Paused => "PAUSED",
        JobStatus::Failed => "FAILED",
        JobStatus::Cancelled => "CANCELLED",
        JobStatus::Done => "DONE",
    }
}

fn status_from_db(value: &str) -> Result<JobStatus, StoreError> {
    match value {
        "PENDING" => Ok(JobStatus::Pending),
        "RUNNING" => Ok(JobStatus::Running),
        "PAUSED" => Ok(JobStatus::Paused),
        "FAILED" => Ok(JobStatus::Failed),
        "CANCELLED" => Ok(JobStatus::Cancelled),
        "DONE" => Ok(JobStatus::Done),
        other => Err(StoreError::InvalidStatus(other.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn persists_and_recovers_running_job_at_same_stage() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("project.sqlite3");
        let store = BatchStore::open(&db).unwrap();
        let mut batch = Batch::new("test", vec!["sample.jpg".to_string()]);
        store.create_batch(&batch).unwrap();

        let item = &mut batch.items[0];
        item.start();
        item.complete_stage();
        item.start();
        assert_eq!(item.stage, BatchStage::Analyze);
        assert_eq!(item.status, JobStatus::Running);
        store.update_item(batch.id, item).unwrap();

        drop(store);
        let reopened = BatchStore::open(&db).unwrap();
        assert_eq!(reopened.recover_interrupted_jobs().unwrap(), 1);
        let loaded = reopened.load_batch(batch.id).unwrap();

        assert_eq!(loaded.items[0].stage, BatchStage::Analyze);
        assert_eq!(loaded.items[0].status, JobStatus::Pending);
    }
}
