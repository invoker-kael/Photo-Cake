import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import App, {
  type BackendBatch,
  type BatchWorkerEvent,
  type PhotoCakeBridge,
} from "@photo-cake/ui";
import "@photo-cake/ui/styles.css";

const bridge: PhotoCakeBridge = {
  listBatches: () => invoke<BackendBatch[]>("list_batches"),
  runBatch: (batchId) => invoke<BackendBatch>("run_batch", { batchId }),
  retryFailed: (batchId) => invoke<BackendBatch>("retry_failed", { batchId }),
  pauseBatch: (batchId) => invoke<BackendBatch>("pause_batch", { batchId }),
  resumeBatch: (batchId) => invoke<BackendBatch>("resume_batch", { batchId }),
  cancelBatch: (batchId) => invoke<BackendBatch>("cancel_batch", { batchId }),
  subscribeBatchUpdates: async (handler) => {
    return listen<BatchWorkerEvent>("photo-cake://batch-updated", (event) => {
      handler(event.payload.batch);
    });
  },
};

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App bridge={bridge} />
  </StrictMode>,
);
