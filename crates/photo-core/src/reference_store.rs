use crate::{ReferenceSet, StyleProfile};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS reference_sets (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    style_profile_json TEXT NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS reference_set_photos (
    reference_set_id TEXT NOT NULL,
    asset_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    PRIMARY KEY(reference_set_id, asset_id),
    FOREIGN KEY(reference_set_id) REFERENCES reference_sets(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS group_reference_bindings (
    group_id TEXT PRIMARY KEY NOT NULL,
    reference_set_id TEXT NOT NULL,
    selected_reference_asset_id TEXT NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL,
    FOREIGN KEY(reference_set_id) REFERENCES reference_sets(id) ON DELETE CASCADE
);
"#;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupReferenceBinding {
    pub group_id: Uuid,
    pub reference_set_id: Uuid,
    pub selected_reference_asset_id: Uuid,
}

#[derive(Debug, Error)]
pub enum ReferenceStoreError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid uuid in reference store: {0}")]
    Uuid(#[from] uuid::Error),
    #[error("reference set not found: {0}")]
    ReferenceSetNotFound(Uuid),
    #[error("selected reference asset {asset_id} is not part of reference set {reference_set_id}")]
    SelectedAssetOutsideSet {
        reference_set_id: Uuid,
        asset_id: Uuid,
    },
}

#[derive(Debug, Clone)]
pub struct ReferenceStore {
    path: PathBuf,
}

impl ReferenceStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ReferenceStoreError> {
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

    fn connect(&self) -> Result<Connection, ReferenceStoreError> {
        let conn = Connection::open(&self.path)?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        Ok(conn)
    }

    pub fn save_set(&self, set: &ReferenceSet) -> Result<(), ReferenceStoreError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO reference_sets (id, name, style_profile_json, updated_at_unix_ms)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET
                 name = excluded.name,
                 style_profile_json = excluded.style_profile_json,
                 updated_at_unix_ms = excluded.updated_at_unix_ms",
            params![
                set.id.to_string(),
                set.name,
                serde_json::to_string(&set.style_profile)?,
                unix_time_ms()
            ],
        )?;
        tx.execute(
            "DELETE FROM reference_set_photos WHERE reference_set_id = ?1",
            [set.id.to_string()],
        )?;
        for (position, asset_id) in set.photo_ids.iter().enumerate() {
            tx.execute(
                "INSERT INTO reference_set_photos (reference_set_id, asset_id, position)
                 VALUES (?1, ?2, ?3)",
                params![set.id.to_string(), asset_id.to_string(), position as i64],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn get_set(&self, id: Uuid) -> Result<Option<ReferenceSet>, ReferenceStoreError> {
        let conn = self.connect()?;
        let row = conn
            .query_row(
                "SELECT name, style_profile_json FROM reference_sets WHERE id = ?1",
                [id.to_string()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;

        let Some((name, style_json)) = row else {
            return Ok(None);
        };

        let mut stmt = conn.prepare(
            "SELECT asset_id FROM reference_set_photos
             WHERE reference_set_id = ?1 ORDER BY position ASC",
        )?;
        let ids = stmt
            .query_map([id.to_string()], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let photo_ids = ids
            .into_iter()
            .map(|value| Uuid::parse_str(&value))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Some(ReferenceSet {
            id,
            name,
            photo_ids,
            style_profile: serde_json::from_str::<StyleProfile>(&style_json)?,
        }))
    }

    pub fn bind_group(
        &self,
        group_id: Uuid,
        reference_set_id: Uuid,
        selected_reference_asset_id: Uuid,
    ) -> Result<GroupReferenceBinding, ReferenceStoreError> {
        let set = self
            .get_set(reference_set_id)?
            .ok_or(ReferenceStoreError::ReferenceSetNotFound(reference_set_id))?;
        if !set.photo_ids.contains(&selected_reference_asset_id) {
            return Err(ReferenceStoreError::SelectedAssetOutsideSet {
                reference_set_id,
                asset_id: selected_reference_asset_id,
            });
        }

        let conn = self.connect()?;
        conn.execute(
            "INSERT INTO group_reference_bindings
             (group_id, reference_set_id, selected_reference_asset_id, updated_at_unix_ms)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(group_id) DO UPDATE SET
                 reference_set_id = excluded.reference_set_id,
                 selected_reference_asset_id = excluded.selected_reference_asset_id,
                 updated_at_unix_ms = excluded.updated_at_unix_ms",
            params![
                group_id.to_string(),
                reference_set_id.to_string(),
                selected_reference_asset_id.to_string(),
                unix_time_ms()
            ],
        )?;

        Ok(GroupReferenceBinding {
            group_id,
            reference_set_id,
            selected_reference_asset_id,
        })
    }

    pub fn group_binding(
        &self,
        group_id: Uuid,
    ) -> Result<Option<GroupReferenceBinding>, ReferenceStoreError> {
        let conn = self.connect()?;
        let row = conn
            .query_row(
                "SELECT reference_set_id, selected_reference_asset_id
                 FROM group_reference_bindings WHERE group_id = ?1",
                [group_id.to_string()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;

        row.map(|(reference_set_id, selected_reference_asset_id)| {
            Ok(GroupReferenceBinding {
                group_id,
                reference_set_id: Uuid::parse_str(&reference_set_id)?,
                selected_reference_asset_id: Uuid::parse_str(&selected_reference_asset_id)?,
            })
        })
        .transpose()
    }

    pub fn clear_group_binding(&self, group_id: Uuid) -> Result<(), ReferenceStoreError> {
        let conn = self.connect()?;
        conn.execute(
            "DELETE FROM group_reference_bindings WHERE group_id = ?1",
            [group_id.to_string()],
        )?;
        Ok(())
    }

    /// Persist the simple workstation action "use this photo as this group's reference"
    /// while still representing the choice as a real ReferenceSet.
    pub fn set_single_photo_reference(
        &self,
        group_id: Uuid,
        asset_id: Uuid,
        name: impl Into<String>,
    ) -> Result<(ReferenceSet, GroupReferenceBinding), ReferenceStoreError> {
        let mut set = match self.group_binding(group_id)? {
            Some(binding) => self
                .get_set(binding.reference_set_id)?
                .ok_or(ReferenceStoreError::ReferenceSetNotFound(
                    binding.reference_set_id,
                ))?,
            None => ReferenceSet::from_photos(name.into(), vec![asset_id]),
        };

        set.photo_ids = vec![asset_id];
        self.save_set(&set)?;
        let binding = self.bind_group(group_id, set.id, asset_id)?;
        Ok((set, binding))
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
    use tempfile::tempdir;

    #[test]
    fn group_reference_selection_round_trips() {
        let dir = tempdir().unwrap();
        let store = ReferenceStore::open(dir.path().join("project.sqlite3")).unwrap();
        let group_id = Uuid::new_v4();
        let asset_id = Uuid::new_v4();

        let (set, binding) = store
            .set_single_photo_reference(group_id, asset_id, "Travel reference")
            .unwrap();

        assert_eq!(set.photo_ids, vec![asset_id]);
        assert_eq!(binding.group_id, group_id);
        assert_eq!(binding.selected_reference_asset_id, asset_id);
        assert_eq!(store.group_binding(group_id).unwrap(), Some(binding));
        assert_eq!(store.get_set(set.id).unwrap(), Some(set));
    }

    #[test]
    fn changing_reference_reuses_group_reference_set_and_style_profile() {
        let dir = tempdir().unwrap();
        let store = ReferenceStore::open(dir.path().join("project.sqlite3")).unwrap();
        let group_id = Uuid::new_v4();
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();

        let (mut set, _) = store
            .set_single_photo_reference(group_id, first, "Street look")
            .unwrap();
        set.style_profile.contrast_preference = Some(7.0);
        store.save_set(&set).unwrap();

        let (updated, binding) = store
            .set_single_photo_reference(group_id, second, "ignored")
            .unwrap();

        assert_eq!(updated.id, set.id);
        assert_eq!(updated.photo_ids, vec![second]);
        assert_eq!(updated.style_profile.contrast_preference, Some(7.0));
        assert_eq!(binding.selected_reference_asset_id, second);
    }
}
