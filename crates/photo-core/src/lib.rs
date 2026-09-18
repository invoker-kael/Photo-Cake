pub mod analysis;
pub mod batch;
pub mod batch_export;
pub mod catalog;
pub mod classification;
pub mod classification_store;
pub mod color_sync;
pub mod color_sync_store;
pub mod culling;
pub mod export;
pub mod export_stage;
pub mod export_store;
pub mod export_worker;
pub mod grouping;
pub mod handoff;
pub mod handoff_store;
pub mod importer;
pub mod models;
pub mod pipeline;
pub mod preview;
pub mod raw;
pub mod recipe;
pub mod reference;
pub mod renderer;
pub mod runner;
pub mod semantic_grouping;
pub mod store;
pub mod xmp;

pub use analysis::{
    AnalysisArtifact, AnalysisCache, AnalysisCacheError, AnalysisCacheKey, InferenceBackend,
    InferenceTask, LocalModelDescriptor, OnlineInferencePolicy,
};
pub use batch::{Batch, BatchItem, BatchStage, JobStatus};
pub use batch_export::{BatchExportExecutor, BatchExportItem};
pub use catalog::{CatalogError, RawCatalog};
pub use classification::{
    classify_photo, processing_route, reclassification_invalidation, should_run_portrait_retouch,
    ClassificationInvalidation, ClassificationInvalidationScope, ClassificationSignals,
    PhotoCategory, PhotoClassification, PortraitClassificationPolicy, ProcessingRoute, SceneTag,
};
pub use classification_store::{ClassificationStore, ClassificationStoreError};
pub use color_sync::{
    build_adaptive_group_plan, build_auto_group_plan, choose_reference_candidate,
    derive_auto_group_intent, promote_group_reference, ColorSyncError, GroupColorIntent,
    GroupColorInvalidation, GroupColorInvalidationScope, GroupColorSyncPlan, GroupSyncMode,
    PhotoColorAnalysis, ResolvedColorEdit, SemanticColorIntent, SemanticRegion,
};
pub use culling::{suggest_decision, CullingDecision, CullingScore};
pub use export::{
    collision_safe_output, plan_export, ExportColorSpace, ExportFormat, ExportPlan,
    ExportPlanError, ExportRecipe, ResizeRecipe,
};
pub use export_stage::{ExportStageError, ExportStageExecutor};
pub use export_store::{ExportCheckpoint, ExportStatus, ExportStore, ExportStoreError};
pub use export_worker::{
    recover_interrupted_export, ExportJob, ExportRenderer, ExportWorker, ExportWorkerError,
};
pub use grouping::{
    initial_group_raw_assets, GroupingBasis, InitialGroupingConfig, PhotoGroup, PhotoGroupKind,
};
pub use models::{
    BundledModelSpec, ModelBundleManifest, ModelPlatform, ModelVariant,
};
pub use pipeline::BatchPipelineExecutor;
pub use preview::{
    preview_cache_path, PreviewArtifact, PreviewSource, PreviewStore, PreviewStoreError,
};
pub use raw::{
    extract_sequence_number, is_supported_raw, scan_raw_directory, scan_raw_paths, RawAsset,
    RawImportScan,
};
pub use recipe::{EditAdjustments, Recipe};
pub use reference::{ReferenceSet, StyleProfile};
pub use renderer::ImageExportRenderer;
pub use runner::{AutomationRunner, NoopStageExecutor, RunStep, RunnerError, StageExecutor};
pub use semantic_grouping::{
    refine_group_by_similarity, ImageEmbedding, SemanticGroupKind, SemanticGroupingConfig,
    SemanticGroupingError, SemanticPhotoGroup,
};
pub use xmp::{sidecar_path_for_raw, write_recipe_sidecar, XmpEditState};
