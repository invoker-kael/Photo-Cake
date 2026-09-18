use crate::{BatchExportExecutor, BatchExportItem, BatchItem, BatchStage, ExportRenderer, ExportWorkerError};
use thiserror::Error;
use uuid::Uuid;
use std::path::PathBuf;

#[derive(Debug, Error)]
pub enum ExportStageError {
    #[error("stage is not export: {0:?}")]
    InvalidStage(BatchStage),
    #[error("export failed: {0}")]
    Export(#[from] ExportWorkerError),
}

/// Adapter used by the batch runner when the current stage reaches Export.
///
/// The runner owns batch state progression. This adapter only performs the
/// export operation and keeps earlier stages reusable.
pub struct ExportStageExecutor<R> {
    executor: BatchExportExecutor<R>,
}

impl<R> ExportStageExecutor<R>
where
    R: ExportRenderer,
{
    pub fn new(executor: BatchExportExecutor<R>) -> Self {
        Self { executor }
    }

    pub fn execute_item(&self, item: &BatchItem) -> Result<(), ExportStageError> {
        if item.stage != BatchStage::Export {
            return Err(ExportStageError::InvalidStage(item.stage));
        }

        self.executor
            .execute(BatchExportItem {
                batch_id: Uuid::nil(),
                item_id: item.id,
                source: PathBuf::from(&item.source_path),
            })
            .map(|_| ())
            .map_err(|error| ExportStageError::Export(ExportWorkerError::Output(error.to_string())))
    }
}
