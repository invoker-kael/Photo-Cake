use std::path::{Path, PathBuf};

use thiserror::Error;
use uuid::Uuid;

use crate::{ExportCheckpoint, ExportStatus, ExportStore};

#[derive(Debug, Error)]
pub enum ExportWorkerError {
    #[error("export renderer failed: {0}")]
    Render(String),
    #[error("export store failed: {0}")]
    Store(String),
    #[error("output operation failed: {0}")]
    Output(String),
    #[error("output verification failed: {0}")]
    Verify(String),
}

pub trait ExportRenderer {
    fn render(&self, source: &Path, destination: &Path) -> Result<(), ExportWorkerError>;
}

#[derive(Debug, Clone)]
pub struct ExportJob {
    pub batch_id: Uuid,
    pub item_id: Uuid,
    pub source: PathBuf,
}

/// Crash-safe export coordinator.
///
/// Export execution owns only the final derivative stage. Previous stages
/// (import, analysis, embedding, grouping and retouch) are never invalidated
/// by an export retry.
pub struct ExportWorker<R> {
    renderer: R,
}

impl<R> ExportWorker<R>
where
    R: ExportRenderer,
{
    pub fn new(renderer: R) -> Self {
        Self { renderer }
    }

    pub fn run_job(
        &self,
        store: &ExportStore,
        job: &ExportJob,
    ) -> Result<ExportCheckpoint, ExportWorkerError> {
        let checkpoint = store
            .reserve(job.batch_id, job.item_id, &job.source)
            .map_err(|e| ExportWorkerError::Store(e.to_string()))?;

        let running = store
            .mark_running(checkpoint)
            .map_err(|e| ExportWorkerError::Store(e.to_string()))?;

        let temporary = running.output_path.with_extension("partial");

        let result = self.renderer.render(&job.source, &temporary);

        if let Err(error) = result {
            let _ = store.mark_failed(running, error.to_string());
            return Err(error);
        }

        std::fs::rename(&temporary, &running.output_path)
            .map_err(|e| ExportWorkerError::Output(e.to_string()))?;

        let metadata = std::fs::metadata(&running.output_path)
            .map_err(|e| ExportWorkerError::Verify(e.to_string()))?;
        if metadata.len() == 0 {
            let failed = store
                .mark_failed(running, "empty exported file")
                .map_err(|e| ExportWorkerError::Store(e.to_string()))?;
            return Err(ExportWorkerError::Verify(format!("{}", failed.output_path.display())));
        }

        store
            .mark_done(running)
            .map_err(|e| ExportWorkerError::Store(e.to_string()))
    }
}

pub fn recover_interrupted_export(status: ExportStatus) -> ExportStatus {
    match status {
        ExportStatus::Running => ExportStatus::Pending,
        other => other,
    }
}
