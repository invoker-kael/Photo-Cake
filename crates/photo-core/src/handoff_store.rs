use crate::FinishPlan;
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS finishing_handoffs (
    id TEXT PRIMARY KEY NOT NULL,
    asset_id TEXT NOT NULL,
    source_path TEXT NOT NULL,
    plan_json TEXT NOT NULL,
    created_at_unix_ms INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_finishing_handoffs_asset_id
    ON finishing_handoffs(asset_id);
"#;

#[derive(Debug, Error)]
pub enum HandoffStoreError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone)]
pub struct HandoffStore {
    path: PathBuf,
}

impl HandoffStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, HandoffStoreError> {
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

    fn connect(&self) -> Result<Connection, HandoffStoreError> {
        Ok(Connection::open(&self.path)?)
    }

    pub fn record_plan(&self, plan: &FinishPlan) -> Result<(), HandoffStoreError> {
        let conn = self.connect()?;
        let plan_json = serde_json::to_string(plan)?;
        conn.execute(
            "INSERT INTO finishing_handoffs
             (id, asset_id, source_path, plan_json, created_at_unix_ms)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                plan.id.to_string(),
                plan.asset_id.to_string(),
                plan.source_path,
                plan_json,
                unix_time_ms()
            ],
        )?;
        Ok(())
    }

    pub fn list_for_asset(&self, asset_id: Uuid) -> Result<Vec<FinishPlan>, HandoffStoreError> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT plan_json
             FROM finishing_handoffs
             WHERE asset_id = ?1
             ORDER BY created_at_unix_ms ASC, rowid ASC",
        )?;
        let rows = stmt.query_map([asset_id.to_string()], |row| row.get::<_, String>(0))?;

        let mut plans = Vec::new();
        for row in rows {
            plans.push(serde_json::from_str::<FinishPlan>(&row?)?);
        }
        Ok(plans)
    }
}

fn unix_time_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{plan_finish, EditCompatibility, FinishTarget, HandoffMode, RawAsset};
    use tempfile::tempdir;

    #[test]
    fn persists_xmp_handoff_manifest_linked_to_raw_asset() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("photo-cake.sqlite3");
        let source_dir = dir.path().join("source");
        std::fs::create_dir_all(&source_dir).unwrap();
        let raw_path = source_dir.join("IMG_0001.CR3");
        std::fs::write(&raw_path, b"raw").unwrap();

        let asset = RawAsset {
            id: Uuid::new_v4(),
            source_path: raw_path.to_string_lossy().into_owned(),
            filename: "IMG_0001.CR3".to_string(),
            extension: "cr3".to_string(),
            camera_id: None,
            capture_time_ms: None,
            file_time_ms: None,
            sequence_number: Some(1),
        };

        let plan = plan_finish(
            &asset,
            FinishTarget::Lightroom,
            EditCompatibility::default(),
            false,
            None,
        )
        .unwrap();
        let store = HandoffStore::open(&db).unwrap();
        store.record_plan(&plan).unwrap();
        let loaded = store.list_for_asset(asset.id).unwrap();

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].asset_id, asset.id);
        assert_eq!(loaded[0].source_path, asset.source_path);
        assert_eq!(loaded[0].mode, HandoffMode::XmpNative);
        assert_eq!(loaded[0], plan);
    }
}