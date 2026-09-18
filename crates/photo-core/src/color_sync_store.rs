use crate::GroupColorSyncPlan;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS group_color_sync (
    group_id TEXT PRIMARY KEY NOT NULL,
    plan_json TEXT NOT NULL,
    revision INTEGER NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL
);
"#;

#[derive(Debug, Error)]
pub enum ColorSyncStoreError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone)]
pub struct ColorSyncStore {
    path: PathBuf,
}

impl ColorSyncStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ColorSyncStoreError> {
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

    fn connect(&self) -> Result<Connection, ColorSyncStoreError> {
        Ok(Connection::open(&self.path)?)
    }

    pub fn save(&self, plan: &GroupColorSyncPlan) -> Result<(), ColorSyncStoreError> {
        let conn = self.connect()?;
        conn.execute(
            "INSERT INTO group_color_sync (group_id, plan_json, revision, updated_at_unix_ms)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(group_id) DO UPDATE SET
                 plan_json = excluded.plan_json,
                 revision = excluded.revision,
                 updated_at_unix_ms = excluded.updated_at_unix_ms",
            params![
                plan.group_id.to_string(),
                serde_json::to_string(plan)?,
                plan.revision as i64,
                unix_time_ms()
            ],
        )?;
        Ok(())
    }

    pub fn get(&self, group_id: Uuid) -> Result<Option<GroupColorSyncPlan>, ColorSyncStoreError> {
        let conn = self.connect()?;
        let json = conn
            .query_row(
                "SELECT plan_json FROM group_color_sync WHERE group_id = ?1",
                [group_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        json.map(|json| Ok(serde_json::from_str(&json)?)).transpose()
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
    use crate::{build_auto_group_plan, PhotoColorAnalysis};
    use tempfile::tempdir;

    #[test]
    fn color_sync_plan_round_trips() {
        let dir = tempdir().unwrap();
        let store = ColorSyncStore::open(dir.path().join("project.sqlite3")).unwrap();
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let plan = build_auto_group_plan(
            Uuid::new_v4(),
            &[first, second],
            &[
                PhotoColorAnalysis {
                    asset_id: first,
                    exposure_ev: -0.5,
                    temperature_k: Some(5100.0),
                    tint: Some(1.0),
                    confidence: 0.9,
                },
                PhotoColorAnalysis {
                    asset_id: second,
                    exposure_ev: 0.4,
                    temperature_k: Some(5800.0),
                    tint: Some(-1.0),
                    confidence: 0.95,
                },
            ],
            1,
        )
        .unwrap();
        store.save(&plan).unwrap();
        assert_eq!(store.get(plan.group_id).unwrap(), Some(plan));
    }
}
