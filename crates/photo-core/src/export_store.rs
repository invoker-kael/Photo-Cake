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
pub enum ExportStatus { Pending, Running, Failed, Done, Skipped }

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
    #[error("sqlite error: {0}")] Sqlite(#[from] rusqlite::Error),
    #[error("filesystem error: {0}")] Io(#[from] std::io::Error),
    #[error("json error: {0}")] Json(#[from] serde_json::Error),
    #[error("invalid uuid: {0}")] Uuid(#[from] uuid::Error),
    #[error("export plan error: {0}")] Plan(#[from] ExportPlanError),
    #[error("invalid export status: {0}")] InvalidStatus(String),
}

#[derive(Debug, Clone)]
pub struct ExportStore { path: PathBuf }

impl ExportStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ExportStoreError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) { std::fs::create_dir_all(parent)?; }
        let store = Self { path };
        let conn = store.connect()?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
        conn.execute_batch(SCHEMA)?;
        Ok(store)
    }

    fn connect(&self) -> Result<Connection, ExportStoreError> { Ok(Connection::open(&self.path)?) }

    pub fn set_recipe(&self, batch_id: Uuid, recipe: &ExportRecipe) -> Result<String, ExportStoreError> {
        recipe.validate()?;
        let json = serde_json::to_string(recipe)?;
        let fingerprint = stable_fingerprint(json.as_bytes());
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        let old: Option<String> = tx.query_row("SELECT recipe_fingerprint FROM export_recipes WHERE batch_id=?1", [batch_id.to_string()], |r| r.get(0)).optional()?;
        tx.execute("INSERT INTO export_recipes(batch_id,recipe_json,recipe_fingerprint) VALUES(?1,?2,?3) ON CONFLICT(batch_id) DO UPDATE SET recipe_json=excluded.recipe_json, recipe_fingerprint=excluded.recipe_fingerprint", params![batch_id.to_string(), json, fingerprint])?;
        if old.as_deref().is_some_and(|old| old != fingerprint) {
            tx.execute("DELETE FROM export_checkpoints WHERE batch_id=?1", [batch_id.to_string()])?;
        }
        tx.commit()?;
        Ok(fingerprint)
    }

    pub fn load_recipe(&self, batch_id: Uuid) -> Result<Option<(ExportRecipe, String)>, ExportStoreError> {
        let conn = self.connect()?;
        let row: Option<(String,String)> = conn.query_row("SELECT recipe_json,recipe_fingerprint FROM export_recipes WHERE batch_id=?1", [batch_id.to_string()], |r| Ok((r.get(0)?,r.get(1)?))).optional()?;
        row.map(|(json, fp)| Ok((serde_json::from_str(&json)?, fp))).transpose()
    }

    pub fn reserve(&self, batch_id: Uuid, item_id: Uuid, source: &Path) -> Result<ExportCheckpoint, ExportStoreError> {
        if let Some(existing) = self.load_checkpoint(batch_id, item_id)? { return Ok(existing); }
        let (recipe, fingerprint) = self.load_recipe(batch_id)?.ok_or_else(|| ExportStoreError::Plan(ExportPlanError::InvalidRecipe("no active export recipe".into())))?;
        let plan = plan_export(source, &recipe)?;
        let output_path = collision_safe_output(&plan.output);
        let checkpoint = ExportCheckpoint { batch_id, item_id, output_path, recipe_fingerprint: fingerprint, status: ExportStatus::Pending, attempts: 0, last_error: None };
        self.save_checkpoint(&checkpoint)?;
        Ok(checkpoint)
    }

    pub fn load_checkpoint(&self, batch_id: Uuid, item_id: Uuid) -> Result<Option<ExportCheckpoint>, ExportStoreError> {
        let conn = self.connect()?;
        let row: Option<(String,String,String,i64,Option<String>)> = conn.query_row("SELECT output_path,recipe_fingerprint,status,attempts,last_error FROM export_checkpoints WHERE batch_id=?1 AND item_id=?2", params![batch_id.to_string(), item_id.to_string()], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?;
        row.map(|(path, fp, status, attempts, err)| Ok(ExportCheckpoint { batch_id, item_id, output_path: PathBuf::from(path), recipe_fingerprint: fp, status: status_from_db(&status)?, attempts: attempts.max(0) as u32, last_error: err })).transpose()
    }

    pub fn save_checkpoint(&self, cp: &ExportCheckpoint) -> Result<(), ExportStoreError> {
        let conn = self.connect()?;
        conn.execute("INSERT INTO export_checkpoints(batch_id,item_id,output_path,recipe_fingerprint,status,attempts,last_error) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(batch_id,item_id) DO UPDATE SET output_path=excluded.output_path,recipe_fingerprint=excluded.recipe_fingerprint,status=excluded.status,attempts=excluded.attempts,last_error=excluded.last_error", params![cp.batch_id.to_string(), cp.item_id.to_string(), cp.output_path.to_string_lossy(), cp.recipe_fingerprint, status_to_db(cp.status), cp.attempts as i64, cp.last_error])?;
        Ok(())
    }

    pub fn mark_running(&self, mut cp: ExportCheckpoint) -> Result<ExportCheckpoint, ExportStoreError> { cp.status=ExportStatus::Running; cp.attempts+=1; cp.last_error=None; self.save_checkpoint(&cp)?; Ok(cp) }
    pub fn mark_failed(&self, mut cp: ExportCheckpoint, error: impl Into<String>) -> Result<ExportCheckpoint, ExportStoreError> { cp.status=ExportStatus::Failed; cp.last_error=Some(error.into()); self.save_checkpoint(&cp)?; Ok(cp) }
    pub fn mark_done(&self, mut cp: ExportCheckpoint) -> Result<ExportCheckpoint, ExportStoreError> { cp.status=ExportStatus::Done; cp.last_error=None; self.save_checkpoint(&cp)?; Ok(cp) }
    pub fn recover_interrupted(&self) -> Result<usize, ExportStoreError> { Ok(self.connect()?.execute("UPDATE export_checkpoints SET status='PENDING' WHERE status='RUNNING'", [])?) }
}

fn status_to_db(s: ExportStatus) -> &'static str { match s { ExportStatus::Pending=>"PENDING", ExportStatus::Running=>"RUNNING", ExportStatus::Failed=>"FAILED", ExportStatus::Done=>"DONE", ExportStatus::Skipped=>"SKIPPED" } }
fn status_from_db(s: &str) -> Result<ExportStatus, ExportStoreError> { match s { "PENDING"=>Ok(ExportStatus::Pending), "RUNNING"=>Ok(ExportStatus::Running), "FAILED"=>Ok(ExportStatus::Failed), "DONE"=>Ok(ExportStatus::Done), "SKIPPED"=>Ok(ExportStatus::Skipped), other=>Err(ExportStoreError::InvalidStatus(other.into())) } }

// Deterministic FNV-1a is sufficient as a cache/invalidation fingerprint; it is not a security hash.
fn stable_fingerprint(bytes: &[u8]) -> String { let mut hash=0xcbf29ce484222325u64; for b in bytes { hash ^= *b as u64; hash=hash.wrapping_mul(0x100000001b3); } format!("{hash:016x}") }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ExportColorSpace, ExportFormat};
    use tempfile::tempdir;

    fn recipe(dest: PathBuf, quality: u8) -> ExportRecipe { ExportRecipe { format: ExportFormat::Jpeg, jpeg_quality: Some(quality), tiff_bit_depth: None, color_space: ExportColorSpace::Srgb, resize: None, preserve_metadata: true, destination: dest, qa_approved_only: true } }

    #[test]
    fn checkpoint_survives_restart_and_running_recovers() {
        let dir=tempdir().unwrap(); let db=dir.path().join("p.sqlite3"); let raw=dir.path().join("raw"); let out=dir.path().join("out"); std::fs::create_dir_all(&raw).unwrap(); std::fs::create_dir_all(&out).unwrap(); let src=raw.join("a.cr3"); std::fs::write(&src,b"raw").unwrap();
        let batch=Uuid::new_v4(); let item=Uuid::new_v4(); let store=ExportStore::open(&db).unwrap(); store.set_recipe(batch,&recipe(out.clone(),92)).unwrap(); let cp=store.reserve(batch,item,&src).unwrap(); let cp=store.mark_running(cp).unwrap(); assert_eq!(cp.attempts,1); drop(store);
        let store=ExportStore::open(&db).unwrap(); assert_eq!(store.recover_interrupted().unwrap(),1); let cp=store.load_checkpoint(batch,item).unwrap().unwrap(); assert_eq!(cp.status,ExportStatus::Pending); assert_eq!(cp.attempts,1);
    }

    #[test]
    fn recipe_change_invalidates_export_only() {
        let dir=tempdir().unwrap(); let db=dir.path().join("p.sqlite3"); let raw=dir.path().join("raw"); let out=dir.path().join("out"); std::fs::create_dir_all(&raw).unwrap(); std::fs::create_dir_all(&out).unwrap(); let src=raw.join("a.cr3"); std::fs::write(&src,b"immutable").unwrap(); let before=std::fs::read(&src).unwrap();
        let batch=Uuid::new_v4(); let item=Uuid::new_v4(); let store=ExportStore::open(&db).unwrap(); let fp1=store.set_recipe(batch,&recipe(out.clone(),92)).unwrap(); store.reserve(batch,item,&src).unwrap(); let fp2=store.set_recipe(batch,&recipe(out,85)).unwrap(); assert_ne!(fp1,fp2); assert!(store.load_checkpoint(batch,item).unwrap().is_none()); assert_eq!(std::fs::read(src).unwrap(),before);
    }

    #[test]
    fn reservation_is_stable_across_collision_and_retry() {
        let dir=tempdir().unwrap(); let db=dir.path().join("p.sqlite3"); let raw=dir.path().join("raw"); let out=dir.path().join("out"); std::fs::create_dir_all(&raw).unwrap(); std::fs::create_dir_all(&out).unwrap(); let src=raw.join("a.cr3"); std::fs::write(&src,b"raw").unwrap(); std::fs::write(out.join("a.jpg"),b"existing").unwrap();
        let batch=Uuid::new_v4(); let item=Uuid::new_v4(); let store=ExportStore::open(&db).unwrap(); store.set_recipe(batch,&recipe(out.clone(),92)).unwrap(); let cp=store.reserve(batch,item,&src).unwrap(); assert_eq!(cp.output_path,out.join("a-1.jpg")); let failed=store.mark_failed(store.mark_running(cp).unwrap(),"renderer failed").unwrap(); let resumed=store.reserve(batch,item,&src).unwrap(); assert_eq!(resumed.output_path,failed.output_path); assert_eq!(resumed.attempts,1);
    }
}
