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

export interface BackendPhotoContext {
  assets: BackendRawAsset[];
  groups: BackendPhotoGroup[];
}

export interface BackendRawImportResult extends BackendPhotoContext {
  batch: BackendBatch | null;
  skipped_non_raw: string[];
}

export type CullingDecision = "KEEP" | "REVIEW" | "REJECT_SUGGESTION";
export type CullingUserDecision = "KEEP" | "REVIEW" | "REJECT";

export interface BackendCullingRecommendation {
  asset_id: string;
  quality_score: number;
  decision: CullingDecision;
  group_rank: number;
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

export interface BatchWorkerEvent {
  batch: BackendBatch;
  step: unknown | null;
}

export interface PhotoCakeBridge {
  listBatches(): Promise<BackendBatch[]>;
  runBatch(batchId: string): Promise<BackendBatch>;
  retryFailed(batchId: string): Promise<BackendBatch>;
  pauseBatch(batchId: string): Promise<BackendBatch>;
  resumeBatch(batchId: string): Promise<BackendBatch>;
  cancelBatch(batchId: string): Promise<BackendBatch>;
  importRawDirectory?(): Promise<BackendRawImportResult | null>;
  loadPhotoContext?(batchId: string): Promise<BackendPhotoContext>;
  loadCulling?(batchId: string): Promise<BackendGroupCullingResult[]>;
  loadCullingReviews?(batchId: string): Promise<BackendCullingReview[]>;
  setCullingReview?(assetId: string, decision: CullingUserDecision | null): Promise<void>;
  loadReferenceBindings?(batchId: string): Promise<BackendReferenceBinding[]>;
  setGroupReference?(groupId: string, assetId: string): Promise<BackendReferenceBinding>;
  clearGroupReference?(groupId: string): Promise<void>;
  subscribeBatchUpdates(handler: (batch: BackendBatch) => void): Promise<() => void>;
}

export const demoJobs: BatchJob[] = [
  { id: "1", filename: "DSC_1042.ARW", stage: "ANALYZE", status: "RUNNING", progress: 54, qa: null },
  { id: "2", filename: "DSC_1043.ARW", stage: "DONE", status: "DONE", progress: 100, qa: null },
  { id: "3", filename: "DSC_1044.ARW", stage: "DONE", status: "DONE", progress: 100, qa: null },
  { id: "4", filename: "DSC_1045.ARW", stage: "ANALYZE", status: "FAILED", progress: 24, qa: null, error: "Analysis worker unavailable" },
  { id: "5", filename: "DSC_1046.ARW", stage: "ANALYZE", status: "PENDING", progress: 20, qa: null },
];
