import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import App, {
  type BackendBatch,
  type BackendCullingReview,
  type BackendCullSelection,
  type BackendCullConfirmVisibleResult,
  type BackendGroupCullingResult,
  type BackendGroupReferencePreview,
  type BackendGroupReferenceStyle,
  type BackendGroupReferenceStyleBatchResult,
  type BackendLightroomBatchHandoffResult,
  type BackendLightroomBatchPreflightResult,
  type BackendLightroomHandoffPreflight,
  type BackendLightroomHandoffResult,
  type BackendMomentQuickCullBatchResult,
  type BackendMomentQuickCullOperation,
  type BackendPhotoContext,
  type BackendRawImportResult,
  type BackendReferenceBatchItem,
  type BackendReferenceBatchResult,
  type BackendReferenceBinding,
  type BackendReferenceReadinessPlan,
  type BackendRecipeReviewBatchItem,
  type BackendRecipeReviewBatchResult,
  type BackendRecipeReviewGroupPreflight,
  type BackendRecipeReviewGroupBatchResult,
  type BackendRecipeReviewOverride,
  type BackendRecipeReviewSyncFields,
  type BackendRecipeReviewSyncResult,
  type BackendReviewRenderResult,
  type BackendSemanticRefinementReport,
  type BackendStyleSyncPreflight,
  type BackendWorkflowStatus,
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
  keepMomentTogether: (batchId, groupId) =>
    invoke<BackendPhotoContext["groups"]>("keep_batch_moment_together", {
      batchId,
      groupId,
    }),
  allowGroupRefinement: (batchId, groupId) =>
    invoke<BackendPhotoContext["groups"]>("allow_batch_group_refinement", {
      batchId,
      groupId,
    }),
  mergeGroups: (batchId, groupIds) =>
    invoke<BackendPhotoContext["groups"]>("merge_batch_groups", {
      batchId,
      groupIds,
    }),
  splitGroup: (batchId, groupId, splitBeforeAssetId) =>
    invoke<BackendPhotoContext["groups"]>("split_batch_group", {
      batchId,
      groupId,
      splitBeforeAssetId,
    }),
  loadWorkflowStatus: (batchId) =>
    invoke<BackendWorkflowStatus>("batch_workflow_status", { batchId }),
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
  loadLatestMomentQuickCull: (batchId) =>
    invoke<BackendMomentQuickCullOperation | null>("batch_latest_moment_quick_cull", {
      batchId,
    }),
  setCullingReview: (batchId, groupId, assetId, decision) =>
    invoke("photography_command", {
      commandId: "cull.decide",
      request: { batch_id: batchId, group_id: groupId, asset_id: assetId, decision },
    }).then(() => undefined),
  confirmVisibleCulling: (batchId, selections: BackendCullSelection[]) =>
    invoke<BackendCullConfirmVisibleResult>("photography_command", {
      commandId: "cull.confirmVisible",
      request: { batch_id: batchId, selections },
    }),
  confirmMomentQuickCull: (batchId, groupIds) =>
    invoke<BackendMomentQuickCullBatchResult>("photography_command", {
      commandId: "cull.quickApply",
      request: { batch_id: batchId, group_ids: groupIds },
    }),
  undoMomentQuickCull: (batchId, operationId) =>
    invoke<BackendMomentQuickCullOperation>("photography_command", {
      commandId: "cull.quickUndo",
      request: { batch_id: batchId, operation_id: operationId },
    }),
  loadReferenceBindings: (batchId) =>
    invoke<BackendReferenceBinding[]>("batch_reference_bindings", { batchId }),
  loadReferenceReadiness: (batchId) =>
    invoke<BackendReferenceReadinessPlan[]>("batch_reference_readiness", { batchId }),
  setGroupReference: (batchId, groupId, assetId) =>
    invoke<BackendReferenceBinding>("photography_command", {
      commandId: "reference.select", request: { batch_id: batchId, group_id: groupId, asset_id: assetId },
    }),
  setGroupReferences: (batchId, items: BackendReferenceBatchItem[]) =>
    invoke<BackendReferenceBatchResult>("photography_command", {
      commandId: "reference.selectBatch",
      request: { batch_id: batchId, selections: items },
    }),
  clearGroupReference: (groupId) =>
    invoke<void>("clear_group_reference", { groupId }),
  loadRecipeReviews: (batchId) =>
    invoke<BackendRecipeReviewOverride[]>("batch_recipe_reviews", { batchId }),
  loadRecipeReviewPreflight: (batchId) =>
    invoke<BackendRecipeReviewGroupPreflight[]>("batch_recipe_review_preflight", { batchId }),
  setRecipeReview: (
    batchId, groupId, assetId, exposureDeltaEv, highlightsDelta,
    shadowsDelta, whitesDelta, blacksDelta, contrastDelta,
    saturationDelta, vibranceDelta,
  ) =>
    invoke<BackendRecipeReviewOverride>("photography_command", {
      commandId: "recipe.exceptionSet",
      request: {
        batch_id: batchId, group_id: groupId, asset_id: assetId,
        deltas: {
          exposure_delta_ev: exposureDeltaEv, highlights_delta: highlightsDelta,
          shadows_delta: shadowsDelta, whites_delta: whitesDelta,
          blacks_delta: blacksDelta, contrast_delta: contrastDelta,
          saturation_delta: saturationDelta, vibrance_delta: vibranceDelta,
        },
      },
    }),
  syncRecipeReviewException: (
    batchId, groupId, sourceAssetId, targetAssetIds,
    fields: BackendRecipeReviewSyncFields,
  ) =>
    invoke<BackendRecipeReviewSyncResult>("photography_command", {
      commandId: "recipe.exceptionSync",
      request: {
        batch_id: batchId, group_id: groupId,
        source_asset_id: sourceAssetId,
        target_asset_ids: targetAssetIds, fields,
      },
    }),
  clearRecipeReview: (batchId, groupId, assetId) =>
    invoke("photography_command", {
      commandId: "recipe.exceptionClear",
      request: { batch_id: batchId, group_id: groupId, asset_id: assetId },
    }).then(() => undefined),
  setRecipeReviewed: (batchId, groupId, assetId) =>
    invoke("photography_command", {
      commandId: "recipe.confirm", request: { batch_id: batchId, group_id: groupId, asset_id: assetId },
    }).then(() => undefined),
  confirmRecipeReviews: (batchId, items: BackendRecipeReviewBatchItem[]) =>
    invoke<BackendRecipeReviewBatchResult>("photography_command", {
      commandId: "recipe.confirmBatch",
      request: { batch_id: batchId, selections: items },
    }),
  confirmRecipeReviewGroups: (batchId, groupIds) =>
    invoke<BackendRecipeReviewGroupBatchResult>("photography_command", {
      commandId: "recipe.confirmClearGroups",
      request: { batch_id: batchId, group_ids: groupIds },
    }),
  clearRecipeReviewed: (batchId, groupId, assetId) =>
    invoke("photography_command", {
      commandId: "recipe.reopen", request: { batch_id: batchId, group_id: groupId, asset_id: assetId },
    }).then(() => undefined),
  renderRecipePreview: async (batchId, groupId, assetId) => {
    const result = await invoke<BackendReviewRenderResult>("photography_command", {
      commandId: "recipe.preview", request: { batch_id: batchId, group_id: groupId, asset_id: assetId },
    });
    return {
      ...result,
      preview_url: `${convertFileSrc(result.cache_path)}?recipe=${result.recipe_id}`,
    };
  },
  loadReferenceStyles: (batchId) =>
    invoke<BackendGroupReferenceStyle[]>("batch_reference_styles", { batchId }),
  preflightReferenceStyleSync: (batchId, sourceGroupId) =>
    invoke<BackendStyleSyncPreflight>("photography_command", {
      commandId: "reference.stylePreflight",
      request: { batch_id: batchId, group_id: sourceGroupId },
    }),
  updateReferenceStyle: (
    batchId, groupId, exposureBiasEv, contrastPreference, saturationPreference,
  ) =>
    invoke<BackendGroupReferenceStyle>("photography_command", {
      commandId: "reference.styleUpdate",
      request: {
        batch_id: batchId, group_id: groupId,
        exposure_bias_ev: exposureBiasEv,
        contrast_preference: contrastPreference,
        saturation_preference: saturationPreference,
      },
    }),
  copyReferenceStyle: (sourceGroupId, targetGroupId) =>
    invoke<BackendGroupReferenceStyle>("copy_group_reference_style", {
      sourceGroupId,
      targetGroupId,
    }),
  copyReferenceStyleToGroups: (batchId, sourceGroupId, targetGroupIds) =>
    invoke<BackendGroupReferenceStyleBatchResult>("photography_command", {
      commandId: "reference.styleCopyBatch",
      request: {
        batch_id: batchId, source_group_id: sourceGroupId,
        target_group_ids: targetGroupIds,
      },
    }),
  loadReferencePreviews: (batchId) =>
    invoke<BackendGroupReferencePreview[]>("batch_reference_previews", { batchId }),
  setHdrMerged: (groupId, merged) =>
    invoke<boolean>("set_group_hdr_merged", { groupId, merged }),
  preflightGroupXmp: (batchId, groupId) =>
    invoke<BackendLightroomHandoffPreflight>("photography_command", {
      commandId: "lightroom.preflight", request: { batch_id: batchId, group_id: groupId },
    }),
  preflightBatchXmp: (batchId, groupIds) =>
    invoke<BackendLightroomBatchPreflightResult>("photography_command", {
      commandId: "lightroom.preflightBatch",
      request: { batch_id: batchId, group_ids: groupIds },
    }),
  writeGroupXmp: (batchId, groupId) =>
    invoke<BackendLightroomHandoffResult>("photography_command", {
      commandId: "lightroom.handoff", request: { batch_id: batchId, group_id: groupId },
    }),
  writeBatchXmp: (batchId, groupIds) =>
    invoke<BackendLightroomBatchHandoffResult>("photography_command", {
      commandId: "lightroom.handoffBatch",
      request: { batch_id: batchId, group_ids: groupIds },
    }),
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
