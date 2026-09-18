use crate::CompanionSnapshot;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS companion_snapshots (
    batch_id TEXT PRIMARY KEY NOT NULL,
    snapshot_id TEXT NOT NULL,
    snapshot_json TEXT NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL
);
"#;

#[derive(Debug, Error)]
pub enum CompanionSnapshotStoreError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid uuid in companion snapshot store: {0}")]
    Uuid(#[from] uuid::Error),
}

#[derive(Debug, Clone)]
pub struct CompanionSnapshotStore {
    path: PathBuf,
}

impl CompanionSnapshotStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, CompanionSnapshotStoreError> {
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

    fn connect(&self) -> Result<Connection, CompanionSnapshotStoreError> {
        Ok(Connection::open(&self.path)?)
    }

    pub fn save(&self, snapshot: &CompanionSnapshot) -> Result<(), CompanionSnapshotStoreError> {
        let conn = self.connect()?;
        conn.execute(
            "INSERT INTO companion_snapshots
             (batch_id, snapshot_id, snapshot_json, updated_at_unix_ms)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(batch_id) DO UPDATE SET
                 snapshot_id = excluded.snapshot_id,
                 snapshot_json = excluded.snapshot_json,
                 updated_at_unix_ms = excluded.updated_at_unix_ms",
            params![
                snapshot.batch_id.to_string(),
                snapshot.snapshot_id.to_string(),
                serde_json::to_string(snapshot)?,
                unix_time_ms()
            ],
        )?;
        Ok(())
    }

    pub fn get(
        &self,
        batch_id: Uuid,
    ) -> Result<Option<CompanionSnapshot>, CompanionSnapshotStoreError> {
        let conn = self.connect()?;
        let json = conn
            .query_row(
                "SELECT snapshot_json FROM companion_snapshots WHERE batch_id = ?1",
                [batch_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        json.map(|value| Ok(serde_json::from_str(&value)?)).transpose()
    }

    pub fn clear(&self, batch_id: Uuid) -> Result<(), CompanionSnapshotStoreError> {
        let conn = self.connect()?;
        conn.execute(
            "DELETE FROM companion_snapshots WHERE batch_id = ?1",
            [batch_id.to_string()],
        )?;
        Ok(())
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
    use crate::COMPANION_SNAPSHOT_SCHEMA_VERSION;
    use tempfile::tempdir;

    fn snapshot() -> CompanionSnapshot {
        CompanionSnapshot {
            schema_version: COMPANION_SNAPSHOT_SCHEMA_VERSION,
            snapshot_id: Uuid::new_v4(),
            batch_id: Uuid::new_v4(),
            batch_name: "Travel".into(),
            assets: Vec::new(),
            groups: Vec::new(),
            culling: Vec::new(),
            culling_reviews: Vec::new(),
            references: Vec::new(),
            metadata: Vec::new(),
            previews: Vec::new(),
        }
    }

    #[test]
    fn snapshot_round_trips_by_batch() {
        let dir = tempdir().unwrap();
        let store = CompanionSnapshotStore::open(dir.path().join("project.sqlite3")).unwrap();
        let snapshot = snapshot();

        store.save(&snapshot).unwrap();
        let loaded = store.get(snapshot.batch_id).unwrap().unwrap();

        assert_eq!(loaded.snapshot_id, snapshot.snapshot_id);
        assert_eq!(loaded.batch_id, snapshot.batch_id);
        assert_eq!(loaded.batch_name, snapshot.batch_name);
    }
}
