import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import App, {
  type BackendBatch,
  type BackendPhotoContext,
  type BackendRawImportResult,
  type BatchWorkerEvent,
  type PhotoCakeBridge,
} from "@photo-cake/ui";
import "@photo-cake/ui/styles.css";

function sessionNameFromDirectory(directory: string) {
  return directory.split(/[\\/]/).filter(Boolean).at(-1) ?? "Photo Session";
}

const bridge: PhotoCakeBridge = {
  listBatches: () => invoke<BackendBatch[]>("list_batches"),
  runBatch: (batchId) => invoke<BackendBatch>("run_batch", { batchId }),
  retryFailed: (batchId) => invoke<BackendBatch>("retry_failed", { batchId }),
  pauseBatch: (batchId) => invoke<BackendBatch>("pause_batch", { batchId }),
  resumeBatch: (batchId) => invoke<BackendBatch>("resume_batch", { batchId }),
  cancelBatch: (batchId) => invoke<BackendBatch>("cancel_batch", { batchId }),
  loadPhotoContext: (batchId) =>
    invoke<BackendPhotoContext>("batch_photo_context", { batchId }),
  importRawDirectory: async () => {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "Select RAW photo folder",
    });
    if (!selected || Array.isArray(selected)) return null;

    return invoke<BackendRawImportResult>("import_raw_directory", {
      name: sessionNameFromDirectory(selected),
      directory: selected,
      recursive: true,
    });
  },
  subscribeBatchUpdates: async (handler) => {
    return listen<BatchWorkerEvent>("photo-cake://batch-updated", (event) => {
      handler(event.payload.batch);
    });
  },
};

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App bridge={bridge} mode="workstation" />
  </StrictMode>,
);
