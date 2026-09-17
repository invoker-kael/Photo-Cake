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
  subscribeBatchUpdates(handler: (batch: BackendBatch) => void): Promise<() => void>;
}

export interface AutomationRecipe {
  name: string;
  autoAnalyze: boolean;
  autoPreset: boolean;
  autoRetouch: boolean;
  autoQa: boolean;
  autoExportPass: boolean;
  retries: number;
  continueOnError: boolean;
}

export const defaultRecipe: AutomationRecipe = {
  name: "Natural Batch",
  autoAnalyze: true,
  autoPreset: true,
  autoRetouch: true,
  autoQa: true,
  autoExportPass: true,
  retries: 2,
  continueOnError: true,
};

export const demoJobs: BatchJob[] = [
  { id: "1", filename: "DSC_1042.ARW", stage: "PORTRAIT_RETOUCH", status: "RUNNING", progress: 68, qa: null },
  { id: "2", filename: "DSC_1043.ARW", stage: "QA", status: "RUNNING", progress: 82, qa: null },
  { id: "3", filename: "DSC_1044.ARW", stage: "DONE", status: "DONE", progress: 100, qa: "PASS" },
  { id: "4", filename: "DSC_1045.ARW", stage: "ANALYZE", status: "FAILED", progress: 24, qa: null, error: "Analysis worker unavailable" },
  { id: "5", filename: "DSC_1046.ARW", stage: "IMPORT", status: "PENDING", progress: 0, qa: null },
];
