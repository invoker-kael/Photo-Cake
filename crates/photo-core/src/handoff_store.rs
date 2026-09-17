use crate::{LightroomHandoffPlan, LightroomHandoffPreset};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS lightroom_handoffs (
    id TEXT PRIMARY KEY NOT NULL,
    asset_id TEXT NOT NULL,
    source_path TEXT NOT NULL,
    output_path TEXT NOT NULL UNIQUE,
    preset_json TEXT NOT NULL,
    created_at_unix_ms INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_lightroom_handoffs_asset_id
    ON lightroom_handoffs(asset_id);
"#;

#[derive(Debug, Error)]
pub enum HandoffStoreError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid uuid in handoff database: {0}")]
    Uuid(#[from] uuid::Error),
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

    pub fn record_plan(&self, plan: &LightroomHandoffPlan) -> Result<(), HandoffStoreError> {
        let conn = self.connect()?;
        let preset_json = serde_json::to_string(&plan.preset)?;
        conn.execute(
            "INSERT INTO lightroom_handoffs
             (id, asset_id, source_path, output_path, preset_json, created_at_unix_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                plan.id.to_string(),
                plan.asset_id.to_string(),
                plan.source_path,
                plan.output_path,
                preset_json,
                unix_time_ms()
            ],
        )?;
        Ok(())
    }

    pub fn list_for_asset(
        &self,
        asset_id: Uuid,
    ) -> Result<Vec<LightroomHandoffPlan>, HandoffStoreError> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT id, source_path, output_path, preset_json
             FROM lightroom_handoffs
             WHERE asset_id = ?1
             ORDER BY created_at_unix_ms ASC, rowid ASC",
        )?;
        let rows = stmt.query_map([asset_id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;

        let mut plans = Vec::new();
        for row in rows {
            let (id, source_path, output_path, preset_json) = row?;
            plans.push(LightroomHandoffPlan {
                id: Uuid::parse_str(&id)?,
                asset_id,
                source_path,
                output_path,
                preset: serde_json::from_str::<LightroomHandoffPreset>(&preset_json)?,
            });
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
    use crate::{plan_lightroom_handoff, RawAsset};
    use tempfile::tempdir;

    #[test]
    fn persists_handoff_manifest_linked_to_raw_asset() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("photo-cake.sqlite3");
        let source_dir = dir.path().join("source");
        let output_dir = dir.path().join("handoff");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&output_dir).unwrap();
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

        let plan = plan_lightroom_handoff(
            &asset,
            &output_dir,
            LightroomHandoffPreset::default(),
        )
        .unwrap();
        let store = HandoffStore::open(&db).unwrap();
        store.record_plan(&plan).unwrap();
        let loaded = store.list_for_asset(asset.id).unwrap();

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].asset_id, asset.id);
        assert_eq!(loaded[0].source_path, asset.source_path);
        assert_eq!(loaded[0].output_path, plan.output_path);
        assert_eq!(loaded[0].preset, LightroomHandoffPreset::default());
    }
}
