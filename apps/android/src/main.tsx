import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import App, {
  type BackendBatch,
  type BackendCullingReview,
  type BackendGroupCullingResult,
  type BackendPhotoContext,
  type BackendReferenceBinding,
  type PhotoCakeBridge,
} from "@photo-cake/ui";
import "@photo-cake/ui/styles.css";

const bridge: PhotoCakeBridge = {
  listBatches: () => invoke<BackendBatch[]>("list_batches"),
  loadPhotoContext: async (batchId) => {
    const context = await invoke<BackendPhotoContext>("batch_photo_context", { batchId });
    return {
      ...context,
      previews: context.previews?.map((preview) => ({
        ...preview,
        preview_url: convertFileSrc(preview.cache_path),
      })),
    };
  },
  loadCulling: (batchId) =>
    invoke<BackendGroupCullingResult[]>("batch_culling", { batchId }),
  loadCullingReviews: (batchId) =>
    invoke<BackendCullingReview[]>("batch_culling_reviews", { batchId }),
  setCullingReview: (assetId, decision) =>
    invoke<void>("set_culling_review", { assetId, decision }),
  loadReferenceBindings: (batchId) =>
    invoke<BackendReferenceBinding[]>("batch_reference_bindings", { batchId }),
  setGroupReference: (groupId, assetId) =>
    invoke<BackendReferenceBinding>("set_group_reference", { groupId, assetId }),
  clearGroupReference: (groupId) =>
    invoke<void>("clear_group_reference", { groupId }),
};

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App bridge={bridge} mode="companion" />
  </StrictMode>,
);
