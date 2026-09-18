use photo_core::{
    AutomationRunner, BatchExportExecutor, BatchPipelineExecutor, BatchStage, ExportColorSpace,
    ExportFormat, ExportRecipe, ExportRenderer, ExportStageExecutor, ExportStatus, ExportStore,
    ExportWorkerError,
};
use std::fs;
use tempfile::tempdir;

struct TestRenderer;

impl ExportRenderer for TestRenderer {
    fn render(
        &self,
        _source: &std::path::Path,
        destination: &std::path::Path,
    ) -> Result<(), ExportWorkerError> {
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

#[test]
fn automation_runner_completes_export_stage() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("sample.cr3");
    let output = dir.path().join("exports");
    fs::create_dir_all(&output).unwrap();
    fs::write(&source, b"immutable raw").unwrap();

    let export_store = ExportStore::open(dir.path().join("export.sqlite3")).unwrap();
    let runner_store = photo_core::BatchStore::open(dir.path().join("batch.sqlite3")).unwrap();

    let batch_id = uuid::Uuid::new_v4();
    export_store.set_recipe(batch_id, &recipe(output)).unwrap();

    let export = BatchExportExecutor::new(TestRenderer, export_store.clone());
    let stage = ExportStageExecutor::new(batch_id, export);
    let pipeline = BatchPipelineExecutor::new(stage);

    let mut runner = AutomationRunner::new(runner_store, pipeline);
    let mut batch = runner
        .create_batch("export-test", vec![source.to_string_lossy().to_string()])
        .unwrap();

    while batch.items[0].stage != BatchStage::Export {
        runner.run_next(batch.id).unwrap();
        batch = runner.load_batch(batch.id).unwrap();
    }

    runner.run_next(batch.id).unwrap();
    batch = runner.load_batch(batch.id).unwrap();

    assert_eq!(batch.items[0].stage, BatchStage::Done);
    assert_eq!(batch.items[0].status, photo_core::JobStatus::Done);

    let checkpoint = export_store
        .load_checkpoint(batch_id, batch.items[0].id)
        .unwrap()
        .unwrap();
    assert_eq!(checkpoint.status, ExportStatus::Done);
    assert_eq!(fs::read(source).unwrap(), b"immutable raw");
}
