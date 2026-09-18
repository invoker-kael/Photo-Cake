use crate::{collision_safe_output, plan_export, ExportPlanError, ExportRecipe};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS export_recipes (
    batch_id TEXT PRIMARY KEY NOT NULL,
    recipe_json TEXT NOT NULL,
    recipe_fingerprint TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS export_checkpoints (
    batch_id TEXT NOT NULL,
    item_id TEXT NOT NULL,
    output_path TEXT NOT NULL,
    recipe_fingerprint TEXT NOT NULL,
    status TEXT NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    PRIMARY KEY(batch_id, item_id)
);
"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExportStatus {
    Pending,
    Running,
    Failed,
    Done,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportCheckpoint {
    pub batch_id: Uuid,
    pub item_id: Uuid,
    pub output_path: PathBuf,
    pub recipe_fingerprint: String,
    pub status: ExportStatus,
    pub attempts: u32,
    pub last_error: Option<String>,
}

#[derive(Debug, Error)]
pub enum ExportStoreError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid uuid: {0}")]
    Uuid(#[from] uuid::Error),
    #[error("export plan error: {0}")]
    Plan(#[from] ExportPlanError),
    #[error("invalid export status: {0}")]
    InvalidStatus(String),
}

#[derive(Debug, Clone)]
pub struct ExportStore {
    path: PathBuf,
}

impl ExportStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ExportStoreError> {
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

    fn connect(&self) -> Result<Connection, ExportStoreError> {
        Ok(Connection::open(&self.path)?)
    }

    pub fn set_recipe(
        &self,
        batch_id: Uuid,
        recipe: &ExportRecipe,
    ) -> Result<String, ExportStoreError> {
        recipe.validate()?;
        let json = serde_json::to_string(recipe)?;
        let fingerprint = stable_fingerprint(json.as_bytes());
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;

        let old: Option<String> = tx
            .query_row(
                "SELECT recipe_fingerprint FROM export_recipes WHERE batch_id = ?1",
                [batch_id.to_string()],
                |row| row.get(0),
            )
            .optional()?;

        tx.execute(
            "INSERT INTO export_recipes(batch_id, recipe_json, recipe_fingerprint)
             VALUES(?1, ?2, ?3)
             ON CONFLICT(batch_id) DO UPDATE SET
                 recipe_json = excluded.recipe_json,
                 recipe_fingerprint = excluded.recipe_fingerprint",
            params![batch_id.to_string(), json, fingerprint],
        )?;

        if old.as_deref().is_some_and(|value| value != fingerprint) {
            tx.execute(
                "DELETE FROM export_checkpoints WHERE batch_id = ?1",
                [batch_id.to_string()],
            )?;
        }

        tx.commit()?;
        Ok(fingerprint)
    }

    pub fn load_recipe(
        &self,
        batch_id: Uuid,
    ) -> Result<Option<(ExportRecipe, String)>, ExportStoreError> {
        let conn = self.connect()?;
        let row: Option<(String, String)> = conn
            .query_row(
                "SELECT recipe_json, recipe_fingerprint
                 FROM export_recipes
                 WHERE batch_id = ?1",
                [batch_id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;

        row.map(|(json, fingerprint)| Ok((serde_json::from_str(&json)?, fingerprint)))
            .transpose()
    }

    pub fn reserve(
        &self,
        batch_id: Uuid,
        item_id: Uuid,
        source: &Path,
    ) -> Result<ExportCheckpoint, ExportStoreError> {
        if let Some(existing) = self.load_checkpoint(batch_id, item_id)? {
            return Ok(existing);
        }

        let (recipe, recipe_fingerprint) = self.load_recipe(batch_id)?.ok_or_else(|| {
            ExportStoreError::Plan(ExportPlanError::InvalidRecipe(
                "no active export recipe".into(),
            ))
        })?;
        let plan = plan_export(source, &recipe)?;
        let output_path = collision_safe_output(&plan.output);
        let checkpoint = ExportCheckpoint {
            batch_id,
            item_id,
            output_path,
            recipe_fingerprint,
            status: ExportStatus::Pending,
            attempts: 0,
            last_error: None,
        };
        self.save_checkpoint(&checkpoint)?;
        Ok(checkpoint)
    }

    pub fn load_checkpoint(
        &self,
        batch_id: Uuid,
        item_id: Uuid,
    ) -> Result<Option<ExportCheckpoint>, ExportStoreError> {
        let conn = self.connect()?;
        let row: Option<(String, String, String, i64, Option<String>)> = conn
            .query_row(
                "SELECT output_path, recipe_fingerprint, status, attempts, last_error
                 FROM export_checkpoints
                 WHERE batch_id = ?1 AND item_id = ?2",
                params![batch_id.to_string(), item_id.to_string()],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .optional()?;

        row.map(|(path, fingerprint, status, attempts, last_error)| {
            Ok(ExportCheckpoint {
                batch_id,
                item_id,
                output_path: PathBuf::from(path),
                recipe_fingerprint: fingerprint,
                status: status_from_db(&status)?,
                attempts: attempts.max(0) as u32,
                last_error,
            })
        })
        .transpose()
    }

    pub fn save_checkpoint(&self, checkpoint: &ExportCheckpoint) -> Result<(), ExportStoreError> {
        let conn = self.connect()?;
        conn.execute(
            "INSERT INTO export_checkpoints
             (batch_id, item_id, output_path, recipe_fingerprint, status, attempts, last_error)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(batch_id, item_id) DO UPDATE SET
                 output_path = excluded.output_path,
                 recipe_fingerprint = excluded.recipe_fingerprint,
                 status = excluded.status,
                 attempts = excluded.attempts,
                 last_error = excluded.last_error",
            params![
                checkpoint.batch_id.to_string(),
                checkpoint.item_id.to_string(),
                checkpoint.output_path.to_string_lossy(),
                checkpoint.recipe_fingerprint,
                status_to_db(checkpoint.status),
                checkpoint.attempts as i64,
                checkpoint.last_error,
            ],
        )?;
        Ok(())
    }

    pub fn mark_running(
        &self,
        mut checkpoint: ExportCheckpoint,
    ) -> Result<ExportCheckpoint, ExportStoreError> {
        checkpoint.status = ExportStatus::Running;
        checkpoint.attempts += 1;
        checkpoint.last_error = None;
        self.save_checkpoint(&checkpoint)?;
        Ok(checkpoint)
    }

    pub fn mark_failed(
        &self,
        mut checkpoint: ExportCheckpoint,
        error: impl Into<String>,
    ) -> Result<ExportCheckpoint, ExportStoreError> {
        checkpoint.status = ExportStatus::Failed;
        checkpoint.last_error = Some(error.into());
        self.save_checkpoint(&checkpoint)?;
        Ok(checkpoint)
    }

    pub fn mark_done(
        &self,
        mut checkpoint: ExportCheckpoint,
    ) -> Result<ExportCheckpoint, ExportStoreError> {
        checkpoint.status = ExportStatus::Done;
        checkpoint.last_error = None;
        self.save_checkpoint(&checkpoint)?;
        Ok(checkpoint)
    }

    /// Recover checkpoints left RUNNING by an abnormal process exit.
    ///
    /// A non-empty final output means the atomic rename completed before the
    /// process stopped, so it is safe to finish the checkpoint as DONE.
    /// Otherwise the reserved temporary output is removed and the same
    /// checkpoint is returned to PENDING with its reservation intact.
    pub fn recover_interrupted(&self) -> Result<usize, ExportStoreError> {
        let running = {
            let conn = self.connect()?;
            let mut statement = conn.prepare(
                "SELECT batch_id, item_id, output_path, recipe_fingerprint, attempts, last_error
                 FROM export_checkpoints
                 WHERE status = 'RUNNING'",
            )?;
            let rows = statement.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            })?;

            let mut checkpoints = Vec::new();
            for row in rows {
                let (batch_id, item_id, output_path, fingerprint, attempts, last_error) = row?;
                checkpoints.push(ExportCheckpoint {
                    batch_id: Uuid::parse_str(&batch_id)?,
                    item_id: Uuid::parse_str(&item_id)?,
                    output_path: PathBuf::from(output_path),
                    recipe_fingerprint: fingerprint,
                    status: ExportStatus::Running,
                    attempts: attempts.max(0) as u32,
                    last_error,
                });
            }
            checkpoints
        };

        let mut recovered = 0;
        for mut checkpoint in running {
            let temporary = partial_output_path(&checkpoint.output_path);
            let output_is_complete = std::fs::metadata(&checkpoint.output_path)
                .map(|metadata| metadata.len() > 0)
                .unwrap_or(false);

            remove_file_if_exists(&temporary)?;
            if output_is_complete {
                checkpoint.status = ExportStatus::Done;
                checkpoint.last_error = None;
            } else {
                remove_file_if_exists(&checkpoint.output_path)?;
                checkpoint.status = ExportStatus::Pending;
                checkpoint.last_error = None;
            }

            self.save_checkpoint(&checkpoint)?;
            recovered += 1;
        }

        Ok(recovered)
    }
}

pub(crate) fn partial_output_path(output: &Path) -> PathBuf {
    output.with_extension("partial")
}

pub(crate) fn remove_file_if_exists(path: &Path) -> Result<(), std::io::Error> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn status_to_db(status: ExportStatus) -> &'static str {
    match status {
        ExportStatus::Pending => "PENDING",
        ExportStatus::Running => "RUNNING",
        ExportStatus::Failed => "FAILED",
        ExportStatus::Done => "DONE",
        ExportStatus::Skipped => "SKIPPED",
    }
}

fn status_from_db(value: &str) -> Result<ExportStatus, ExportStoreError> {
    match value {
        "PENDING" => Ok(ExportStatus::Pending),
        "RUNNING" => Ok(ExportStatus::Running),
        "FAILED" => Ok(ExportStatus::Failed),
        "DONE" => Ok(ExportStatus::Done),
        "SKIPPED" => Ok(ExportStatus::Skipped),
        other => Err(ExportStoreError::InvalidStatus(other.to_string())),
    }
}

// Deterministic FNV-1a is sufficient as a cache/invalidation fingerprint; it is not a security hash.
fn stable_fingerprint(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ExportColorSpace, ExportFormat};
    use tempfile::tempdir;

    fn recipe(destination: PathBuf, quality: u8) -> ExportRecipe {
        ExportRecipe {
            format: ExportFormat::Jpeg,
            jpeg_quality: Some(quality),
            tiff_bit_depth: None,
            color_space: ExportColorSpace::Srgb,
            resize: None,
            preserve_metadata: true,
            destination,
            qa_approved_only: true,
        }
    }

    #[test]
    fn checkpoint_survives_restart_and_running_recovers() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("project.sqlite3");
        let raw = dir.path().join("raw");
        let out = dir.path().join("out");
        std::fs::create_dir_all(&raw).unwrap();
        std::fs::create_dir_all(&out).unwrap();
        let source = raw.join("a.cr3");
        std::fs::write(&source, b"raw").unwrap();

        let batch = Uuid::new_v4();
        let item = Uuid::new_v4();
        let store = ExportStore::open(&db).unwrap();
        store.set_recipe(batch, &recipe(out.clone(), 92)).unwrap();
        let checkpoint = store.reserve(batch, item, &source).unwrap();
        let checkpoint = store.mark_running(checkpoint).unwrap();
        assert_eq!(checkpoint.attempts, 1);
        drop(store);

        let store = ExportStore::open(&db).unwrap();
        assert_eq!(store.recover_interrupted().unwrap(), 1);
        let checkpoint = store.load_checkpoint(batch, item).unwrap().unwrap();
        assert_eq!(checkpoint.status, ExportStatus::Pending);
        assert_eq!(checkpoint.attempts, 1);
    }

    #[test]
    fn crash_recovery_cleans_partial_and_keeps_reservation() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("project.sqlite3");
        let raw = dir.path().join("raw");
        let out = dir.path().join("out");
        std::fs::create_dir_all(&raw).unwrap();
        std::fs::create_dir_all(&out).unwrap();
        let source = raw.join("a.cr3");
        std::fs::write(&source, b"raw").unwrap();

        let batch = Uuid::new_v4();
        let item = Uuid::new_v4();
        let store = ExportStore::open(&db).unwrap();
        store.set_recipe(batch, &recipe(out, 92)).unwrap();
        let checkpoint = store.mark_running(store.reserve(batch, item, &source).unwrap()).unwrap();
        let temporary = partial_output_path(&checkpoint.output_path);
        std::fs::write(&temporary, b"partial").unwrap();
        let reserved_path = checkpoint.output_path.clone();
        drop(store);

        let store = ExportStore::open(&db).unwrap();
        assert_eq!(store.recover_interrupted().unwrap(), 1);
        let recovered = store.load_checkpoint(batch, item).unwrap().unwrap();
        assert_eq!(recovered.status, ExportStatus::Pending);
        assert_eq!(recovered.output_path, reserved_path);
        assert!(!temporary.exists());
    }

    #[test]
    fn crash_recovery_after_rename_finishes_checkpoint() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("project.sqlite3");
        let raw = dir.path().join("raw");
        let out = dir.path().join("out");
        std::fs::create_dir_all(&raw).unwrap();
        std::fs::create_dir_all(&out).unwrap();
        let source = raw.join("a.cr3");
        std::fs::write(&source, b"raw").unwrap();

        let batch = Uuid::new_v4();
        let item = Uuid::new_v4();
        let store = ExportStore::open(&db).unwrap();
        store.set_recipe(batch, &recipe(out, 92)).unwrap();
        let checkpoint = store.mark_running(store.reserve(batch, item, &source).unwrap()).unwrap();
        std::fs::write(&checkpoint.output_path, b"jpeg").unwrap();
        std::fs::write(partial_output_path(&checkpoint.output_path), b"stale").unwrap();
        drop(store);

        let store = ExportStore::open(&db).unwrap();
        assert_eq!(store.recover_interrupted().unwrap(), 1);
        let recovered = store.load_checkpoint(batch, item).unwrap().unwrap();
        assert_eq!(recovered.status, ExportStatus::Done);
        assert!(!partial_output_path(&recovered.output_path).exists());
    }

    #[test]
    fn recipe_change_invalidates_export_only() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("project.sqlite3");
        let raw = dir.path().join("raw");
        let out = dir.path().join("out");
        std::fs::create_dir_all(&raw).unwrap();
        std::fs::create_dir_all(&out).unwrap();
        let source = raw.join("a.cr3");
        std::fs::write(&source, b"immutable").unwrap();
        let before = std::fs::read(&source).unwrap();

        let batch = Uuid::new_v4();
        let item = Uuid::new_v4();
        let store = ExportStore::open(&db).unwrap();
        let first = store.set_recipe(batch, &recipe(out.clone(), 92)).unwrap();
        store.reserve(batch, item, &source).unwrap();
        let second = store.set_recipe(batch, &recipe(out, 85)).unwrap();

        assert_ne!(first, second);
        assert!(store.load_checkpoint(batch, item).unwrap().is_none());
        assert_eq!(std::fs::read(source).unwrap(), before);
    }

    #[test]
    fn reservation_is_stable_across_collision_and_retry() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("project.sqlite3");
        let raw = dir.path().join("raw");
        let out = dir.path().join("out");
        std::fs::create_dir_all(&raw).unwrap();
        std::fs::create_dir_all(&out).unwrap();
        let source = raw.join("a.cr3");
        std::fs::write(&source, b"raw").unwrap();
        std::fs::write(out.join("a.jpg"), b"existing").unwrap();

        let batch = Uuid::new_v4();
        let item = Uuid::new_v4();
        let store = ExportStore::open(&db).unwrap();
        store.set_recipe(batch, &recipe(out.clone(), 92)).unwrap();
        let checkpoint = store.reserve(batch, item, &source).unwrap();
        assert_eq!(checkpoint.output_path, out.join("a-1.jpg"));

        let failed = store
            .mark_failed(store.mark_running(checkpoint).unwrap(), "renderer failed")
            .unwrap();
        let resumed = store.reserve(batch, item, &source).unwrap();
        assert_eq!(resumed.output_path, failed.output_path);
        assert_eq!(resumed.attempts, 1);
    }
}
