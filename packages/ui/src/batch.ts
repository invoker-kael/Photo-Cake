export type BatchStage =
  | "IMPORT"
  | "ANALYZE"
  | "APPLY_PRESET"
  | "PORTRAIT_RETOUCH"
  | "QA"
  | "EXPORT"
  | "DONE";

export type JobStatus =
  | "PENDING"
  | "RUNNING"
  | "PAUSED"
  | "FAILED"
  | "CANCELLED"
  | "DONE";

export type QaResult = "PASS" | "REVIEW" | "FAIL" | null;

export interface BatchJob {
  id: string;
  filename: string;
  stage: BatchStage;
  status: JobStatus;
  progress: number;
  qa: QaResult;
  error?: string;
}

export interface BackendBatchItem {
  id: string;
  asset_id: string | null;
  source_path: string;
  stage: BatchStage;
  status: JobStatus;
  attempts: number;
  last_error: string | null;
}

export interface BackendBatch {
  id: string;
  name: string;
  items: BackendBatchItem[];
  auto_qa: boolean;
  stop_on_error: boolean;
}

export interface BackendRawAsset {
  id: string;
  source_path: string;
  filename: string;
  extension: string;
  camera_id: string | null;
  capture_time_ms: number | null;
  file_time_ms: number | null;
  sequence_number: number | null;
}

export interface BackendPhotoGroup {
  id: string;
  kind: "MOMENT" | "SIMILAR" | "MANUAL";
  basis:
    | "TIME"
    | "TIME_AND_SEQUENCE"
    | "SEQUENCE_FALLBACK"
    | "SEMANTIC_SIMILARITY"
    | "SINGLETON";
  asset_ids: string[];
  manual_locked: boolean;
}

export interface BackendPreviewArtifact {
  asset_id: string;
  source_fingerprint: string;
  revision: string;
  cache_path: string;
  mime_type: string;
  width: number;
  height: number;
  source: "EMBEDDED_RAW_PREVIEW" | "RENDERED_RAW_PREVIEW";
  preview_url?: string;
}

export interface BackendRawRational {
  num: number;
  denom: number;
}

export interface BackendRawWhiteBalanceEvidence {
  as_shot_neutral: [BackendRawRational, BackendRawRational, BackendRawRational] | null;
  as_shot_white_xy: [BackendRawRational, BackendRawRational] | null;
}

export interface BackendRawMetadataEvidence {
  camera_id: string | null;
  capture_time_ms: number | null;
  white_balance: BackendRawWhiteBalanceEvidence | null;
}

export interface BackendAssetMetadataEvidence {
  asset_id: string;
  evidence: BackendRawMetadataEvidence;
}

export interface BackendPhotoContext {
  assets: BackendRawAsset[];
  groups: BackendPhotoGroup[];
  previews?: BackendPreviewArtifact[];
  metadata?: BackendAssetMetadataEvidence[];
}

export interface BackendSemanticRefinementReport {
  collection_id: string;
  refined_parent_group_ids: string[];
  pending_asset_ids: string[];
  effective_groups: BackendPhotoGroup[];
}

export interface BackendRawImportResult extends BackendPhotoContext {
  batch: BackendBatch | null;
  skipped_non_raw: string[];
}

export type CullingDecision = "KEEP" | "REVIEW" | "REJECT_SUGGESTION";
export type CullingUserDecision = "KEEP" | "REVIEW" | "REJECT";

export interface BackendCullingPortraitEvidence {
  person_count: number;
  face_count: number;
  primary_subject_ratio: number;
  people_confidence: number;
}

export interface BackendCullingRecommendation {
  asset_id: string;
  quality_score: number;
  decision: CullingDecision;
  group_rank: number;
  portrait_evidence?: BackendCullingPortraitEvidence | null;
}

export interface BackendGroupCullingResult {
  group_id: string;
  recommendations: BackendCullingRecommendation[];
  pending_asset_ids: string[];
}

export interface BackendCullingReview {
  asset_id: string;
  decision: CullingUserDecision;
}

export interface BackendReferenceBinding {
  group_id: string;
  reference_set_id: string;
  selected_reference_asset_id: string;
}

export interface BackendStyleProfile {
  exposure_bias_ev: number | null;
  temperature_bias: number | null;
  tint_bias: number | null;
  contrast_preference: number | null;
  saturation_preference: number | null;
  notes: string | null;
}

export interface BackendGroupReferenceStyle {
  group_id: string;
  reference_set_id: string;
  style_profile: BackendStyleProfile;
}

export interface BackendEditAdjustments {
  exposure: number | null;
  contrast: number | null;
  highlights: number | null;
  shadows: number | null;
  temperature: number | null;
  tint: number | null;
  saturation: number | null;
}

export interface BackendRecipe {
  id: string;
  name: string;
  target_asset_id: string | null;
  source_reference_ids: string[];
  adjustments: BackendEditAdjustments;
}

export interface BackendGroupReferencePreview {
  group_id: string;
  selected_reference_asset_id: string;
  recipes: BackendRecipe[];
  pending_asset_id: string | null;
}

export interface BackendLightroomHandoffResult {
  group_id: string;
  written_sidecars: string[];
}

export interface BackendRecipeReviewOverride {
  asset_id: string;
  exposure_delta_ev: number;
  contrast_delta: number;
  saturation_delta: number;
}

export interface BackendReviewRenderResult {
  asset_id: string;
  recipe_id: string;
  cache_path: string;
  preview_url?: string;
}

export interface BatchWorkerEvent {
  batch: BackendBatch;
  step: unknown | null;
}

export interface PhotoCakeBridge {
  listBatches(): Promise<BackendBatch[]>;
  runBatch?(batchId: string): Promise<BackendBatch>;
  retryFailed?(batchId: string): Promise<BackendBatch>;
  pauseBatch?(batchId: string): Promise<BackendBatch>;
  resumeBatch?(batchId: string): Promise<BackendBatch>;
  cancelBatch?(batchId: string): Promise<BackendBatch>;
  importRawDirectory?(): Promise<BackendRawImportResult | null>;
  loadPhotoContext?(batchId: string): Promise<BackendPhotoContext>;
  refineGroups?(batchId: string): Promise<BackendSemanticRefinementReport>;
  loadCulling?(batchId: string): Promise<BackendGroupCullingResult[]>;
  loadCullingReviews?(batchId: string): Promise<BackendCullingReview[]>;
  setCullingReview?(assetId: string, decision: CullingUserDecision | null): Promise<void>;
  loadReferenceBindings?(batchId: string): Promise<BackendReferenceBinding[]>;
  setGroupReference?(groupId: string, assetId: string): Promise<BackendReferenceBinding>;
  clearGroupReference?(groupId: string): Promise<void>;
  loadRecipeReviews?(batchId: string): Promise<BackendRecipeReviewOverride[]>;
  setRecipeReview?(
    assetId: string,
    exposureDeltaEv: number,
    contrastDelta: number,
    saturationDelta: number,
  ): Promise<BackendRecipeReviewOverride>;
  clearRecipeReview?(assetId: string): Promise<void>;
  renderRecipePreview?(groupId: string, assetId: string): Promise<BackendReviewRenderResult>;
  loadReferenceStyles?(batchId: string): Promise<BackendGroupReferenceStyle[]>;
  updateReferenceStyle?(
    groupId: string,
    exposureBiasEv: number,
    contrastPreference: number,
    saturationPreference: number,
  ): Promise<BackendGroupReferenceStyle>;
  loadReferencePreviews?(batchId: string): Promise<BackendGroupReferencePreview[]>;
  writeGroupXmp?(groupId: string): Promise<BackendLightroomHandoffResult>;
  subscribeBatchUpdates?(handler: (batch: BackendBatch) => void): Promise<() => void>;
}

export const demoJobs: BatchJob[] = [
  { id: "1", filename: "DSC_1042.ARW", stage: "ANALYZE", status: "RUNNING", progress: 54, qa: null },
  { id: "2", filename: "DSC_1043.ARW", stage: "DONE", status: "DONE", progress: 100, qa: null },
  { id: "3", filename: "DSC_1044.ARW", stage: "DONE", status: "DONE", progress: 100, qa: null },
  { id: "4", filename: "DSC_1045.ARW", stage: "ANALYZE", status: "FAILED", progress: 24, qa: null, error: "Analysis worker unavailable" },
  { id: "5", filename: "DSC_1046.ARW", stage: "ANALYZE", status: "PENDING", progress: 20, qa: null },
];
