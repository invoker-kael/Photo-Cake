use crate::{
    initial_group_raw_assets, Batch, BatchStore, CatalogError, InitialGroupingConfig, PhotoGroup,
    RawAsset, RawCatalog, RawImportScan, RunnerError,
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

        let all_assets = self.catalog.list_assets()?;
        let groups = initial_group_raw_assets(&all_assets, self.grouping);
        self.catalog.replace_automatic_groups(&groups)?;

        let batch = if assets.is_empty() {
            None
        } else {
            let batch = Batch::new(
                batch_name,
                assets.iter().map(|asset| asset.source_path.clone()),
            );
            self.batch_store
                .create_batch(&batch)
                .map_err(RunnerError::from)?;
            Some(batch)
        };

        Ok(RawImportResult {
            assets,
            groups,
            batch,
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
        let stable_id = first.assets[0].id;

        let second = importer.import_paths("second", vec![raw]).unwrap();
        assert_eq!(second.assets[0].id, stable_id);
    }
}
