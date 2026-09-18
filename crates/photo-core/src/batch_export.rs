use std::path::PathBuf;

use uuid::Uuid;

use crate::{ExportJob, ExportRenderer, ExportStore, ExportStoreError, ExportWorker, ExportWorkerError};

#[derive(Debug, Clone)]
pub struct BatchExportItem {
    pub batch_id: Uuid,
    pub item_id: Uuid,
    pub source: PathBuf,
}

#[derive(Debug)]
pub enum BatchExportError {
    Store(ExportStoreError),
    Worker(ExportWorkerError),
}

impl From<ExportStoreError> for BatchExportError {
    fn from(value: ExportStoreError) -> Self { Self::Store(value) }
}

impl From<ExportWorkerError> for BatchExportError {
    fn from(value: ExportWorkerError) -> Self { Self::Worker(value) }
}

/// Connects batch execution with persistent export state.
///
/// Export is isolated from previous AI/edit stages. A failed export resumes
/// from its checkpoint instead of invalidating upstream work.
pub struct BatchExportExecutor<R> {
    worker: ExportWorker<R>,
    store: ExportStore,
}

impl<R> BatchExportExecutor<R>
where
    R: ExportRenderer,
{
    pub fn new(renderer: R, store: ExportStore) -> Self {
        Self {
            worker: ExportWorker::new(renderer),
            store,
        }
    }

    pub fn execute(&self, item: BatchExportItem) -> Result<(), BatchExportError> {
        let checkpoint = self.store.reserve(item.batch_id, item.item_id, &item.source)?;
        if matches!(checkpoint.status, crate::ExportStatus::Done) {
            return Ok(());
        }

        let running = self.store.mark_running(checkpoint)?;
        let job = ExportJob {
            photo_id: item.item_id.to_string(),
            source: item.source,
            output: running.output_path.clone(),
        };

        match self.worker.run_job(&job) {
            Ok(()) => {
                self.store.mark_done(running)?;
                Ok(())
            }
            Err(error) => {
                self.store.mark_failed(running, error.to_string())?;
                Err(error.into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ExportColorSpace, ExportFormat, ExportRecipe};
    use std::fs;
    use tempfile::tempdir;

    struct MockRenderer;

    impl ExportRenderer for MockRenderer {
        fn render(&self, _source: &std::path::Path, destination: &std::path::Path) -> Result<(), ExportWorkerError> {
            fs::write(destination, b"jpeg").map_err(|e| ExportWorkerError::Output(e.to_string()))
        }
    }

    #[test]
    fn export_updates_checkpoint_and_keeps_raw() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("export.sqlite3");
        let raw = dir.path().join("a.cr3");
        let out = dir.path().join("out");
        fs::create_dir_all(&out).unwrap();
        fs::write(&raw, b"immutable raw").unwrap();
        let before = fs::read(&raw).unwrap();

        let store = ExportStore::open(&db).unwrap();
        let batch = Uuid::new_v4();
        let item = Uuid::new_v4();
        store.set_recipe(batch, &ExportRecipe {
            format: ExportFormat::Jpeg,
            jpeg_quality: Some(92),
            tiff_bit_depth: None,
            color_space: ExportColorSpace::Srgb,
            resize: None,
            preserve_metadata: true,
            destination: out,
            qa_approved_only: true,
        }).unwrap();

        BatchExportExecutor::new(MockRenderer, store.clone())
            .execute(BatchExportItem { batch_id: batch, item_id: item, source: raw.clone() })
            .unwrap();

        assert_eq!(fs::read(raw).unwrap(), before);
    }
}
