use std::path::{Path, PathBuf};

use thiserror::Error;
use uuid::Uuid;

use crate::export_store::{partial_output_path, remove_file_if_exists};
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
            .map_err(|error| ExportWorkerError::Store(error.to_string()))?;

        if checkpoint.status == ExportStatus::Done {
            return Ok(checkpoint);
        }

        let running = store
            .mark_running(checkpoint)
            .map_err(|error| ExportWorkerError::Store(error.to_string()))?;
        let temporary = partial_output_path(&running.output_path);

        if let Some(parent) = temporary
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            if let Err(error) = std::fs::create_dir_all(parent) {
                return Err(record_failure(
                    store,
                    running,
                    ExportWorkerError::Output(error.to_string()),
                    &temporary,
                ));
            }
        }

        if let Err(error) = remove_file_if_exists(&temporary) {
            return Err(record_failure(
                store,
                running,
                ExportWorkerError::Output(error.to_string()),
                &temporary,
            ));
        }

        if let Err(error) = self.renderer.render(&job.source, &temporary) {
            return Err(record_failure(store, running, error, &temporary));
        }

        let temporary_metadata = match std::fs::metadata(&temporary) {
            Ok(metadata) => metadata,
            Err(error) => {
                return Err(record_failure(
                    store,
                    running,
                    ExportWorkerError::Verify(error.to_string()),
                    &temporary,
                ))
            }
        };
        if temporary_metadata.len() == 0 {
            return Err(record_failure(
                store,
                running,
                ExportWorkerError::Verify("temporary output is empty".to_string()),
                &temporary,
            ));
        }

        if let Err(error) = std::fs::rename(&temporary, &running.output_path) {
            return Err(record_failure(
                store,
                running,
                ExportWorkerError::Output(error.to_string()),
                &temporary,
            ));
        }

        let output_metadata = match std::fs::metadata(&running.output_path) {
            Ok(metadata) => metadata,
            Err(error) => {
                let _ = remove_file_if_exists(&running.output_path);
                return Err(record_failure(
                    store,
                    running,
                    ExportWorkerError::Verify(error.to_string()),
                    &temporary,
                ));
            }
        };
        if output_metadata.len() == 0 {
            let _ = remove_file_if_exists(&running.output_path);
            return Err(record_failure(
                store,
                running,
                ExportWorkerError::Verify("final output is empty".to_string()),
                &temporary,
            ));
        }

        store
            .mark_done(running)
            .map_err(|error| ExportWorkerError::Store(error.to_string()))
    }
}

fn record_failure(
    store: &ExportStore,
    running: ExportCheckpoint,
    error: ExportWorkerError,
    temporary: &Path,
) -> ExportWorkerError {
    let _ = remove_file_if_exists(temporary);
    match store.mark_failed(running, error.to_string()) {
        Ok(_) => error,
        Err(store_error) => ExportWorkerError::Store(store_error.to_string()),
    }
}

pub fn recover_interrupted_export(status: ExportStatus) -> ExportStatus {
    match status {
        ExportStatus::Running => ExportStatus::Pending,
        other => other,
    }
}
