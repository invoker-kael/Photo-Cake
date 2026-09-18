use crate::{
    GroupingBasis, PhotoGroup, PhotoGroupKind, RawAsset, SemanticGroupKind, SemanticPhotoGroup,
};
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

CREATE TABLE IF NOT EXISTS photo_group_collections (
    group_id TEXT PRIMARY KEY NOT NULL,
    collection_id TEXT NOT NULL,
    FOREIGN KEY(group_id) REFERENCES photo_groups(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS semantic_photo_groups (
    id TEXT PRIMARY KEY NOT NULL,
    parent_group_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    reference_candidate_id TEXT,
    similarity_threshold REAL NOT NULL,
    FOREIGN KEY(parent_group_id) REFERENCES photo_groups(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS semantic_photo_group_members (
    group_id TEXT NOT NULL,
    asset_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    PRIMARY KEY(group_id, asset_id),
    FOREIGN KEY(group_id) REFERENCES semantic_photo_groups(id) ON DELETE CASCADE,
    FOREIGN KEY(asset_id) REFERENCES raw_assets(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_raw_assets_source_path ON raw_assets(source_path);
CREATE INDEX IF NOT EXISTS idx_group_members_asset_id ON photo_group_members(asset_id);
CREATE INDEX IF NOT EXISTS idx_group_collections_collection_id ON photo_group_collections(collection_id);
CREATE INDEX IF NOT EXISTS idx_semantic_groups_parent ON semantic_photo_groups(parent_group_id);
CREATE INDEX IF NOT EXISTS idx_semantic_group_members_asset ON semantic_photo_group_members(asset_id);
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
    #[error("invalid semantic group kind: {0}")]
    InvalidSemanticGroupKind(String),
    #[error("semantic group {group_id} does not belong to parent {parent_group_id}")]
    SemanticParentMismatch {
        group_id: Uuid,
        parent_group_id: Uuid,
    },
    #[error("photo group not found in collection: {0}")]
    GroupNotInCollection(Uuid),
    #[error("semantic child {0} must be reverted to its parent before manual merge/split")]
    SemanticGroupRequiresParent(Uuid),
    #[error("manual group operation requires at least two groups")]
    MergeNeedsTwoGroups,
    #[error("duplicate group in manual operation: {0}")]
    DuplicateGroup(Uuid),
    #[error("manual merge groups must be adjacent in collection order")]
    NonAdjacentGroups,
    #[error("split asset {asset_id} is not an interior member of group {group_id}")]
    InvalidSplitAsset { group_id: Uuid, asset_id: Uuid },
    #[error("manual group operation would duplicate asset {0}")]
    DuplicateAsset(Uuid),
    #[error("manual group {0} cannot be unlocked for automatic refinement")]
    ManualGroupCannotUnlock(Uuid),
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
            let existing = tx
                .query_row(
                    "SELECT id, camera_id, capture_time_ms FROM raw_assets WHERE source_path = ?1",
                    [&incoming.source_path],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, Option<i64>>(2)?,
                        ))
                    },
                )
                .optional()?;

            let mut asset = incoming.clone();
            if let Some((id, existing_camera_id, existing_capture_time_ms)) = existing {
                asset.id = Uuid::parse_str(&id)?;
                if asset.camera_id.is_none() {
                    asset.camera_id = existing_camera_id;
                }
                if asset.capture_time_ms.is_none() {
                    asset.capture_time_ms = existing_capture_time_ms;
                }
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

    pub fn replace_automatic_groups(
        &self,
        collection_id: Uuid,
        groups: &[PhotoGroup],
    ) -> Result<(), CatalogError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM photo_groups
             WHERE manual_locked = 0
               AND id IN (
                   SELECT group_id FROM photo_group_collections WHERE collection_id = ?1
               )",
            [collection_id.to_string()],
        )?;

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
            tx.execute(
                "INSERT INTO photo_group_collections (group_id, collection_id) VALUES (?1, ?2)",
                params![group.id.to_string(), collection_id.to_string()],
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

    pub fn semantic_parent_group_id(
        &self,
        group_id: Uuid,
    ) -> Result<Option<Uuid>, CatalogError> {
        let conn = self.connect()?;
        let value = conn
            .query_row(
                "SELECT parent_group_id FROM semantic_photo_groups WHERE id = ?1",
                [group_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        value
            .map(|value| Uuid::parse_str(&value).map_err(CatalogError::from))
            .transpose()
    }

    /// Collapse semantic children back to their original moment and lock that
    /// parent against automatic refinement.
    pub fn keep_moment_together(
        &self,
        collection_id: Uuid,
        group_id: Uuid,
    ) -> Result<PhotoGroup, CatalogError> {
        let parent_group_id = self.semantic_parent_group_id(group_id)?.unwrap_or(group_id);
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        ensure_group_in_collection(&tx, collection_id, parent_group_id)?;
        tx.execute(
            "DELETE FROM semantic_photo_groups WHERE parent_group_id = ?1",
            [parent_group_id.to_string()],
        )?;
        tx.execute(
            "UPDATE photo_groups SET manual_locked = 1 WHERE id = ?1",
            [parent_group_id.to_string()],
        )?;
        tx.commit()?;
        self.find_group(parent_group_id)?
            .ok_or(CatalogError::GroupNotInCollection(parent_group_id))
    }

    pub fn allow_parent_refinement(
        &self,
        collection_id: Uuid,
        group_id: Uuid,
    ) -> Result<PhotoGroup, CatalogError> {
        if self.semantic_parent_group_id(group_id)?.is_some() {
            return Err(CatalogError::SemanticGroupRequiresParent(group_id));
        }

        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        let (kind, _) = load_parent_group_row(&tx, collection_id, group_id)?;
        if kind == PhotoGroupKind::Manual {
            return Err(CatalogError::ManualGroupCannotUnlock(group_id));
        }
        tx.execute(
            "UPDATE photo_groups SET manual_locked = 0 WHERE id = ?1",
            [group_id.to_string()],
        )?;
        tx.commit()?;
        self.find_group(group_id)?
            .ok_or(CatalogError::GroupNotInCollection(group_id))
    }

    /// Merge adjacent parent groups into one manual locked group. The first
    /// group ID is preserved so downstream UI identity remains stable.
    pub fn merge_parent_groups(
        &self,
        collection_id: Uuid,
        group_ids: &[Uuid],
    ) -> Result<PhotoGroup, CatalogError> {
        if group_ids.len() < 2 {
            return Err(CatalogError::MergeNeedsTwoGroups);
        }
        let mut seen_groups = std::collections::HashSet::with_capacity(group_ids.len());
        for group_id in group_ids {
            if !seen_groups.insert(*group_id) {
                return Err(CatalogError::DuplicateGroup(*group_id));
            }
            if self.semantic_parent_group_id(*group_id)?.is_some() {
                return Err(CatalogError::SemanticGroupRequiresParent(*group_id));
            }
        }

        let ordered = self.list_groups_for_collection(collection_id)?;
        let positions = group_ids
            .iter()
            .map(|group_id| {
                ordered
                    .iter()
                    .position(|group| group.id == *group_id)
                    .ok_or(CatalogError::GroupNotInCollection(*group_id))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut sorted_positions = positions.clone();
        sorted_positions.sort_unstable();
        if sorted_positions
            .windows(2)
            .any(|window| window[1] != window[0] + 1)
        {
            return Err(CatalogError::NonAdjacentGroups);
        }

        let mut combined_assets = Vec::new();
        let mut seen_assets = std::collections::HashSet::new();
        for position in sorted_positions {
            for asset_id in &ordered[position].asset_ids {
                if !seen_assets.insert(*asset_id) {
                    return Err(CatalogError::DuplicateAsset(*asset_id));
                }
                combined_assets.push(*asset_id);
            }
        }

        let keep_id = ordered[*positions.iter().min().unwrap()].id;
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        for group_id in group_ids {
            ensure_group_in_collection(&tx, collection_id, *group_id)?;
            tx.execute(
                "DELETE FROM semantic_photo_groups WHERE parent_group_id = ?1",
                [group_id.to_string()],
            )?;
        }

        tx.execute(
            "UPDATE photo_groups SET kind = 'MANUAL', basis = ?2, manual_locked = 1 WHERE id = ?1",
            params![
                keep_id.to_string(),
                grouping_basis_to_db(GroupingBasis::Manual)
            ],
        )?;
        tx.execute(
            "DELETE FROM photo_group_members WHERE group_id = ?1",
            [keep_id.to_string()],
        )?;
        for (position, asset_id) in combined_assets.iter().enumerate() {
            tx.execute(
                "INSERT INTO photo_group_members (group_id, asset_id, position) VALUES (?1, ?2, ?3)",
                params![keep_id.to_string(), asset_id.to_string(), position as i64],
            )?;
        }
        for group_id in group_ids {
            if *group_id == keep_id {
                continue;
            }
            tx.execute(
                "DELETE FROM photo_groups WHERE id = ?1",
                [group_id.to_string()],
            )?;
        }
        tx.commit()?;
        self.find_group(keep_id)?
            .ok_or(CatalogError::GroupNotInCollection(keep_id))
    }

    /// Split a parent group immediately before the chosen asset. Both results
    /// become manual locked groups so future semantic refinement cannot undo the
    /// photographer's correction.
    pub fn split_parent_group(
        &self,
        collection_id: Uuid,
        group_id: Uuid,
        split_before_asset_id: Uuid,
    ) -> Result<(PhotoGroup, PhotoGroup), CatalogError> {
        if self.semantic_parent_group_id(group_id)?.is_some() {
            return Err(CatalogError::SemanticGroupRequiresParent(group_id));
        }

        let parent = self
            .list_groups_for_collection(collection_id)?
            .into_iter()
            .find(|group| group.id == group_id)
            .ok_or(CatalogError::GroupNotInCollection(group_id))?;
        let split_index = parent
            .asset_ids
            .iter()
            .position(|asset_id| *asset_id == split_before_asset_id)
            .filter(|index| *index > 0 && *index < parent.asset_ids.len())
            .ok_or(CatalogError::InvalidSplitAsset {
                group_id,
                asset_id: split_before_asset_id,
            })?;
        let left_assets = parent.asset_ids[..split_index].to_vec();
        let right_assets = parent.asset_ids[split_index..].to_vec();
        let right_id = Uuid::new_v4();

        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        ensure_group_in_collection(&tx, collection_id, group_id)?;
        tx.execute(
            "DELETE FROM semantic_photo_groups WHERE parent_group_id = ?1",
            [group_id.to_string()],
        )?;
        tx.execute(
            "UPDATE photo_groups SET kind = 'MANUAL', basis = ?2, manual_locked = 1 WHERE id = ?1",
            params![
                group_id.to_string(),
                grouping_basis_to_db(GroupingBasis::Manual)
            ],
        )?;
        tx.execute(
            "DELETE FROM photo_group_members WHERE group_id = ?1",
            [group_id.to_string()],
        )?;
        for (position, asset_id) in left_assets.iter().enumerate() {
            tx.execute(
                "INSERT INTO photo_group_members (group_id, asset_id, position) VALUES (?1, ?2, ?3)",
                params![group_id.to_string(), asset_id.to_string(), position as i64],
            )?;
        }

        tx.execute(
            "INSERT INTO photo_groups (id, kind, basis, manual_locked) VALUES (?1, 'MANUAL', ?2, 1)",
            params![
                right_id.to_string(),
                grouping_basis_to_db(GroupingBasis::Manual)
            ],
        )?;
        tx.execute(
            "INSERT INTO photo_group_collections (group_id, collection_id) VALUES (?1, ?2)",
            params![right_id.to_string(), collection_id.to_string()],
        )?;
        for (position, asset_id) in right_assets.iter().enumerate() {
            tx.execute(
                "INSERT INTO photo_group_members (group_id, asset_id, position) VALUES (?1, ?2, ?3)",
                params![right_id.to_string(), asset_id.to_string(), position as i64],
            )?;
        }
        tx.commit()?;

        let left = self
            .find_group(group_id)?
            .ok_or(CatalogError::GroupNotInCollection(group_id))?;
        let right = self
            .find_group(right_id)?
            .ok_or(CatalogError::GroupNotInCollection(right_id))?;
        Ok((left, right))
    }

    pub fn replace_semantic_groups(
        &self,
        parent_group_id: Uuid,
        groups: &[SemanticPhotoGroup],
    ) -> Result<(), CatalogError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;

        for group in groups {
            if group.parent_group_id != parent_group_id {
                return Err(CatalogError::SemanticParentMismatch {
                    group_id: group.id,
                    parent_group_id,
                });
            }
        }

        tx.execute(
            "DELETE FROM semantic_photo_groups WHERE parent_group_id = ?1",
            [parent_group_id.to_string()],
        )?;

        for group in groups {
            tx.execute(
                "INSERT INTO semantic_photo_groups
                 (id, parent_group_id, kind, reference_candidate_id, similarity_threshold)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    group.id.to_string(),
                    parent_group_id.to_string(),
                    semantic_group_kind_to_db(group.kind),
                    group.reference_candidate_id.map(|value| value.to_string()),
                    group.similarity_threshold
                ],
            )?;
            for (position, asset_id) in group.asset_ids.iter().enumerate() {
                tx.execute(
                    "INSERT INTO semantic_photo_group_members (group_id, asset_id, position)
                     VALUES (?1, ?2, ?3)",
                    params![group.id.to_string(), asset_id.to_string(), position as i64],
                )?;
            }
        }

        tx.commit()?;
        Ok(())
    }

    pub fn list_semantic_groups_for_parent(
        &self,
        parent_group_id: Uuid,
    ) -> Result<Vec<SemanticPhotoGroup>, CatalogError> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT id, kind, reference_candidate_id, similarity_threshold
             FROM semantic_photo_groups
             WHERE parent_group_id = ?1
             ORDER BY rowid ASC",
        )?;
        let rows = stmt
            .query_map([parent_group_id.to_string()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, f32>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut groups = Vec::with_capacity(rows.len());
        for (id, kind, reference_candidate_id, similarity_threshold) in rows {
            let group_id = Uuid::parse_str(&id)?;
            let mut members = conn.prepare(
                "SELECT asset_id FROM semantic_photo_group_members
                 WHERE group_id = ?1 ORDER BY position ASC",
            )?;
            let asset_ids = members
                .query_map([id], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .map(|value| Uuid::parse_str(&value))
                .collect::<Result<Vec<_>, _>>()?;
            groups.push(SemanticPhotoGroup {
                id: group_id,
                parent_group_id,
                kind: semantic_group_kind_from_db(&kind)?,
                asset_ids,
                reference_candidate_id: reference_candidate_id
                    .map(|value| Uuid::parse_str(&value))
                    .transpose()?,
                similarity_threshold,
            });
        }
        Ok(groups)
    }

    pub fn list_effective_groups_for_collection(
        &self,
        collection_id: Uuid,
    ) -> Result<Vec<PhotoGroup>, CatalogError> {
        let parents = self.list_groups_for_collection(collection_id)?;
        let mut effective = Vec::new();
        for parent in parents {
            if parent.manual_locked {
                effective.push(parent);
                continue;
            }

            let refined = self.list_semantic_groups_for_parent(parent.id)?;
            if refined.is_empty() {
                effective.push(parent);
            } else {
                effective.extend(refined.into_iter().map(|group| group.to_photo_group()));
            }
        }
        Ok(effective)
    }

    pub fn find_group(&self, group_id: Uuid) -> Result<Option<PhotoGroup>, CatalogError> {
        if let Some(group) = self
            .list_groups()?
            .into_iter()
            .find(|group| group.id == group_id)
        {
            return Ok(Some(group));
        }

        let conn = self.connect()?;
        let row = conn
            .query_row(
                "SELECT parent_group_id, kind, reference_candidate_id, similarity_threshold
                 FROM semantic_photo_groups WHERE id = ?1",
                [group_id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, f32>(3)?,
                    ))
                },
            )
            .optional()?;
        let Some((parent_group_id, kind, reference_candidate_id, similarity_threshold)) = row else {
            return Ok(None);
        };

        let mut members = conn.prepare(
            "SELECT asset_id FROM semantic_photo_group_members
             WHERE group_id = ?1 ORDER BY position ASC",
        )?;
        let asset_ids = members
            .query_map([group_id.to_string()], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|value| Uuid::parse_str(&value))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Some(
            SemanticPhotoGroup {
                id: group_id,
                parent_group_id: Uuid::parse_str(&parent_group_id)?,
                kind: semantic_group_kind_from_db(&kind)?,
                asset_ids,
                reference_candidate_id: reference_candidate_id
                    .map(|value| Uuid::parse_str(&value))
                    .transpose()?,
                similarity_threshold,
            }
            .to_photo_group(),
        ))
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
        self.list_groups_where(None)
    }

    pub fn list_groups_for_collection(
        &self,
        collection_id: Uuid,
    ) -> Result<Vec<PhotoGroup>, CatalogError> {
        self.list_groups_where(Some(collection_id))
    }

    fn list_groups_where(
        &self,
        collection_id: Option<Uuid>,
    ) -> Result<Vec<PhotoGroup>, CatalogError> {
        let conn = self.connect()?;
        let mut group_rows = Vec::new();

        if let Some(collection_id) = collection_id {
            let mut stmt = conn.prepare(
                "SELECT g.id, g.kind, g.basis, g.manual_locked
                 FROM photo_groups g
                 JOIN photo_group_collections c ON c.group_id = g.id
                 WHERE c.collection_id = ?1
                 ORDER BY g.rowid ASC",
            )?;
            let rows = stmt.query_map([collection_id.to_string()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)? != 0,
                ))
            })?;
            group_rows.extend(rows.collect::<Result<Vec<_>, _>>()?);
        } else {
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
            group_rows.extend(rows.collect::<Result<Vec<_>, _>>()?);
        }

        let mut groups = Vec::new();
        for (id, kind, basis, manual_locked) in group_rows {
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

fn ensure_group_in_collection(
    tx: &rusqlite::Transaction<'_>,
    collection_id: Uuid,
    group_id: Uuid,
) -> Result<(), CatalogError> {
    let found = tx
        .query_row(
            "SELECT 1 FROM photo_group_collections WHERE collection_id = ?1 AND group_id = ?2",
            params![collection_id.to_string(), group_id.to_string()],
            |_| Ok(()),
        )
        .optional()?;
    if found.is_none() {
        return Err(CatalogError::GroupNotInCollection(group_id));
    }
    Ok(())
}

fn load_parent_group_row(
    tx: &rusqlite::Transaction<'_>,
    collection_id: Uuid,
    group_id: Uuid,
) -> Result<(PhotoGroupKind, GroupingBasis), CatalogError> {
    ensure_group_in_collection(tx, collection_id, group_id)?;
    let (kind, basis) = tx.query_row(
        "SELECT kind, basis FROM photo_groups WHERE id = ?1",
        [group_id.to_string()],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )?;
    Ok((group_kind_from_db(&kind)?, grouping_basis_from_db(&basis)?))
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
        GroupingBasis::SemanticSimilarity => "SEMANTIC_SIMILARITY",
        GroupingBasis::Manual => "MANUAL",
        GroupingBasis::Singleton => "SINGLETON",
    }
}

fn semantic_group_kind_to_db(value: SemanticGroupKind) -> &'static str {
    match value {
        SemanticGroupKind::PortraitSimilar => "PORTRAIT_SIMILAR",
        SemanticGroupKind::SceneSimilar => "SCENE_SIMILAR",
    }
}

fn semantic_group_kind_from_db(value: &str) -> Result<SemanticGroupKind, CatalogError> {
    match value {
        "PORTRAIT_SIMILAR" => Ok(SemanticGroupKind::PortraitSimilar),
        "SCENE_SIMILAR" => Ok(SemanticGroupKind::SceneSimilar),
        other => Err(CatalogError::InvalidSemanticGroupKind(other.to_string())),
    }
}

fn grouping_basis_from_db(value: &str) -> Result<GroupingBasis, CatalogError> {
    match value {
        "TIME" => Ok(GroupingBasis::Time),
        "TIME_AND_SEQUENCE" => Ok(GroupingBasis::TimeAndSequence),
        "SEQUENCE_FALLBACK" => Ok(GroupingBasis::SequenceFallback),
        "SEMANTIC_SIMILARITY" => Ok(GroupingBasis::SemanticSimilarity),
        "MANUAL" => Ok(GroupingBasis::Manual),
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
    fn reimport_does_not_erase_existing_capture_metadata_when_new_read_is_missing() {
        let dir = tempdir().unwrap();
        let catalog = RawCatalog::open(dir.path().join("catalog.sqlite3")).unwrap();
        let source = "C:/shoot/IMG_2001.CR3";

        let original = RawAsset {
            id: Uuid::new_v4(),
            source_path: source.to_string(),
            filename: "IMG_2001.CR3".into(),
            extension: "cr3".into(),
            camera_id: Some("Canon EOS R5".into()),
            capture_time_ms: Some(1_700_000_000_000),
            file_time_ms: Some(1_700_000_100_000),
            sequence_number: Some(2001),
        };
        let first = catalog.ensure_assets(&[original]).unwrap().remove(0);

        let missing_metadata = RawAsset {
            id: Uuid::new_v4(),
            source_path: source.to_string(),
            filename: "IMG_2001.CR3".into(),
            extension: "cr3".into(),
            camera_id: None,
            capture_time_ms: None,
            file_time_ms: Some(1_700_000_200_000),
            sequence_number: Some(2001),
        };
        let second = catalog.ensure_assets(&[missing_metadata]).unwrap().remove(0);

        assert_eq!(second.id, first.id);
        assert_eq!(second.camera_id.as_deref(), Some("Canon EOS R5"));
        assert_eq!(second.capture_time_ms, Some(1_700_000_000_000));
        assert_eq!(second.file_time_ms, Some(1_700_000_200_000));
    }

    #[test]
    fn semantic_grouping_basis_round_trips() {
        let dir = tempdir().unwrap();
        let catalog = RawCatalog::open(dir.path().join("catalog.sqlite3")).unwrap();
        let asset = catalog
            .ensure_assets(&[sample_asset("C:/shoot/IMG_3001.CR3", 3001)])
            .unwrap()
            .remove(0);
        let collection = Uuid::new_v4();
        let group = PhotoGroup {
            id: Uuid::new_v4(),
            kind: PhotoGroupKind::Similar,
            basis: GroupingBasis::SemanticSimilarity,
            asset_ids: vec![asset.id],
            manual_locked: false,
        };

        catalog.replace_automatic_groups(collection, &[group.clone()]).unwrap();
        let loaded = catalog.list_groups_for_collection(collection).unwrap();
        assert_eq!(loaded, vec![group]);
    }

    #[test]
    fn semantic_children_persist_without_destroying_moment_parent() {
        let dir = tempdir().unwrap();
        let catalog = RawCatalog::open(dir.path().join("catalog.sqlite3")).unwrap();
        let assets = catalog
            .ensure_assets(&[
                sample_asset("C:/shoot/IMG_4001.CR3", 4001),
                sample_asset("C:/shoot/IMG_4002.CR3", 4002),
            ])
            .unwrap();
        let collection = Uuid::new_v4();
        let parent = initial_group_raw_assets(&assets, InitialGroupingConfig::default())
            .remove(0);
        catalog
            .replace_automatic_groups(collection, &[parent.clone()])
            .unwrap();

        let child = SemanticPhotoGroup {
            id: Uuid::new_v4(),
            parent_group_id: parent.id,
            kind: SemanticGroupKind::SceneSimilar,
            asset_ids: assets.iter().map(|asset| asset.id).collect(),
            reference_candidate_id: Some(assets[0].id),
            similarity_threshold: 0.84,
        };
        catalog
            .replace_semantic_groups(parent.id, &[child.clone()])
            .unwrap();

        assert_eq!(
            catalog.list_groups_for_collection(collection).unwrap(),
            vec![parent]
        );
        assert_eq!(
            catalog.list_semantic_groups_for_parent(child.parent_group_id).unwrap(),
            vec![child.clone()]
        );
        assert_eq!(
            catalog.list_effective_groups_for_collection(collection).unwrap(),
            vec![child.to_photo_group()]
        );
    }

    #[test]
    fn find_group_resolves_persisted_semantic_child_for_downstream_workflow() {
        let dir = tempdir().unwrap();
        let catalog = RawCatalog::open(dir.path().join("catalog.sqlite3")).unwrap();
        let assets = catalog
            .ensure_assets(&[
                sample_asset("C:/shoot/IMG_4501.CR3", 4501),
                sample_asset("C:/shoot/IMG_4502.CR3", 4502),
            ])
            .unwrap();
        let collection = Uuid::new_v4();
        let parent = initial_group_raw_assets(&assets, InitialGroupingConfig::default())
            .remove(0);
        catalog
            .replace_automatic_groups(collection, &[parent.clone()])
            .unwrap();

        let child = SemanticPhotoGroup {
            id: Uuid::new_v4(),
            parent_group_id: parent.id,
            kind: SemanticGroupKind::SceneSimilar,
            asset_ids: assets.iter().map(|asset| asset.id).collect(),
            reference_candidate_id: Some(assets[0].id),
            similarity_threshold: 0.84,
        };
        catalog
            .replace_semantic_groups(parent.id, &[child.clone()])
            .unwrap();

        assert_eq!(
            catalog.find_group(child.id).unwrap(),
            Some(child.to_photo_group())
        );
    }

    #[test]
    fn replacing_parent_groups_cascades_stale_semantic_children() {
        let dir = tempdir().unwrap();
        let catalog = RawCatalog::open(dir.path().join("catalog.sqlite3")).unwrap();
        let assets = catalog
            .ensure_assets(&[sample_asset("C:/shoot/IMG_5001.CR3", 5001)])
            .unwrap();
        let collection = Uuid::new_v4();
        let parent = initial_group_raw_assets(&assets, InitialGroupingConfig::default())
            .remove(0);
        catalog
            .replace_automatic_groups(collection, &[parent.clone()])
            .unwrap();
        catalog
            .replace_semantic_groups(
                parent.id,
                &[SemanticPhotoGroup {
                    id: Uuid::new_v4(),
                    parent_group_id: parent.id,
                    kind: SemanticGroupKind::SceneSimilar,
                    asset_ids: vec![assets[0].id],
                    reference_candidate_id: Some(assets[0].id),
                    similarity_threshold: 0.84,
                }],
            )
            .unwrap();

        let replacement = PhotoGroup {
            id: Uuid::new_v4(),
            kind: PhotoGroupKind::Moment,
            basis: GroupingBasis::Time,
            asset_ids: vec![assets[0].id],
            manual_locked: false,
        };
        catalog
            .replace_automatic_groups(collection, &[replacement.clone()])
            .unwrap();

        assert!(catalog
            .list_semantic_groups_for_parent(parent.id)
            .unwrap()
            .is_empty());
        assert_eq!(
            catalog.list_effective_groups_for_collection(collection).unwrap(),
            vec![replacement]
        );
    }

    #[test]
    fn groups_are_scoped_to_collection() {
        let dir = tempdir().unwrap();
        let catalog = RawCatalog::open(dir.path().join("catalog.sqlite3")).unwrap();
        let first_assets = catalog
            .ensure_assets(&[
                sample_asset("C:/shoot-a/IMG_1001.CR3", 1001),
                sample_asset("C:/shoot-a/IMG_1002.CR3", 1002),
            ])
            .unwrap();
        let second_assets = catalog
            .ensure_assets(&[
                sample_asset("C:/shoot-b/IMG_2001.CR3", 2001),
                sample_asset("C:/shoot-b/IMG_2002.CR3", 2002),
            ])
            .unwrap();

        let first_collection = Uuid::new_v4();
        let second_collection = Uuid::new_v4();
        let first_groups = initial_group_raw_assets(&first_assets, InitialGroupingConfig::default());
        let second_groups = initial_group_raw_assets(&second_assets, InitialGroupingConfig::default());
        catalog
            .replace_automatic_groups(first_collection, &first_groups)
            .unwrap();
        catalog
            .replace_automatic_groups(second_collection, &second_groups)
            .unwrap();

        let loaded_first = catalog.list_groups_for_collection(first_collection).unwrap();
        let loaded_second = catalog.list_groups_for_collection(second_collection).unwrap();
        assert_eq!(loaded_first.len(), 1);
        assert_eq!(loaded_second.len(), 1);
        assert_eq!(loaded_first[0].asset_ids, first_groups[0].asset_ids);
        assert_eq!(loaded_second[0].asset_ids, second_groups[0].asset_ids);
    }
    #[test]
    fn semantic_child_can_be_reverted_to_locked_parent() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("catalog.sqlite3");
        let catalog = RawCatalog::open(&db).unwrap();
        let assets = catalog
            .ensure_assets(&[
                sample_asset("C:/shoot/A.CR3", 1),
                sample_asset("C:/shoot/B.CR3", 2),
            ])
            .unwrap();
        let collection = Uuid::new_v4();
        let parent = PhotoGroup {
            id: Uuid::new_v4(),
            kind: PhotoGroupKind::Moment,
            basis: GroupingBasis::Time,
            asset_ids: assets.iter().map(|asset| asset.id).collect(),
            manual_locked: false,
        };
        catalog.replace_automatic_groups(collection, std::slice::from_ref(&parent)).unwrap();
        let child = SemanticPhotoGroup {
            id: Uuid::new_v4(),
            parent_group_id: parent.id,
            kind: SemanticGroupKind::SceneSimilar,
            asset_ids: vec![assets[0].id],
            reference_candidate_id: None,
            similarity_threshold: 0.9,
        };
        let child_two = SemanticPhotoGroup {
            id: Uuid::new_v4(),
            parent_group_id: parent.id,
            kind: SemanticGroupKind::SceneSimilar,
            asset_ids: vec![assets[1].id],
            reference_candidate_id: None,
            similarity_threshold: 0.9,
        };
        catalog
            .replace_semantic_groups(parent.id, &[child.clone(), child_two])
            .unwrap();

        let kept = catalog.keep_moment_together(collection, child.id).unwrap();

        assert_eq!(kept.id, parent.id);
        assert!(kept.manual_locked);
        assert_eq!(kept.asset_ids, parent.asset_ids);
        assert!(catalog.list_semantic_groups_for_parent(parent.id).unwrap().is_empty());
    }

    #[test]
    fn adjacent_parent_groups_merge_transactionally() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("catalog.sqlite3");
        let catalog = RawCatalog::open(&db).unwrap();
        let assets = catalog
            .ensure_assets(&[
                sample_asset("C:/shoot/A.CR3", 1),
                sample_asset("C:/shoot/B.CR3", 2),
                sample_asset("C:/shoot/C.CR3", 30),
            ])
            .unwrap();
        let collection = Uuid::new_v4();
        let groups = vec![
            PhotoGroup {
                id: Uuid::new_v4(),
                kind: PhotoGroupKind::Moment,
                basis: GroupingBasis::Time,
                asset_ids: vec![assets[0].id, assets[1].id],
                manual_locked: false,
            },
            PhotoGroup {
                id: Uuid::new_v4(),
                kind: PhotoGroupKind::Moment,
                basis: GroupingBasis::Singleton,
                asset_ids: vec![assets[2].id],
                manual_locked: false,
            },
        ];
        catalog.replace_automatic_groups(collection, &groups).unwrap();

        let merged = catalog
            .merge_parent_groups(collection, &[groups[0].id, groups[1].id])
            .unwrap();

        assert_eq!(merged.id, groups[0].id);
        assert_eq!(merged.kind, PhotoGroupKind::Manual);
        assert_eq!(merged.basis, GroupingBasis::Manual);
        assert!(merged.manual_locked);
        assert_eq!(
            merged.asset_ids,
            assets.iter().map(|asset| asset.id).collect::<Vec<_>>()
        );
        assert_eq!(catalog.list_groups_for_collection(collection).unwrap().len(), 1);
    }

    #[test]
    fn non_adjacent_parent_groups_do_not_merge() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("catalog.sqlite3");
        let catalog = RawCatalog::open(&db).unwrap();
        let assets = catalog
            .ensure_assets(&[
                sample_asset("C:/shoot/A.CR3", 1),
                sample_asset("C:/shoot/B.CR3", 20),
                sample_asset("C:/shoot/C.CR3", 40),
            ])
            .unwrap();
        let collection = Uuid::new_v4();
        let groups = assets
            .iter()
            .map(|asset| PhotoGroup {
                id: Uuid::new_v4(),
                kind: PhotoGroupKind::Moment,
                basis: GroupingBasis::Singleton,
                asset_ids: vec![asset.id],
                manual_locked: false,
            })
            .collect::<Vec<_>>();
        catalog.replace_automatic_groups(collection, &groups).unwrap();

        assert!(matches!(
            catalog
                .merge_parent_groups(collection, &[groups[0].id, groups[2].id])
                .unwrap_err(),
            CatalogError::NonAdjacentGroups
        ));
        assert_eq!(catalog.list_groups_for_collection(collection).unwrap().len(), 3);
    }

    #[test]
    fn split_group_creates_two_locked_manual_groups() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("catalog.sqlite3");
        let catalog = RawCatalog::open(&db).unwrap();
        let assets = catalog
            .ensure_assets(&[
                sample_asset("C:/shoot/A.CR3", 1),
                sample_asset("C:/shoot/B.CR3", 2),
                sample_asset("C:/shoot/C.CR3", 3),
            ])
            .unwrap();
        let collection = Uuid::new_v4();
        let parent = PhotoGroup {
            id: Uuid::new_v4(),
            kind: PhotoGroupKind::Moment,
            basis: GroupingBasis::Time,
            asset_ids: assets.iter().map(|asset| asset.id).collect(),
            manual_locked: false,
        };
        catalog.replace_automatic_groups(collection, std::slice::from_ref(&parent)).unwrap();

        let (left, right) = catalog
            .split_parent_group(collection, parent.id, assets[1].id)
            .unwrap();

        assert_eq!(left.id, parent.id);
        assert_eq!(left.asset_ids, vec![assets[0].id]);
        assert_eq!(right.asset_ids, vec![assets[1].id, assets[2].id]);
        assert_eq!(left.kind, PhotoGroupKind::Manual);
        assert_eq!(right.kind, PhotoGroupKind::Manual);
        assert_eq!(left.basis, GroupingBasis::Manual);
        assert_eq!(right.basis, GroupingBasis::Manual);
        assert!(left.manual_locked && right.manual_locked);
    }

    #[test]
    fn automatic_parent_lock_can_be_reopened_but_manual_group_cannot() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("catalog.sqlite3");
        let catalog = RawCatalog::open(&db).unwrap();
        let assets = catalog
            .ensure_assets(&[
                sample_asset("C:/shoot/A.CR3", 1),
                sample_asset("C:/shoot/B.CR3", 2),
            ])
            .unwrap();
        let collection = Uuid::new_v4();
        let parent = PhotoGroup {
            id: Uuid::new_v4(),
            kind: PhotoGroupKind::Moment,
            basis: GroupingBasis::Time,
            asset_ids: assets.iter().map(|asset| asset.id).collect(),
            manual_locked: false,
        };
        catalog.replace_automatic_groups(collection, std::slice::from_ref(&parent)).unwrap();

        let locked = catalog.keep_moment_together(collection, parent.id).unwrap();
        assert!(locked.manual_locked);
        let unlocked = catalog.allow_parent_refinement(collection, parent.id).unwrap();
        assert!(!unlocked.manual_locked);

        let (manual, _) = catalog
            .split_parent_group(collection, parent.id, assets[1].id)
            .unwrap();
        assert!(matches!(
            catalog.allow_parent_refinement(collection, manual.id).unwrap_err(),
            CatalogError::ManualGroupCannotUnlock(id) if id == manual.id
        ));
    }

}
