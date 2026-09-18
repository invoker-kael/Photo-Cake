use crate::ExposureBracketSet;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS exposure_bracket_merge_state (
    group_id TEXT PRIMARY KEY NOT NULL,
    bracket_fingerprint TEXT NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL
);
"#;

#[derive(Debug, Error)]
pub enum ExposureBracketMergeStoreError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone)]
pub struct ExposureBracketMergeStore {
    path: PathBuf,
}

impl ExposureBracketMergeStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ExposureBracketMergeStoreError> {
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

    fn connect(&self) -> Result<Connection, ExposureBracketMergeStoreError> {
        Ok(Connection::open(&self.path)?)
    }

    pub fn mark_merged(
        &self,
        group_id: Uuid,
        sets: &[ExposureBracketSet],
    ) -> Result<(), ExposureBracketMergeStoreError> {
        if sets.is_empty() {
            return self.clear(group_id);
        }
        let conn = self.connect()?;
        conn.execute(
            "INSERT INTO exposure_bracket_merge_state
                (group_id, bracket_fingerprint, updated_at_unix_ms)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(group_id) DO UPDATE SET
                bracket_fingerprint = excluded.bracket_fingerprint,
                updated_at_unix_ms = excluded.updated_at_unix_ms",
            params![
                group_id.to_string(),
                bracket_fingerprint(sets),
                unix_time_ms()
            ],
        )?;
        Ok(())
    }

    pub fn clear(&self, group_id: Uuid) -> Result<(), ExposureBracketMergeStoreError> {
        let conn = self.connect()?;
        conn.execute(
            "DELETE FROM exposure_bracket_merge_state WHERE group_id = ?1",
            [group_id.to_string()],
        )?;
        Ok(())
    }

    pub fn is_currently_merged(
        &self,
        group_id: Uuid,
        sets: &[ExposureBracketSet],
    ) -> Result<bool, ExposureBracketMergeStoreError> {
        if sets.is_empty() {
            return Ok(false);
        }
        let conn = self.connect()?;
        let saved = conn
            .query_row(
                "SELECT bracket_fingerprint
                 FROM exposure_bracket_merge_state
                 WHERE group_id = ?1",
                [group_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        Ok(saved.is_some_and(|value| value == bracket_fingerprint(sets)))
    }
}

fn bracket_fingerprint(sets: &[ExposureBracketSet]) -> String {
    let mut set_parts = sets
        .iter()
        .map(|set| {
            let mut members = set
                .members
                .iter()
                .map(|member| member.asset_id.to_string())
                .collect::<Vec<_>>();
            members.sort();
            format!("{}:{}", set.center_asset_id, members.join(","))
        })
        .collect::<Vec<_>>();
    set_parts.sort();
    format!("bracket-members-v1|{}", set_parts.join("|"))
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
    use crate::{ExposureBracketMember, ExposureBracketRole};
    use tempfile::tempdir;

    fn bracket(group_id: Uuid, center: Uuid, others: [Uuid; 2]) -> ExposureBracketSet {
        ExposureBracketSet {
            group_id,
            center_asset_id: center,
            members: vec![
                ExposureBracketMember {
                    asset_id: center,
                    exposure_ev: 0.0,
                    offset_from_center_ev: 0.0,
                    role: ExposureBracketRole::Base,
                },
                ExposureBracketMember {
                    asset_id: others[0],
                    exposure_ev: -1.0,
                    offset_from_center_ev: -1.0,
                    role: ExposureBracketRole::Under,
                },
                ExposureBracketMember {
                    asset_id: others[1],
                    exposure_ev: 1.0,
                    offset_from_center_ev: 1.0,
                    role: ExposureBracketRole::Over,
                },
            ],
            span_ev: 2.0,
            minimum_embedding_similarity: 0.98,
        }
    }

    #[test]
    fn completion_is_bound_to_current_bracket_membership() {
        let dir = tempdir().unwrap();
        let store = ExposureBracketMergeStore::open(dir.path().join("project.sqlite3")).unwrap();
        let group_id = Uuid::new_v4();
        let first = bracket(
            group_id,
            Uuid::new_v4(),
            [Uuid::new_v4(), Uuid::new_v4()],
        );

        store.mark_merged(group_id, std::slice::from_ref(&first)).unwrap();
        assert!(store
            .is_currently_merged(group_id, std::slice::from_ref(&first))
            .unwrap());

        let changed = bracket(
            group_id,
            first.center_asset_id,
            [first.members[1].asset_id, Uuid::new_v4()],
        );
        assert!(!store
            .is_currently_merged(group_id, std::slice::from_ref(&changed))
            .unwrap());
    }

    #[test]
    fn clearing_reopens_hdr_merge_action() {
        let dir = tempdir().unwrap();
        let store = ExposureBracketMergeStore::open(dir.path().join("project.sqlite3")).unwrap();
        let group_id = Uuid::new_v4();
        let set = bracket(
            group_id,
            Uuid::new_v4(),
            [Uuid::new_v4(), Uuid::new_v4()],
        );
        store.mark_merged(group_id, std::slice::from_ref(&set)).unwrap();
        store.clear(group_id).unwrap();
        assert!(!store
            .is_currently_merged(group_id, std::slice::from_ref(&set))
            .unwrap());
    }
}
