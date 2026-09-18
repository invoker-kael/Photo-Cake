import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import App, {
  type BackendBatch,
  type BackendCullingReview,
  type BackendGroupCullingResult,
  type BackendGroupReferencePreview,
  type BackendGroupReferenceStyle,
  type BackendLightroomHandoffResult,
  type BackendPhotoContext,
  type BackendRawImportResult,
  type BackendReferenceBinding,
  type BackendRecipeReviewOverride,
  type BackendReviewRenderResult,
  type BackendSemanticRefinementReport,
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
  refineGroups: (batchId) =>
    invoke<BackendSemanticRefinementReport>("refine_batch_groups", { batchId }),
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
  setCullingReviews: (reviews) =>
    invoke<void>("set_culling_reviews", { reviews }),
  loadReferenceBindings: (batchId) =>
    invoke<BackendReferenceBinding[]>("batch_reference_bindings", { batchId }),
  setGroupReference: (groupId, assetId) =>
    invoke<BackendReferenceBinding>("set_group_reference", { groupId, assetId }),
  clearGroupReference: (groupId) =>
    invoke<void>("clear_group_reference", { groupId }),
  loadRecipeReviews: (batchId) =>
    invoke<BackendRecipeReviewOverride[]>("batch_recipe_reviews", { batchId }),
  setRecipeReview: (
    assetId,
    exposureDeltaEv,
    contrastDelta,
    saturationDelta,
  ) =>
    invoke<BackendRecipeReviewOverride>("set_recipe_review", {
      assetId,
      exposureDeltaEv,
      contrastDelta,
      saturationDelta,
    }),
  clearRecipeReview: (assetId) =>
    invoke<void>("clear_recipe_review", { assetId }),
  renderRecipePreview: async (groupId, assetId) => {
    const result = await invoke<BackendReviewRenderResult>("render_group_recipe_preview", {
      groupId,
      assetId,
    });
    return {
      ...result,
      preview_url: `${convertFileSrc(result.cache_path)}?recipe=${result.recipe_id}`,
    };
  },
  loadReferenceStyles: (batchId) =>
    invoke<BackendGroupReferenceStyle[]>("batch_reference_styles", { batchId }),
  updateReferenceStyle: (
    groupId,
    exposureBiasEv,
    contrastPreference,
    saturationPreference,
  ) =>
    invoke<BackendGroupReferenceStyle>("update_group_reference_style", {
      groupId,
      exposureBiasEv,
      contrastPreference,
      saturationPreference,
    }),
  loadReferencePreviews: (batchId) =>
    invoke<BackendGroupReferencePreview[]>("batch_reference_previews", { batchId }),
  writeGroupXmp: (groupId) =>
    invoke<BackendLightroomHandoffResult>("write_group_reference_xmp", { groupId }),
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
