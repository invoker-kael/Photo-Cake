use std::path::PathBuf;

use thiserror::Error;
use uuid::Uuid;

use crate::{
    ExportCheckpoint, ExportJob, ExportRenderer, ExportStore, ExportStoreError, ExportWorker,
    ExportWorkerError,
};

#[derive(Debug, Clone)]
pub struct BatchExportItem {
    pub batch_id: Uuid,
    pub item_id: Uuid,
    pub source: PathBuf,
}

#[derive(Debug, Error)]
pub enum BatchExportError {
    #[error(transparent)]
    Store(#[from] ExportStoreError),
    #[error(transparent)]
    Worker(#[from] ExportWorkerError),
}

/// Connects batch export execution to the persistent export checkpoint store.
///
/// The executor is deliberately scoped to the export stage. It does not own or
/// invalidate any upstream import, analysis, grouping, preset, retouch, or QA state.
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

    pub fn execute(
        &self,
        item: BatchExportItem,
    ) -> Result<ExportCheckpoint, BatchExportError> {
        let job = ExportJob {
            batch_id: item.batch_id,
            item_id: item.item_id,
            source: item.source,
        };
        Ok(self.worker.run_job(&self.store, &job)?)
    }

    pub fn recover_interrupted(&self) -> Result<usize, BatchExportError> {
        Ok(self.store.recover_interrupted()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ExportColorSpace, ExportFormat, ExportRecipe, ExportStatus};
    use std::fs;
    use std::sync::{Arc, Mutex};
    use tempfile::tempdir;

    struct MockRenderer;

    impl ExportRenderer for MockRenderer {
        fn render(
            &self,
            _source: &std::path::Path,
            destination: &std::path::Path,
        ) -> Result<(), ExportWorkerError> {
            fs::write(destination, b"jpeg")
                .map_err(|error| ExportWorkerError::Output(error.to_string()))
        }
    }

    struct FailingRenderer;

    impl ExportRenderer for FailingRenderer {
        fn render(
            &self,
            _source: &std::path::Path,
            destination: &std::path::Path,
        ) -> Result<(), ExportWorkerError> {
            fs::write(destination, b"partial")
                .map_err(|error| ExportWorkerError::Output(error.to_string()))?;
            Err(ExportWorkerError::Render("renderer failed".to_string()))
        }
    }

    struct FailOnceRenderer {
        calls: Arc<Mutex<u32>>,
    }

    impl ExportRenderer for FailOnceRenderer {
        fn render(
            &self,
            _source: &std::path::Path,
            destination: &std::path::Path,
        ) -> Result<(), ExportWorkerError> {
            let mut calls = self.calls.lock().unwrap();
            *calls += 1;
            if *calls == 1 {
                fs::write(destination, b"partial")
                    .map_err(|error| ExportWorkerError::Output(error.to_string()))?;
                return Err(ExportWorkerError::Render("temporary failure".to_string()));
            }
            fs::write(destination, b"jpeg")
                .map_err(|error| ExportWorkerError::Output(error.to_string()))
        }
    }

    fn recipe(destination: std::path::PathBuf) -> ExportRecipe {
        ExportRecipe {
            format: ExportFormat::Jpeg,
            jpeg_quality: Some(92),
            tiff_bit_depth: None,
            color_space: ExportColorSpace::Srgb,
            resize: None,
            preserve_metadata: true,
            destination,
            qa_approved_only: true,
        }
    }

    fn fixture() -> (
        tempfile::TempDir,
        ExportStore,
        BatchExportItem,
    ) {
        let dir = tempdir().unwrap();
        let raw = dir.path().join("IMG_0001.CR3");
        let output = dir.path().join("exports");
        fs::create_dir_all(&output).unwrap();
        fs::write(&raw, b"immutable raw source").unwrap();

        let store = ExportStore::open(dir.path().join("project.sqlite3")).unwrap();
        let batch_id = Uuid::new_v4();
        let item_id = Uuid::new_v4();
        store.set_recipe(batch_id, &recipe(output)).unwrap();

        (
            dir,
            store,
            BatchExportItem {
                batch_id,
                item_id,
                source: raw,
            },
        )
    }

    #[test]
    fn successful_batch_export_persists_done_checkpoint_and_keeps_raw() {
        let (dir, store, item) = fixture();
        let before = fs::read(&item.source).unwrap();

        let checkpoint = BatchExportExecutor::new(MockRenderer, store.clone())
            .execute(item.clone())
            .unwrap();

        assert_eq!(checkpoint.status, ExportStatus::Done);
        assert_eq!(checkpoint.attempts, 1);
        assert_eq!(fs::read(&checkpoint.output_path).unwrap(), b"jpeg");
        assert_eq!(store.load_checkpoint(item.batch_id, item.item_id).unwrap(), Some(checkpoint));
        assert_eq!(fs::read(item.source).unwrap(), before);
        drop(dir);
    }

    #[test]
    fn renderer_failure_marks_failed_and_cleans_partial_output() {
        let (dir, store, item) = fixture();

        let result = BatchExportExecutor::new(FailingRenderer, store.clone()).execute(item.clone());
        assert!(matches!(
            result,
            Err(BatchExportError::Worker(ExportWorkerError::Render(_)))
        ));

        let checkpoint = store.load_checkpoint(item.batch_id, item.item_id).unwrap().unwrap();
        assert_eq!(checkpoint.status, ExportStatus::Failed);
        assert_eq!(checkpoint.attempts, 1);
        assert!(!checkpoint.output_path.exists());
        assert!(!checkpoint.output_path.with_extension("partial").exists());
        drop(dir);
    }

    #[test]
    fn retry_after_failure_reuses_same_output_reservation() {
        let (dir, store, item) = fixture();
        let calls = Arc::new(Mutex::new(0));
        let executor = BatchExportExecutor::new(
            FailOnceRenderer {
                calls: calls.clone(),
            },
            store.clone(),
        );

        let first_error = executor.execute(item.clone()).unwrap_err();
        assert!(matches!(
            first_error,
            BatchExportError::Worker(ExportWorkerError::Render(_))
        ));
        let failed = store
            .load_checkpoint(item.batch_id, item.item_id)
            .unwrap()
            .unwrap();

        let completed = executor.execute(item.clone()).unwrap();
        assert_eq!(completed.status, ExportStatus::Done);
        assert_eq!(completed.attempts, 2);
        assert_eq!(completed.output_path, failed.output_path);
        assert_eq!(*calls.lock().unwrap(), 2);
        drop(dir);
    }
}
