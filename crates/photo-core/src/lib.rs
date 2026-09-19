pub mod analysis;
pub mod batch;
pub mod batch_export;
pub mod bracketing;
pub mod bracketing_store;
pub mod catalog;
pub mod classification;
pub mod classification_store;
pub mod color_sync;
pub mod color_sync_store;
pub mod companion;
pub mod companion_store;
pub mod culling;
pub mod culling_store;
pub mod export;
pub mod edit_preview;
pub mod export_stage;
pub mod export_store;
pub mod export_worker;
pub mod grouping;
pub mod group_refinement;
pub mod handoff;
pub mod handoff_store;
pub mod importer;
pub mod metadata;
pub mod metadata_store;
pub mod models;
pub mod pipeline;
pub mod preview;
pub mod raw;
pub mod recipe;
pub mod recipe_review_store;
pub mod reference;
pub mod reference_store;
pub mod renderer;
pub mod runner;
pub mod semantic_grouping;
pub mod store;
pub mod xmp;
pub mod workflow;

pub use analysis::{
    AnalysisArtifact, AnalysisCache, AnalysisCacheError, AnalysisCacheKey, InferenceBackend,
    InferenceTask, LocalModelDescriptor, OnlineInferencePolicy,
};
pub use batch::{Batch, BatchItem, BatchStage, JobStatus};
pub use batch_export::{BatchExportExecutor, BatchExportItem};
pub use bracketing::{
    detect_exposure_brackets, route_exposure_bracket_sources, ExposureBracketError,
    ExposureBracketMember, ExposureBracketRole, ExposureBracketRouting, ExposureBracketSet,
};
pub use bracketing_store::{ExposureBracketMergeStore, ExposureBracketMergeStoreError};
pub use catalog::{CatalogError, RawCatalog};
pub use classification::{
    classify_photo, processing_route, reclassification_invalidation, should_run_portrait_retouch,
    ClassificationInvalidation, ClassificationInvalidationScope, ClassificationSignals,
    PhotoCategory, PhotoClassification, PortraitClassificationPolicy, ProcessingRoute, SceneTag,
};
pub use classification_store::{ClassificationRoutingExecutor, ClassificationStore, ClassificationStoreError};
pub use color_sync::{
    build_adaptive_group_plan, build_auto_group_plan, choose_reference_candidate,
    derive_auto_group_intent, promote_group_reference, ColorSyncError, GroupColorIntent,
    GroupColorInvalidation, GroupColorInvalidationScope, GroupColorSyncPlan, GroupSyncMode,
    PhotoColorAnalysis, PhotoExposureAnalysis, ResolvedColorEdit, SemanticColorIntent,
    SemanticRegion, reference_relative_tone_adjustments,
};
pub use culling::{
    build_group_culling_result, build_moment_quick_cull_plan, rank_group_candidates,
    rank_group_candidates_with_embeddings, suggest_decision, CullingCandidate, CullingDecision,
    CullingEvidenceError, CullingReason, CullingRecommendation, CullingScore, GroupCullingResult,
    MomentQuickCullPlan,
};
pub use culling_store::{
    CullingReview, CullingReviewStore, CullingReviewStoreError, CullingUserDecision,
    MomentQuickCullOperation,
};
pub use companion::{
    apply_companion_patch, build_companion_decision_patch, build_companion_snapshot,
    hydrate_companion_snapshot, CompanionAsset, CompanionCullingChange, CompanionDecisionPatch,
    CompanionError, CompanionPatchApplyReport, CompanionPreviewIndex, CompanionReferenceChange,
    CompanionReferenceState, CompanionSnapshot, COMPANION_SNAPSHOT_SCHEMA_VERSION,
};
pub use companion_store::{CompanionSnapshotStore, CompanionSnapshotStoreError};
pub use edit_preview::{apply_preview_adjustments, render_recipe_preview, EditPreviewError};
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
pub use group_refinement::{
    refine_collection_semantic_groups, SemanticRefinementError, SemanticRefinementReport,
};
pub use handoff::{
    plan_finish, xmp_sidecar_path, EditCompatibility, FallbackReason, FinishPlan, FinishTarget,
    HandoffColorSpace, HandoffError, HandoffMode, RenderedHandoffPreset, TiffCompression,
};
pub use handoff_store::{HandoffStore, HandoffStoreError};
pub use importer::{RawImportError, RawImportResult, RawImporter};
pub use metadata::{
    read_raw_metadata, RawMetadataEvidence, RawRational, RawWhiteBalanceEvidence,
};
pub use metadata_store::{
    AssetMetadataEvidence, RawMetadataStore, RawMetadataStoreError,
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
pub use recipe_review_store::{
    recipe_review_fingerprint, RecipeReviewConfirmation, RecipeReviewOverride,
    RecipeReviewStore, RecipeReviewStoreError, RecipeReviewSyncFields,
};
pub use reference::{
    build_reference_readiness_plan, build_style_sync_group_context, build_style_sync_preflight,
    style_sync_compatibility, ReferenceCandidateSource, ReferenceGroupResult,
    ReferenceReadinessPlan, ReferenceReadinessStatus, ReferenceSet, ReferenceWorkflowError,
    StyleProfile, StyleSyncCompatibility, StyleSyncGroupContext, StyleSyncPreflight,
    StyleSyncReason, StyleSyncTargetPlan,
};
pub use reference_store::{GroupReferenceBinding, ReferenceStore, ReferenceStoreError};
pub use renderer::ImageExportRenderer;
pub use runner::{AutomationRunner, NoopStageExecutor, RunStep, RunnerError, StageExecutor};
pub use semantic_grouping::{
    embedding_similarity, refine_group_by_similarity, ImageEmbedding, SemanticGroupKind, SemanticGroupingConfig,
    SemanticGroupingError, SemanticPhotoGroup,
};
pub use store::{BatchStore, StoreError};
pub use xmp::{
    preflight_group_sidecars, sidecar_path_for_raw, validate_recipe_xmp, verify_group_sidecars,
    write_group_sidecars, write_recipe_sidecar, write_sidecar_batch, XmpEditState,
    XmpHandoffVerification, XmpParseError, XmpSidecarPreflight, XmpWriteError,
};

pub use workflow::{
    build_recipe_review_group_preflight, derive_workflow_status, recipe_review_attention_reason,
    recipe_review_group_can_confirm, recipe_review_requires_attention,
    RecipeReviewAssetPreflight, RecipeReviewAttentionReason, RecipeReviewDisposition,
    RecipeReviewGroupPreflight, RecipeReviewSignal, WorkflowFacts, WorkflowFocus, WorkflowStatus,
};
