pub mod analysis;
pub mod batch;
pub mod catalog;
pub mod classification;
pub mod classification_store;
pub mod color_sync;
pub mod color_sync_store;
pub mod export;
pub mod grouping;
pub mod handoff;
pub mod handoff_store;
pub mod importer;
pub mod models;
pub mod preview;
pub mod raw;
pub mod runner;
pub mod semantic_grouping;
pub mod store;

pub use analysis::{
    AnalysisArtifact, AnalysisCache, AnalysisCacheError, AnalysisCacheKey, InferenceBackend,
    InferenceTask, LocalModelDescriptor, OnlineInferencePolicy,
};
pub use batch::{Batch, BatchItem, BatchStage, JobStatus};
pub use catalog::{CatalogError, RawCatalog};
pub use classification::{
    classify_photo, processing_route, reclassification_invalidation, should_run_portrait_retouch,
    ClassificationInvalidation, ClassificationInvalidationScope, ClassificationSignals,
    PhotoCategory, PhotoClassification, PortraitClassificationPolicy, ProcessingRoute, SceneTag,
};
pub use classification_store::{
    ClassificationRoutingExecutor, ClassificationStore, ClassificationStoreError,
};
pub use color_sync::{
    build_adaptive_group_plan, build_auto_group_plan, choose_reference_candidate,
    derive_auto_group_intent, promote_group_reference, ColorSyncError, GroupColorIntent,
    GroupColorInvalidation, GroupColorInvalidationScope, GroupColorSyncPlan, GroupSyncMode,
    PhotoColorAnalysis, ResolvedColorEdit, SemanticColorIntent, SemanticRegion,
};
pub use color_sync_store::{ColorSyncStore, ColorSyncStoreError};
pub use export::{
    collision_safe_output, plan_export, ExportColorSpace, ExportFormat, ExportPlan, ExportPlanError,
    ExportRecipe, ResizeRecipe,
};
pub use grouping::{
    initial_group_raw_assets, GroupingBasis, InitialGroupingConfig, PhotoGroup, PhotoGroupKind,
};
pub use handoff::{
    plan_finish, xmp_sidecar_path, EditCompatibility, FallbackReason, FinishPlan, FinishTarget,
    HandoffColorSpace, HandoffError, HandoffMode, RenderedHandoffPreset, TiffCompression,
};
pub use handoff_store::{HandoffStore, HandoffStoreError};
pub use importer::{RawImportError, RawImportResult, RawImporter};
pub use models::{BundledModelSpec, ModelBundleManifest, ModelPlatform, ModelVariant};
pub use preview::{preview_cache_path, PreviewArtifact, PreviewSource, PreviewStore, PreviewStoreError};
pub use raw::{
    extract_sequence_number, is_supported_raw, scan_raw_directory, scan_raw_paths, RawAsset,
    RawImportScan,
};
pub use runner::{AutomationRunner, NoopStageExecutor, RunStep, RunnerError, StageExecutor};
pub use semantic_grouping::{
    refine_group_by_similarity, ImageEmbedding, SemanticGroupKind, SemanticGroupingConfig,
    SemanticGroupingError, SemanticPhotoGroup,
};
pub use store::{BatchStore, StoreError};
