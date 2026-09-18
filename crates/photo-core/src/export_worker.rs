use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::{ExportStatus, ExportStore};

#[derive(Debug, Error)]
pub enum ExportWorkerError {
    #[error("export renderer failed: {0}")]
    Render(String),
    #[error("export store failed: {0}")]
    Store(String),
    #[error("output operation failed: {0}")]
    Output(String),
}

pub trait ExportRenderer {
    fn render(&self, source: &Path, destination: &Path) -> Result<(), ExportWorkerError>;
}

#[derive(Debug, Clone)]
pub struct ExportJob {
    pub photo_id: String,
    pub source: PathBuf,
    pub output: PathBuf,
}

/// Crash-safe export coordinator.
///
/// The worker intentionally owns only export execution. Analysis, grouping,
/// AI inference and color planning remain independent checkpoints.
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

    pub fn run_job(&self, job: &ExportJob) -> Result<(), ExportWorkerError> {
        let temporary = job.output.with_extension("partial");

        self.renderer.render(&job.source, &temporary)?;

        std::fs::rename(&temporary, &job.output)
            .map_err(|e| ExportWorkerError::Output(e.to_string()))?;

        Ok(())
    }
}

pub fn recover_interrupted_export(status: ExportStatus) -> ExportStatus {
    match status {
        ExportStatus::Running => ExportStatus::Pending,
        other => other,
    }
}
