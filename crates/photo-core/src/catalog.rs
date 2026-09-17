use crate::{GroupingBasis, PhotoGroup, PhotoGroupKind, RawAsset};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

const RAW_CATALOG_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS raw_assets (
    id TEXT PRIMARY KEY NOT NULL,
    source_path TEXT NOT NULL UNIQUE,
    filename TEXT NOT NULL,
    extension TEXT NOT NULL,
    camera_id TEXT,
    capture_time_ms INTEGER,
    file_time_ms INTEGER,
    sequence_number INTEGER
);

CREATE TABLE IF NOT EXISTS photo_groups (
    id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL,
    basis TEXT NOT NULL,
    manual_locked INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS photo_group_members (
    group_id TEXT NOT NULL,
    asset_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    PRIMARY KEY(group_id, asset_id),
    FOREIGN KEY(group_id) REFERENCES photo_groups(id) ON DELETE CASCADE,
    FOREIGN KEY(asset_id) REFERENCES raw_assets(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_raw_assets_source_path ON raw_assets(source_path);
CREATE INDEX IF NOT EXISTS idx_group_members_asset_id ON photo_group_members(asset_id);
"#;

#[derive(Debug, Error)]
pub enum CatalogError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid uuid in catalog: {0}")]
    Uuid(#[from] uuid::Error),
    #[error("invalid group kind: {0}")]
    InvalidGroupKind(String),
    #[error("invalid grouping basis: {0}")]
    InvalidGroupingBasis(String),
}

#[derive(Debug, Clone)]
pub struct RawCatalog {
    path: PathBuf,
}

impl RawCatalog {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, CatalogError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }

        let catalog = Self { path };
        let conn = catalog.connect()?;
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")?;
        conn.execute_batch(RAW_CATALOG_SCHEMA)?;
        Ok(catalog)
    }

    fn connect(&self) -> Result<Connection, CatalogError> {
        let conn = Connection::open(&self.path)?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        Ok(conn)
    }

    pub fn ensure_assets(&self, assets: &[RawAsset]) -> Result<Vec<RawAsset>, CatalogError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        let mut canonical = Vec::with_capacity(assets.len());

        for incoming in assets {
            let existing_id = tx
                .query_row(
                    "SELECT id FROM raw_assets WHERE source_path = ?1",
                    [&incoming.source_path],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;

            let mut asset = incoming.clone();
            if let Some(id) = existing_id {
                asset.id = Uuid::parse_str(&id)?;
                tx.execute(
                    "UPDATE raw_assets
                     SET filename = ?2, extension = ?3, camera_id = ?4,
                         capture_time_ms = ?5, file_time_ms = ?6, sequence_number = ?7
                     WHERE id = ?1",
                    params![
                        asset.id.to_string(),
                        asset.filename,
                        asset.extension,
                        asset.camera_id,
                        asset.capture_time_ms,
                        asset.file_time_ms,
                        asset.sequence_number.map(|value| value as i64)
                    ],
                )?;
            } else {
                tx.execute(
                    "INSERT INTO raw_assets
                     (id, source_path, filename, extension, camera_id, capture_time_ms, file_time_ms, sequence_number)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        asset.id.to_string(),
                        asset.source_path,
                        asset.filename,
                        asset.extension,
                        asset.camera_id,
                        asset.capture_time_ms,
                        asset.file_time_ms,
                        asset.sequence_number.map(|value| value as i64)
                    ],
                )?;
            }
            canonical.push(asset);
        }

        tx.commit()?;
        Ok(canonical)
    }

    pub fn replace_automatic_groups(&self, groups: &[PhotoGroup]) -> Result<(), CatalogError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM photo_groups WHERE manual_locked = 0", [])?;

        for group in groups {
            tx.execute(
                "INSERT INTO photo_groups (id, kind, basis, manual_locked) VALUES (?1, ?2, ?3, ?4)",
                params![
                    group.id.to_string(),
                    group_kind_to_db(group.kind),
                    grouping_basis_to_db(group.basis),
                    bool_to_int(group.manual_locked)
                ],
            )?;
            for (position, asset_id) in group.asset_ids.iter().enumerate() {
                tx.execute(
                    "INSERT INTO photo_group_members (group_id, asset_id, position) VALUES (?1, ?2, ?3)",
                    params![group.id.to_string(), asset_id.to_string(), position as i64],
                )?;
            }
        }

        tx.commit()?;
        Ok(())
    }

    pub fn list_assets(&self) -> Result<Vec<RawAsset>, CatalogError> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT id, source_path, filename, extension, camera_id, capture_time_ms, file_time_ms, sequence_number
             FROM raw_assets ORDER BY rowid ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<i64>>(5)?,
                row.get::<_, Option<i64>>(6)?,
                row.get::<_, Option<i64>>(7)?,
            ))
        })?;

        let mut assets = Vec::new();
        for row in rows {
            let (id, source_path, filename, extension, camera_id, capture_time_ms, file_time_ms, sequence_number) = row?;
            assets.push(RawAsset {
                id: Uuid::parse_str(&id)?,
                source_path,
                filename,
                extension,
                camera_id,
                capture_time_ms,
                file_time_ms,
                sequence_number: sequence_number.and_then(|value| u64::try_from(value).ok()),
            });
        }
        Ok(assets)
    }

    pub fn list_groups(&self) -> Result<Vec<PhotoGroup>, CatalogError> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT id, kind, basis, manual_locked FROM photo_groups ORDER BY rowid ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)? != 0,
            ))
        })?;

        let mut groups = Vec::new();
        for row in rows {
            let (id, kind, basis, manual_locked) = row?;
            let group_id = Uuid::parse_str(&id)?;
            let mut members = conn.prepare(
                "SELECT asset_id FROM photo_group_members WHERE group_id = ?1 ORDER BY position ASC",
            )?;
            let ids = members
                .query_map([id], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            let asset_ids = ids
                .into_iter()
                .map(|id| Uuid::parse_str(&id))
                .collect::<Result<Vec<_>, _>>()?;

            groups.push(PhotoGroup {
                id: group_id,
                kind: group_kind_from_db(&kind)?,
                basis: grouping_basis_from_db(&basis)?,
                asset_ids,
                manual_locked,
            });
        }
        Ok(groups)
    }
}

fn bool_to_int(value: bool) -> i64 {
    if value { 1 } else { 0 }
}

fn group_kind_to_db(value: PhotoGroupKind) -> &'static str {
    match value {
        PhotoGroupKind::Moment => "MOMENT",
        PhotoGroupKind::Similar => "SIMILAR",
        PhotoGroupKind::Manual => "MANUAL",
    }
}

fn group_kind_from_db(value: &str) -> Result<PhotoGroupKind, CatalogError> {
    match value {
        "MOMENT" => Ok(PhotoGroupKind::Moment),
        "SIMILAR" => Ok(PhotoGroupKind::Similar),
        "MANUAL" => Ok(PhotoGroupKind::Manual),
        other => Err(CatalogError::InvalidGroupKind(other.to_string())),
    }
}

fn grouping_basis_to_db(value: GroupingBasis) -> &'static str {
    match value {
        GroupingBasis::Time => "TIME",
        GroupingBasis::TimeAndSequence => "TIME_AND_SEQUENCE",
        GroupingBasis::SequenceFallback => "SEQUENCE_FALLBACK",
        GroupingBasis::Singleton => "SINGLETON",
    }
}

fn grouping_basis_from_db(value: &str) -> Result<GroupingBasis, CatalogError> {
    match value {
        "TIME" => Ok(GroupingBasis::Time),
        "TIME_AND_SEQUENCE" => Ok(GroupingBasis::TimeAndSequence),
        "SEQUENCE_FALLBACK" => Ok(GroupingBasis::SequenceFallback),
        "SINGLETON" => Ok(GroupingBasis::Singleton),
        other => Err(CatalogError::InvalidGroupingBasis(other.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{initial_group_raw_assets, InitialGroupingConfig};
    use tempfile::tempdir;

    fn sample_asset(path: &str, sequence_number: u64) -> RawAsset {
        RawAsset {
            id: Uuid::new_v4(),
            source_path: path.to_string(),
            filename: Path::new(path)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            extension: "cr3".to_string(),
            camera_id: Some("camera-a".to_string()),
            capture_time_ms: Some(sequence_number as i64 * 500),
            file_time_ms: None,
            sequence_number: Some(sequence_number),
        }
    }

    #[test]
    fn reimport_keeps_stable_asset_id() {
        let dir = tempdir().unwrap();
        let catalog = RawCatalog::open(dir.path().join("catalog.sqlite3")).unwrap();
        let first = sample_asset("C:/shoot/IMG_1001.CR3", 1001);
        let first_id = catalog.ensure_assets(&[first]).unwrap()[0].id;

        let second = sample_asset("C:/shoot/IMG_1001.CR3", 1001);
        let second_id = catalog.ensure_assets(&[second]).unwrap()[0].id;
        assert_eq!(first_id, second_id);
    }

    #[test]
    fn persists_groups_with_canonical_asset_ids() {
        let dir = tempdir().unwrap();
        let catalog = RawCatalog::open(dir.path().join("catalog.sqlite3")).unwrap();
        let assets = catalog
            .ensure_assets(&[
                sample_asset("C:/shoot/IMG_1001.CR3", 1001),
                sample_asset("C:/shoot/IMG_1002.CR3", 1002),
            ])
            .unwrap();
        let groups = initial_group_raw_assets(&assets, InitialGroupingConfig::default());
        catalog.replace_automatic_groups(&groups).unwrap();

        let loaded = catalog.list_groups().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].asset_ids, groups[0].asset_ids);
    }
}
