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
    asset_id TEXT,
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
CREATE INDEX IF NOT EXISTS idx_batch_items_asset_id
    ON batch_items(asset_id);
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
        ensure_asset_id_column(&conn)?;
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
                 (id, batch_id, position, asset_id, source_path, stage, status, attempts, last_error)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    item.id.to_string(),
                    batch.id.to_string(),
                    position as i64,
                    item.asset_id.map(|id| id.to_string()),
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

    pub fn replace_batch(&self, batch: &Batch) -> Result<(), StoreError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO batches (id, name, auto_qa, stop_on_error)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET
                 name = excluded.name,
                 auto_qa = excluded.auto_qa,
                 stop_on_error = excluded.stop_on_error",
            params![
                batch.id.to_string(),
                batch.name,
                bool_to_int(batch.auto_qa),
                bool_to_int(batch.stop_on_error)
            ],
        )?;
        tx.execute(
            "DELETE FROM batch_items WHERE batch_id = ?1",
            [batch.id.to_string()],
        )?;

        for (position, item) in batch.items.iter().enumerate() {
            tx.execute(
                "INSERT INTO batch_items
                 (id, batch_id, position, asset_id, source_path, stage, status, attempts, last_error)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    item.id.to_string(),
                    batch.id.to_string(),
                    position as i64,
                    item.asset_id.map(|id| id.to_string()),
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
             SET asset_id = ?3, source_path = ?4, stage = ?5, status = ?6, attempts = ?7, last_error = ?8
             WHERE id = ?1 AND batch_id = ?2",
            params![
                item.id.to_string(),
                batch_id.to_string(),
                item.asset_id.map(|id| id.to_string()),
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
            "SELECT id, asset_id, source_path, stage, status, attempts, last_error
             FROM batch_items WHERE batch_id = ?1 ORDER BY position ASC",
        )?;
        let rows = stmt.query_map([batch_id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, Option<String>>(6)?,
            ))
        })?;

        let mut items = Vec::new();
        for row in rows {
            let (id, asset_id, source_path, stage, status, attempts, last_error) = row?;
            items.push(BatchItem {
                id: Uuid::parse_str(&id)?,
                asset_id: asset_id
                    .as_deref()
                    .map(Uuid::parse_str)
                    .transpose()?,
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

fn ensure_asset_id_column(conn: &Connection) -> Result<(), StoreError> {
    let mut stmt = conn.prepare("PRAGMA table_info(batch_items)")?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    if !columns.iter().any(|column| column == "asset_id") {
        conn.execute("ALTER TABLE batch_items ADD COLUMN asset_id TEXT", [])?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_batch_items_asset_id ON batch_items(asset_id)",
            [],
        )?;
    }
    Ok(())
}

fn bool_to_int(value: bool) -> i64 {
    if value { 1 } else { 0 }
}

fn stage_to_db(stage: BatchStage) -> &'static str {
    match stage {
        BatchStage::Import => "IMPORT",
        BatchStage::Analyze => "ANALYZE",
        BatchStage::ApplyPreset => "APPLY_PRESET",
        BatchStage::PortraitRetouch => "PORTRAIT_RETOUCH",
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
        "PORTRAIT_RETOUCH" | "RETOUCH" => Ok(BatchStage::PortraitRetouch),
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

    #[test]
    fn replace_batch_is_transactional_upsert_for_companion_hydration() {
        let dir = tempdir().unwrap();
        let store = BatchStore::open(dir.path().join("replace.sqlite3")).unwrap();
        let first_asset = Uuid::new_v4();
        let second_asset = Uuid::new_v4();
        let mut batch = Batch::from_imported_assets(
            "first",
            vec![(first_asset, "companion://first".to_string())],
        );
        store.replace_batch(&batch).unwrap();

        batch.name = "updated".into();
        batch.items = vec![BatchItem {
            id: Uuid::new_v4(),
            asset_id: Some(second_asset),
            source_path: "companion://second".into(),
            stage: BatchStage::Done,
            status: JobStatus::Done,
            attempts: 0,
            last_error: None,
        }];
        store.replace_batch(&batch).unwrap();

        let loaded = store.load_batch(batch.id).unwrap();
        assert_eq!(loaded.name, "updated");
        assert_eq!(loaded.items.len(), 1);
        assert_eq!(loaded.items[0].asset_id, Some(second_asset));
        assert_eq!(loaded.items[0].status, JobStatus::Done);
    }

    #[test]
    fn imported_asset_id_round_trips() {
        let dir = tempdir().unwrap();
        let store = BatchStore::open(dir.path().join("asset.sqlite3")).unwrap();
        let asset_id = Uuid::new_v4();
        let batch = Batch::from_imported_assets(
            "raw",
            vec![(asset_id, "sample.cr3".to_string())],
        );
        store.create_batch(&batch).unwrap();
        let loaded = store.load_batch(batch.id).unwrap();
        assert_eq!(loaded.items[0].asset_id, Some(asset_id));
    }
}
