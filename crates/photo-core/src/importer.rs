use crate::{
    initial_group_raw_assets, Batch, BatchStage, BatchStore, CatalogError, InitialGroupingConfig,
    PhotoGroup, RawAsset, RawCatalog, RawImportScan, RunnerError,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawImportResult {
    pub assets: Vec<RawAsset>,
    pub groups: Vec<PhotoGroup>,
    pub batch: Option<Batch>,
    pub skipped_non_raw: Vec<String>,
}

#[derive(Debug, Error)]
pub enum RawImportError {
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    #[error(transparent)]
    Runner(#[from] RunnerError),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone)]
pub struct RawImporter {
    catalog: RawCatalog,
    batch_store: BatchStore,
    grouping: InitialGroupingConfig,
}

impl RawImporter {
    pub fn open(project_db: impl AsRef<Path>) -> Result<Self, RawImportError> {
        let path = project_db.as_ref();
        Ok(Self {
            catalog: RawCatalog::open(path)?,
            batch_store: BatchStore::open(path).map_err(RunnerError::from)?,
            grouping: InitialGroupingConfig::default(),
        })
    }

    pub fn with_grouping_config(mut self, grouping: InitialGroupingConfig) -> Self {
        self.grouping = grouping;
        self
    }

    pub fn import_paths(
        &self,
        batch_name: impl Into<String>,
        paths: impl IntoIterator<Item = PathBuf>,
    ) -> Result<RawImportResult, RawImportError> {
        let scan = crate::scan_raw_paths(paths);
        self.persist_scan(batch_name.into(), scan)
    }

    pub fn import_directory(
        &self,
        batch_name: impl Into<String>,
        directory: impl AsRef<Path>,
        recursive: bool,
    ) -> Result<RawImportResult, RawImportError> {
        let scan = crate::scan_raw_directory(directory, recursive)?;
        self.persist_scan(batch_name.into(), scan)
    }

    fn persist_scan(
        &self,
        batch_name: String,
        scan: RawImportScan,
    ) -> Result<RawImportResult, RawImportError> {
        let assets = self.catalog.ensure_assets(&scan.assets)?;

        if assets.is_empty() {
            return Ok(RawImportResult {
                assets,
                groups: Vec::new(),
                batch: None,
                skipped_non_raw: scan.skipped_non_raw,
            });
        }

        let batch = Batch::from_imported(
            batch_name,
            assets.iter().map(|asset| asset.source_path.clone()),
        );
        debug_assert!(
            batch.items.iter().all(|item| item.stage == BatchStage::Analyze),
            "RAW ingest already completed import stage"
        );
        self.batch_store
            .create_batch(&batch)
            .map_err(RunnerError::from)?;

        let groups = initial_group_raw_assets(&assets, self.grouping);
        self.catalog
            .replace_automatic_groups(batch.id, &groups)?;

        Ok(RawImportResult {
            assets,
            groups,
            batch: Some(batch),
            skipped_non_raw: scan.skipped_non_raw,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn raw_import_filters_non_raw_persists_ids_and_creates_batch() {
        let dir = tempdir().unwrap();
        let project = dir.path().join("photo-cake.sqlite3");
        let files = dir.path().join("shoot");
        std::fs::create_dir_all(&files).unwrap();
        let raw = files.join("IMG_1001.CR3");
        let jpeg = files.join("IMG_1001.JPG");
        std::fs::write(&raw, b"raw").unwrap();
        std::fs::write(&jpeg, b"jpeg").unwrap();

        let importer = RawImporter::open(&project).unwrap();
        let first = importer
            .import_paths("first", vec![raw.clone(), jpeg.clone()])
            .unwrap();
        assert_eq!(first.assets.len(), 1);
        assert_eq!(first.skipped_non_raw.len(), 1);
        assert_eq!(first.batch.as_ref().unwrap().items.len(), 1);
        assert_eq!(first.batch.as_ref().unwrap().items[0].stage, BatchStage::Analyze);
        assert_eq!(first.groups.len(), 1);
        let stable_id = first.assets[0].id;

        let second = importer.import_paths("second", vec![raw]).unwrap();
        assert_eq!(second.assets[0].id, stable_id);
        assert_ne!(
            first.batch.as_ref().unwrap().id,
            second.batch.as_ref().unwrap().id
        );
    }

    #[test]
    fn separate_imports_keep_separate_group_collections() {
        let dir = tempdir().unwrap();
        let project = dir.path().join("photo-cake.sqlite3");
        let first_dir = dir.path().join("shoot-a");
        let second_dir = dir.path().join("shoot-b");
        std::fs::create_dir_all(&first_dir).unwrap();
        std::fs::create_dir_all(&second_dir).unwrap();
        let first = first_dir.join("IMG_1001.CR3");
        let second = second_dir.join("IMG_2001.CR3");
        std::fs::write(&first, b"raw").unwrap();
        std::fs::write(&second, b"raw").unwrap();

        let importer = RawImporter::open(&project).unwrap();
        let a = importer.import_paths("a", vec![first]).unwrap();
        let b = importer.import_paths("b", vec![second]).unwrap();
        assert_eq!(a.groups.len(), 1);
        assert_eq!(b.groups.len(), 1);
        assert_ne!(a.groups[0].id, b.groups[0].id);
    }
}
