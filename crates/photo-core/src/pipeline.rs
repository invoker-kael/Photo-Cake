use crate::{
    BatchItem, BatchStage, ExportStageExecutor, ExportRenderer, NoopStageExecutor, StageExecutor,
};

/// Dispatches batch stages while keeping the runner generic.
///
/// Export is routed to the real export executor; other stages continue using
/// the existing executor until their dedicated implementations are connected.
pub struct BatchPipelineExecutor<R> {
    pub export: ExportStageExecutor<R>,
}

impl<R> BatchPipelineExecutor<R> {
    pub fn new(export: ExportStageExecutor<R>) -> Self {
        Self { export }
    }
}

impl<R> StageExecutor for BatchPipelineExecutor<R>
where
    R: ExportRenderer,
{
    fn execute(&mut self, item: &BatchItem) -> Result<(), String> {
        match item.stage {
            BatchStage::Export => self
                .export
                .execute_item(item)
                .map_err(|error| error.to_string()),
            _ => {
                let mut noop = NoopStageExecutor;
                noop.execute(item)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BatchExportExecutor, ExportColorSpace, ExportFormat, ExportRecipe, ExportStageExecutor,
        ExportStatus, ExportStore,
    };
    use std::fs;
    use tempfile::tempdir;
    use uuid::Uuid;

    struct MockRenderer;

    impl ExportRenderer for MockRenderer {
        fn render(
            &self,
            _source: &std::path::Path,
            destination: &std::path::Path,
        ) -> Result<(), crate::ExportWorkerError> {
            fs::write(destination, b"jpeg")
                .map_err(|error| crate::ExportWorkerError::Output(error.to_string()))
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

    #[test]
    fn export_stage_dispatch_executes_real_export_path() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("sample.cr3");
        let output = dir.path().join("exports");
        fs::create_dir_all(&output).unwrap();
        fs::write(&source, b"raw immutable").unwrap();

        let store = ExportStore::open(dir.path().join("project.sqlite3")).unwrap();
        let batch_id = Uuid::new_v4();
        let item_id = Uuid::new_v4();
        store.set_recipe(batch_id, &recipe(output)).unwrap();

        let export = BatchExportExecutor::new(MockRenderer, store.clone());
        let stage = ExportStageExecutor::new(batch_id, export);
        let mut item = BatchItem::new(source.to_string_lossy().to_string());
        item.id = item_id;
        item.stage = BatchStage::Export;

        let mut pipeline = BatchPipelineExecutor::new(stage);
        pipeline.execute(&item).unwrap();

        let checkpoint = store.load_checkpoint(batch_id, item_id).unwrap().unwrap();
        assert_eq!(checkpoint.status, ExportStatus::Done);
        assert_eq!(fs::read(source).unwrap(), b"raw immutable");
    }

    #[test]
    fn non_export_stage_remains_noop_until_connected() {
        let mut noop = NoopStageExecutor;
        let item = BatchItem::new("sample.raw");
        assert!(noop.execute(&item).is_ok());
    }
}
