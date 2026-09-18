use crate::RawMetadataEvidence;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS raw_metadata_evidence (
    asset_id TEXT PRIMARY KEY NOT NULL,
    evidence_json TEXT NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL
);
"#;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetMetadataEvidence {
    pub asset_id: Uuid,
    pub evidence: RawMetadataEvidence,
}

#[derive(Debug, Error)]
pub enum RawMetadataStoreError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone)]
pub struct RawMetadataStore {
    path: PathBuf,
}

impl RawMetadataStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, RawMetadataStoreError> {
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

    fn connect(&self) -> Result<Connection, RawMetadataStoreError> {
        Ok(Connection::open(&self.path)?)
    }

    pub fn save(
        &self,
        asset_id: Uuid,
        evidence: &RawMetadataEvidence,
    ) -> Result<(), RawMetadataStoreError> {
        let merged = match self.get(asset_id)? {
            Some(existing) => RawMetadataEvidence {
                camera_id: evidence
                    .camera_id
                    .clone()
                    .or(existing.evidence.camera_id),
                capture_time_ms: evidence
                    .capture_time_ms
                    .or(existing.evidence.capture_time_ms),
                white_balance: evidence
                    .white_balance
                    .clone()
                    .or(existing.evidence.white_balance),
            },
            None => evidence.clone(),
        };
        let conn = self.connect()?;
        conn.execute(
            "INSERT INTO raw_metadata_evidence (asset_id, evidence_json, updated_at_unix_ms)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(asset_id) DO UPDATE SET
                 evidence_json = excluded.evidence_json,
                 updated_at_unix_ms = excluded.updated_at_unix_ms",
            params![
                asset_id.to_string(),
                serde_json::to_string(&merged)?,
                unix_time_ms()
            ],
        )?;
        Ok(())
    }

    pub fn get(
        &self,
        asset_id: Uuid,
    ) -> Result<Option<AssetMetadataEvidence>, RawMetadataStoreError> {
        let conn = self.connect()?;
        let json = conn
            .query_row(
                "SELECT evidence_json FROM raw_metadata_evidence WHERE asset_id = ?1",
                [asset_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        json.map(|value| {
            Ok(AssetMetadataEvidence {
                asset_id,
                evidence: serde_json::from_str(&value)?,
            })
        })
        .transpose()
    }

    pub fn list_for_assets(
        &self,
        asset_ids: &[Uuid],
    ) -> Result<Vec<AssetMetadataEvidence>, RawMetadataStoreError> {
        let mut values = Vec::new();
        for asset_id in asset_ids {
            if let Some(value) = self.get(*asset_id)? {
                values.push(value);
            }
        }
        Ok(values)
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
    use crate::{RawRational, RawWhiteBalanceEvidence};
    use tempfile::tempdir;

    #[test]
    fn rescan_does_not_erase_existing_white_balance_evidence() {
        let dir = tempdir().unwrap();
        let store = RawMetadataStore::open(dir.path().join("project.sqlite3")).unwrap();
        let asset_id = Uuid::new_v4();
        let original = RawMetadataEvidence {
            camera_id: Some("SONY ILCE-7M4".into()),
            capture_time_ms: Some(1_800_000_000_000),
            white_balance: Some(RawWhiteBalanceEvidence {
                as_shot_neutral: Some([
                    RawRational { num: 2, denom: 5 },
                    RawRational { num: 1, denom: 1 },
                    RawRational { num: 3, denom: 5 },
                ]),
                as_shot_white_xy: None,
            }),
        };
        store.save(asset_id, &original).unwrap();
        store
            .save(
                asset_id,
                &RawMetadataEvidence {
                    camera_id: None,
                    capture_time_ms: None,
                    white_balance: None,
                },
            )
            .unwrap();

        assert_eq!(store.get(asset_id).unwrap().unwrap().evidence, original);
    }

    #[test]
    fn exact_white_balance_evidence_round_trips_by_asset() {
        let dir = tempdir().unwrap();
        let store = RawMetadataStore::open(dir.path().join("project.sqlite3")).unwrap();
        let asset_id = Uuid::new_v4();
        let evidence = RawMetadataEvidence {
            camera_id: Some("Leica Q3".into()),
            capture_time_ms: Some(1_800_000_000_000),
            white_balance: Some(RawWhiteBalanceEvidence {
                as_shot_neutral: Some([
                    RawRational { num: 1, denom: 2 },
                    RawRational { num: 1, denom: 1 },
                    RawRational { num: 3, denom: 5 },
                ]),
                as_shot_white_xy: None,
            }),
        };

        store.save(asset_id, &evidence).unwrap();
        assert_eq!(store.get(asset_id).unwrap().unwrap().evidence, evidence);
    }
}
