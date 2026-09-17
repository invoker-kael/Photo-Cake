use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PreviewSource {
    EmbeddedRawPreview,
    RenderedRawPreview,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewArtifact {
    pub asset_id: Uuid,
    pub source_fingerprint: String,
    pub revision: String,
    pub cache_path: String,
    pub mime_type: String,
    pub width: u32,
    pub height: u32,
    pub source: PreviewSource,
}

#[derive(Debug, Error)]
pub enum PreviewStoreError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone)]
pub struct PreviewStore {
    path: PathBuf,
}

impl PreviewStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, PreviewStoreError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let store = Self { path };
        let conn = store.connect()?;
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS preview_artifacts (
                asset_id TEXT PRIMARY KEY NOT NULL,
                source_fingerprint TEXT NOT NULL,
                revision TEXT NOT NULL,
                artifact_json TEXT NOT NULL,
                updated_at_unix_ms INTEGER NOT NULL
            );
            "#,
        )?;
        Ok(store)
    }

    fn connect(&self) -> Result<Connection, PreviewStoreError> {
        Ok(Connection::open(&self.path)?)
    }

    pub fn save(&self, artifact: &PreviewArtifact) -> Result<(), PreviewStoreError> {
        let conn = self.connect()?;
        conn.execute(
            "INSERT INTO preview_artifacts
             (asset_id, source_fingerprint, revision, artifact_json, updated_at_unix_ms)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(asset_id) DO UPDATE SET
                 source_fingerprint = excluded.source_fingerprint,
                 revision = excluded.revision,
                 artifact_json = excluded.artifact_json,
                 updated_at_unix_ms = excluded.updated_at_unix_ms",
            params![
                artifact.asset_id.to_string(),
                artifact.source_fingerprint,
                artifact.revision,
                serde_json::to_string(artifact)?,
                unix_time_ms()
            ],
        )?;
        Ok(())
    }

    pub fn get(&self, asset_id: Uuid) -> Result<Option<PreviewArtifact>, PreviewStoreError> {
        let conn = self.connect()?;
        let json = conn
            .query_row(
                "SELECT artifact_json FROM preview_artifacts WHERE asset_id = ?1",
                [asset_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        json.map(|json| Ok(serde_json::from_str(&json)?)).transpose()
    }

    pub fn is_current(
        &self,
        asset_id: Uuid,
        source_fingerprint: &str,
        revision: &str,
    ) -> Result<bool, PreviewStoreError> {
        Ok(self
            .get(asset_id)?
            .is_some_and(|artifact| {
                artifact.source_fingerprint == source_fingerprint && artifact.revision == revision
            }))
    }
}

pub fn preview_cache_path(
    cache_root: impl AsRef<Path>,
    asset_id: Uuid,
    revision: &str,
) -> PathBuf {
    let safe_revision = revision
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    cache_root
        .as_ref()
        .join("previews")
        .join(format!("{asset_id}-{safe_revision}.jpg"))
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
    use tempfile::tempdir;

    #[test]
    fn preview_artifact_round_trips_and_reuses_exact_revision() {
        let dir = tempdir().unwrap();
        let store = PreviewStore::open(dir.path().join("project.sqlite3")).unwrap();
        let asset_id = Uuid::new_v4();
        let artifact = PreviewArtifact {
            asset_id,
            source_fingerprint: "raw-v1".to_string(),
            revision: "embedded-preview-v1".to_string(),
            cache_path: preview_cache_path(dir.path(), asset_id, "embedded-preview-v1")
                .to_string_lossy()
                .into_owned(),
            mime_type: "image/jpeg".to_string(),
            width: 1600,
            height: 1067,
            source: PreviewSource::EmbeddedRawPreview,
        };
        store.save(&artifact).unwrap();
        assert_eq!(store.get(asset_id).unwrap(), Some(artifact));
        assert!(store
            .is_current(asset_id, "raw-v1", "embedded-preview-v1")
            .unwrap());
        assert!(!store
            .is_current(asset_id, "raw-v2", "embedded-preview-v1")
            .unwrap());
    }
}
