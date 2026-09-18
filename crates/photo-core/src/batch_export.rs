use std::path::PathBuf;

use crate::{ExportJob, ExportRenderer, ExportWorker, ExportWorkerError};

#[derive(Debug, Clone)]
pub struct BatchExportItem {
    pub item_id: String,
    pub source: PathBuf,
    pub output: PathBuf,
}

/// Adapter between the batch pipeline and the export executor.
///
/// Keeps export execution isolated from earlier AI/editing stages. A failed
/// export can be retried without invalidating analysis or retouch state.
pub struct BatchExportExecutor<R> {
    worker: ExportWorker<R>,
}

impl<R> BatchExportExecutor<R>
where
    R: ExportRenderer,
{
    pub fn new(renderer: R) -> Self {
        Self {
            worker: ExportWorker::new(renderer),
        }
    }

    pub fn execute(&self, item: BatchExportItem) -> Result<(), ExportWorkerError> {
        self.worker.run_job(&ExportJob {
            photo_id: item.item_id,
            source: item.source,
            output: item.output,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    struct MockRenderer;

    impl ExportRenderer for MockRenderer {
        fn render(&self, _source: &std::path::Path, destination: &std::path::Path) -> Result<(), ExportWorkerError> {
            fs::write(destination, b"jpeg").map_err(|e| ExportWorkerError::Output(e.to_string()))
        }
    }

    #[test]
    fn batch_export_adapter_runs_renderer() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("a.cr3");
        let output = dir.path().join("a.jpg");
        fs::write(&source, b"raw").unwrap();
        BatchExportExecutor::new(MockRenderer)
            .execute(BatchExportItem {
                item_id: "1".into(),
                source: source.clone(),
                output: output.clone(),
            })
            .unwrap();
        assert_eq!(fs::read(output).unwrap(), b"jpeg");
        assert_eq!(fs::read(source).unwrap(), b"raw");
    }
}
