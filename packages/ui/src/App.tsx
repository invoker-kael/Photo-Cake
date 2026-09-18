import { useEffect, useMemo, useState } from "react";
import {
  demoJobs,
  type BackendBatch,
  type BackendBatchItem,
  type BackendCullingReview,
  type BackendGroupCullingResult,
  type BackendGroupReferencePreview,
  type BackendGroupReferenceStyle,
  type BackendLightroomHandoffResult,
  type BackendPhotoContext,
  type BackendRawMetadataEvidence,
  type BackendReferenceBinding,
  type BackendRecipeReviewOverride,
  type BackendReviewRenderResult,
  type BackendSemanticRefinementReport,
  type BatchJob,
  type BatchStage,
  type CullingDecision,
  type CullingReason,
  type CullingUserDecision,
  type PhotoCakeBridge,
} from "./batch";

export type {
  BackendBatch,
  BackendCullingReview,
  BackendGroupCullingResult,
  BackendGroupReferencePreview,
  BackendGroupReferenceStyle,
  BackendLightroomHandoffResult,
  BackendPhotoContext,
  BackendRawImportResult,
  BackendReferenceBinding,
  BackendRecipeReviewOverride,
  BackendReviewRenderResult,
  BackendSemanticRefinementReport,
  BatchWorkerEvent,
  PhotoCakeBridge,
} from "./batch";

type WorkspaceView = "library" | "cull" | "groups" | "reference" | "review" | "lightroom";
type CullViewMode = "TRIAGE" | "ALL";

const stageProgress: Record<BatchStage, number> = {
  IMPORT: 5,
  ANALYZE: 55,
  APPLY_PRESET: 70,
  PORTRAIT_RETOUCH: 78,
  QA: 86,
  EXPORT: 95,
  DONE: 100,
};

function filenameFromPath(path: string) {
  return path.split(/[\\/]/).filter(Boolean).at(-1) ?? path;
}

function timelineLabel(captureTimeMs: number | null, fileTimeMs: number | null) {
  const value = captureTimeMs ?? fileTimeMs;
  if (value == null) return "Time unavailable";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "Time unavailable";
  const source = captureTimeMs != null ? "EXIF" : "File";
  return `${source} · ${date.toISOString().replace("T", " ").slice(0, 19)}`;
}

function jobFromItem(item: BackendBatchItem): BatchJob {
  return {
    id: item.id,
    filename: filenameFromPath(item.source_path),
    stage: item.stage,
    status: item.status,
    progress: item.status === "DONE" ? 100 : stageProgress[item.stage],
    qa: null,
    error: item.last_error ?? undefined,
  };
}

function stageLabel(stage: BatchStage) {
  const labels: Record<BatchStage, string> = {
    IMPORT: "Import",
    ANALYZE: "Local analysis",
    APPLY_PRESET: "Legacy preset",
    PORTRAIT_RETOUCH: "Legacy portrait stage",
    QA: "Legacy QA",
    EXPORT: "Direct export",
    DONE: "Ready for review",
  };
  return labels[stage];
}

function statusLabel(job: BatchJob) {
  return job.status === "DONE" ? "READY" : job.status;
}

function cullingLabel(decision: CullingDecision) {
  if (decision === "REJECT_SUGGESTION") return "Reject suggestion";
  return decision === "KEEP" ? "Keep" : "Review";
}

function userDecisionLabel(decision: CullingUserDecision) {
  if (decision === "REJECT") return "Reject";
  return decision === "KEEP" ? "Keep" : "Review";
}

function cullingReasonLabel(reason: CullingReason) {
  const labels: Record<CullingReason, string> = {
    STRONG_TECHNICAL_CANDIDATE: "Strong technical candidate",
    LOW_SHARPNESS: "Low sharpness",
    BLUR_RISK: "Blur risk",
    EXPOSURE_RISK: "Exposure risk",
    NEAR_DUPLICATE: "Near duplicate",
    LOW_TECHNICAL_QUALITY: "Low technical quality",
  };
  return labels[reason];
}

function signed(value: number, decimals = 1) {
  return `${value > 0 ? "+" : ""}${value.toFixed(decimals)}`;
}

function whiteBalanceEvidenceLabel(evidence: BackendRawMetadataEvidence | undefined) {
  const wb = evidence?.white_balance;
  if (!wb) return "WB evidence unavailable";
  const kinds = [];
  if (wb.as_shot_neutral) kinds.push("AsShotNeutral");
  if (wb.as_shot_white_xy) kinds.push("WhiteXY");
  return kinds.length ? `RAW WB evidence · ${kinds.join(" + ")}` : "WB evidence unavailable";
}

function JobRow({ job }: { job: BatchJob }) {
  return (
    <div className="job-row">
      <div className="thumb">RAW</div>
      <div className="job-main">
        <div className="job-title-line">
          <strong>{job.filename}</strong>
          <span className={`status status-${job.status.toLowerCase()}`}>{statusLabel(job)}</span>
        </div>
        <div className="job-meta">
          <span>{stageLabel(job.stage)}</span>
          {job.error && <span className="error-text">{job.error}</span>}
        </div>
        <div className="progress"><span style={{ width: `${job.progress}%` }} /></div>
      </div>
      <div className="job-percent">{job.progress}%</div>
    </div>
  );
}

function upsertBatch(current: BackendBatch[], batch: BackendBatch) {
  const index = current.findIndex((candidate) => candidate.id === batch.id);
  if (index < 0) return [batch, ...current];
  const next = current.slice();
  next[index] = batch;
  return next;
}

export interface AppProps {
  bridge?: PhotoCakeBridge;
  mode?: "workstation" | "companion";
}

export default function App({ bridge, mode = "workstation" }: AppProps) {
  const [demoState, setDemoState] = useState(demoJobs);
  const [batches, setBatches] = useState<BackendBatch[]>([]);
  const [activeBatchId, setActiveBatchId] = useState<string | null>(null);
  const [activeView, setActiveView] = useState<WorkspaceView>("library");
  const [demoPaused, setDemoPaused] = useState(false);
  const [backendError, setBackendError] = useState<string | null>(null);
  const [importNote, setImportNote] = useState<string | null>(null);
  const [importing, setImporting] = useState(false);
  const [photoContext, setPhotoContext] = useState<BackendPhotoContext | null>(null);
  const [culling, setCulling] = useState<BackendGroupCullingResult[]>([]);
  const [cullingReviews, setCullingReviews] = useState<Record<string, CullingUserDecision>>({});
  const [cullingLoading, setCullingLoading] = useState(false);
  const [cullViewMode, setCullViewMode] = useState<CullViewMode>("TRIAGE");
  const [cullBatchUpdating, setCullBatchUpdating] = useState(false);
  const [groupRevision, setGroupRevision] = useState(0);
  const [groupRefining, setGroupRefining] = useState(false);
  const [groupRefinementNote, setGroupRefinementNote] = useState<string | null>(null);
  const [referenceBindings, setReferenceBindings] = useState<Record<string, BackendReferenceBinding>>({});
  const [referenceStyles, setReferenceStyles] = useState<Record<string, BackendGroupReferenceStyle>>({});
  const [referencePreviews, setReferencePreviews] = useState<Record<string, BackendGroupReferencePreview>>({});
  const [recipeReviews, setRecipeReviews] = useState<Record<string, BackendRecipeReviewOverride>>({});
  const [editedPreviews, setEditedPreviews] = useState<Record<string, BackendReviewRenderResult>>({});
  const [styleUpdating, setStyleUpdating] = useState<string | null>(null);
  const [reviewUpdating, setReviewUpdating] = useState<string | null>(null);
  const [handoffResults, setHandoffResults] = useState<Record<string, BackendLightroomHandoffResult>>({});
  const [handoffRunning, setHandoffRunning] = useState<string | null>(null);

  useEffect(() => {
    if (!bridge) return;

    let disposed = false;
    let unsubscribe: (() => void) | undefined;

    bridge
      .listBatches()
      .then((loaded) => {
        if (disposed) return;
        setBatches(loaded);
        setActiveBatchId((current) => current ?? loaded[0]?.id ?? null);
        setBackendError(null);
      })
      .catch((error: unknown) => {
        if (!disposed) setBackendError(String(error));
      });

    if (bridge.subscribeBatchUpdates) {
      bridge
        .subscribeBatchUpdates((batch) => {
          if (disposed) return;
          setBatches((current) => upsertBatch(current, batch));
          setActiveBatchId((current) => current ?? batch.id);
          setBackendError(null);
        })
        .then((stop) => {
          if (disposed) stop();
          else unsubscribe = stop;
        })
        .catch((error: unknown) => {
          if (!disposed) setBackendError(String(error));
        });
    }

    return () => {
      disposed = true;
      unsubscribe?.();
    };
  }, [bridge]);

  const activeBatch = useMemo(
    () => batches.find((batch) => batch.id === activeBatchId) ?? batches[0] ?? null,
    [activeBatchId, batches],
  );

  const analysisRevision = useMemo(
    () =>
      activeBatch?.items
        .map((item) => `${item.id}:${item.stage}:${item.status}:${item.attempts}`)
        .join("|") ?? "",
    [activeBatch],
  );

  useEffect(() => {
    if (!bridge?.loadPhotoContext || !activeBatchId) {
      setPhotoContext(null);
      return;
    }

    let disposed = false;
    bridge
      .loadPhotoContext(activeBatchId)
      .then((context) => {
        if (!disposed) setPhotoContext(context);
      })
      .catch((error: unknown) => {
        if (!disposed) setBackendError(String(error));
      });

    return () => {
      disposed = true;
    };
  }, [activeBatchId, analysisRevision, bridge, groupRevision]);

  useEffect(() => {
    if (!bridge?.loadCulling || !activeBatchId) {
      setCulling([]);
      return;
    }

    let disposed = false;
    setCullingLoading(true);
    bridge
      .loadCulling(activeBatchId)
      .then((results) => {
        if (!disposed) {
          setCulling(results);
          setBackendError(null);
        }
      })
      .catch((error: unknown) => {
        if (!disposed) setBackendError(String(error));
      })
      .finally(() => {
        if (!disposed) setCullingLoading(false);
      });

    return () => {
      disposed = true;
    };
  }, [activeBatchId, analysisRevision, bridge, groupRevision]);

  useEffect(() => {
    if (!bridge?.loadCullingReviews || !activeBatchId) {
      setCullingReviews({});
      return;
    }

    let disposed = false;
    bridge
      .loadCullingReviews(activeBatchId)
      .then((reviews) => {
        if (disposed) return;
        setCullingReviews(
          Object.fromEntries(reviews.map((review) => [review.asset_id, review.decision])),
        );
      })
      .catch((error: unknown) => {
        if (!disposed) setBackendError(String(error));
      });

    return () => {
      disposed = true;
    };
  }, [activeBatchId, bridge, groupRevision]);

  useEffect(() => {
    if (!bridge?.loadReferenceBindings || !activeBatchId) {
      setReferenceBindings({});
      return;
    }

    let disposed = false;
    bridge
      .loadReferenceBindings(activeBatchId)
      .then((bindings) => {
        if (disposed) return;
        setReferenceBindings(
          Object.fromEntries(bindings.map((binding) => [binding.group_id, binding])),
        );
      })
      .catch((error: unknown) => {
        if (!disposed) setBackendError(String(error));
      });

    return () => {
      disposed = true;
    };
  }, [activeBatchId, bridge, groupRevision]);

  useEffect(() => {
    if (!bridge?.loadReferenceStyles || !activeBatchId) {
      setReferenceStyles({});
      return;
    }

    let disposed = false;
    bridge
      .loadReferenceStyles(activeBatchId)
      .then((styles) => {
        if (disposed) return;
        setReferenceStyles(
          Object.fromEntries(styles.map((style) => [style.group_id, style])),
        );
      })
      .catch((error: unknown) => {
        if (!disposed) setBackendError(String(error));
      });

    return () => {
      disposed = true;
    };
  }, [activeBatchId, bridge, referenceBindings]);

  useEffect(() => {
    if (!bridge?.loadRecipeReviews || !activeBatchId) {
      setRecipeReviews({});
      return;
    }

    let disposed = false;
    bridge
      .loadRecipeReviews(activeBatchId)
      .then((reviews) => {
        if (disposed) return;
        setRecipeReviews(
          Object.fromEntries(reviews.map((review) => [review.asset_id, review])),
        );
      })
      .catch((error: unknown) => {
        if (!disposed) setBackendError(String(error));
      });

    return () => {
      disposed = true;
    };
  }, [activeBatchId, bridge]);

  useEffect(() => {
    if (!bridge?.loadReferencePreviews || !activeBatchId) {
      setReferencePreviews({});
      return;
    }

    let disposed = false;
    bridge
      .loadReferencePreviews(activeBatchId)
      .then((previews) => {
        if (disposed) return;
        setReferencePreviews(
          Object.fromEntries(previews.map((preview) => [preview.group_id, preview])),
        );
      })
      .catch((error: unknown) => {
        if (!disposed) setBackendError(String(error));
      });

    return () => {
      disposed = true;
    };
  }, [
    activeBatchId,
    analysisRevision,
    bridge,
    cullingReviews,
    recipeReviews,
    referenceBindings,
    referenceStyles,
    groupRevision,
  ]);

  const jobs = useMemo(
    () => (bridge ? activeBatch?.items.map(jobFromItem) ?? [] : demoState),
    [activeBatch, bridge, demoState],
  );

  const summary = useMemo(() => {
    const done = jobs.filter((job) => job.status === "DONE").length;
    const failed = jobs.filter((job) => job.status === "FAILED").length;
    const running = jobs.filter((job) => job.status === "RUNNING").length;
    const pending = jobs.filter((job) => job.status === "PENDING").length;
    const paused = jobs.filter((job) => job.status === "PAUSED").length;
    return { done, failed, running, pending, paused, total: jobs.length };
  }, [jobs]);

  const cullingSummary = useMemo(() => {
    const recommendations = culling.flatMap((group) => group.recommendations);
    let keep = 0;
    let review = 0;
    let reject = 0;
    let rejectSuggestions = 0;

    for (const item of recommendations) {
      const user = cullingReviews[item.asset_id];
      if (user === "KEEP") keep += 1;
      else if (user === "REVIEW") review += 1;
      else if (user === "REJECT") reject += 1;
      else if (item.decision === "KEEP") keep += 1;
      else if (item.decision === "REVIEW") review += 1;
      else rejectSuggestions += 1;
    }

    return {
      keep,
      review,
      reject,
      rejectSuggestions,
      pending: culling.reduce((total, group) => total + group.pending_asset_ids.length, 0),
      confirmed: Object.keys(cullingReviews).length,
    };
  }, [culling, cullingReviews]);

  const assetNames = useMemo(
    () => new Map(photoContext?.assets.map((asset) => [asset.id, asset.filename]) ?? []),
    [photoContext],
  );

  const previewUrls = useMemo(
    () =>
      new Map(
        (photoContext?.previews ?? [])
          .filter((preview) => preview.preview_url)
          .map((preview) => [preview.asset_id, preview.preview_url as string]),
      ),
    [photoContext],
  );

  const metadataEvidence = useMemo(
    () =>
      new Map(
        (photoContext?.metadata ?? []).map((record) => [record.asset_id, record.evidence]),
      ),
    [photoContext],
  );

  const cullingRecommendations = useMemo(() => {
    const values = culling.flatMap((group) => group.recommendations);
    return new Map(values.map((item) => [item.asset_id, item]));
  }, [culling]);

  const cullingGroupNumbers = useMemo(
    () => new Map(culling.map((group, index) => [group.group_id, index + 1])),
    [culling],
  );

  const visibleCulling = useMemo(() => {
    if (cullViewMode === "ALL") return culling;

    return culling
      .map((group) => ({
        ...group,
        recommendations: group.recommendations
          .filter(
            (item) =>
              !cullingReviews[item.asset_id] &&
              item.decision !== "KEEP",
          )
          .slice()
          .sort((left, right) => {
            const priority = (decision: CullingDecision) =>
              decision === "REJECT_SUGGESTION" ? 0 : decision === "REVIEW" ? 1 : 2;
            const byDecision = priority(left.decision) - priority(right.decision);
            if (byDecision !== 0) return byDecision;
            const byQuality = left.quality_score - right.quality_score;
            if (Math.abs(byQuality) > 0.0001) return byQuality;
            return left.group_rank - right.group_rank;
          }),
        pending_asset_ids: group.pending_asset_ids.filter(
          (assetId) => !cullingReviews[assetId],
        ),
      }))
      .filter(
        (group) =>
          group.recommendations.length > 0 || group.pending_asset_ids.length > 0,
      );
  }, [cullViewMode, culling, cullingReviews]);

  const visibleSuggestionCount = useMemo(
    () =>
      visibleCulling.reduce(
        (total, group) =>
          total +
          group.recommendations.filter(
            (item) =>
              !cullingReviews[item.asset_id] &&
              !(
                item.decision === "REJECT_SUGGESTION" &&
                selectedReferenceAssetIds.has(item.asset_id)
              ),
          ).length,
        0,
      ),
    [cullingReviews, selectedReferenceAssetIds, visibleCulling],
  );

  const selectedReferenceAssetIds = useMemo(
    () =>
      new Set(
        Object.values(referenceBindings).map(
          (binding) => binding.selected_reference_asset_id,
        ),
      ),
    [referenceBindings],
  );

  const referenceCandidatesForGroup = (assetIds: string[]) =>
    assetIds
      .filter((assetId) => cullingReviews[assetId] !== "REJECT")
      .sort((left, right) => {
        const score = (assetId: string) => {
          const user = cullingReviews[assetId];
          const recommendation = cullingRecommendations.get(assetId);
          if (user === "KEEP") return 0;
          if (recommendation?.decision === "KEEP") return 1;
          if (user === "REVIEW") return 2;
          if (recommendation?.decision === "REVIEW") return 3;
          return 4;
        };
        const byClass = score(left) - score(right);
        if (byClass !== 0) return byClass;
        return (
          (cullingRecommendations.get(left)?.group_rank ?? Number.MAX_SAFE_INTEGER) -
          (cullingRecommendations.get(right)?.group_rank ?? Number.MAX_SAFE_INTEGER)
        );
      });

  const isPaused = bridge
    ? summary.paused > 0 && summary.running === 0 && summary.pending === 0
    : demoPaused;

  const applyBackendBatch = (batch: BackendBatch) => {
    setBatches((current) => upsertBatch(current, batch));
    setActiveBatchId(batch.id);
    setBackendError(null);
  };

  const runBackendAction = async (action: () => Promise<BackendBatch>) => {
    try {
      applyBackendBatch(await action());
    } catch (error) {
      setBackendError(String(error));
    }
  };

  const importRawDirectory = async () => {
    if (!bridge?.importRawDirectory) return;
    setImporting(true);
    setImportNote(null);
    try {
      const result = await bridge.importRawDirectory();
      if (!result) return;
      if (result.batch) applyBackendBatch(result.batch);
      setPhotoContext({ assets: result.assets, groups: result.groups });
      setActiveView("library");
      setImportNote(
        `${result.assets.length} RAW imported · ${result.groups.length} moment groups` +
          (result.skipped_non_raw.length ? ` · ${result.skipped_non_raw.length} non-RAW skipped` : ""),
      );
    } catch (error) {
      setBackendError(String(error));
    } finally {
      setImporting(false);
    }
  };

  const retryFailed = () => {
    if (bridge?.retryFailed && activeBatch) {
      void runBackendAction(() => bridge.retryFailed!(activeBatch.id));
      return;
    }
    setDemoState((current) =>
      current.map((job) =>
        job.status === "FAILED"
          ? { ...job, status: "PENDING", error: undefined }
          : job,
      ),
    );
  };

  const togglePause = () => {
    if (bridge && activeBatch && bridge.resumeBatch && bridge.pauseBatch) {
      void runBackendAction(() =>
        isPaused ? bridge.resumeBatch!(activeBatch.id) : bridge.pauseBatch!(activeBatch.id),
      );
      return;
    }
    setDemoPaused((value) => !value);
  };

  const startOrContinue = () => {
    if (bridge?.runBatch && activeBatch) {
      void runBackendAction(() => bridge.runBatch!(activeBatch.id));
    }
  };

  const cancelBatch = () => {
    if (bridge?.cancelBatch && activeBatch) {
      void runBackendAction(() => bridge.cancelBatch!(activeBatch.id));
    }
  };

  const refineGroups = async () => {
    if (!bridge?.refineGroups || !activeBatch) return;
    setGroupRefining(true);
    setGroupRefinementNote(null);
    try {
      const report = await bridge.refineGroups(activeBatch.id);
      setPhotoContext((current) =>
        current ? { ...current, groups: report.effective_groups } : current,
      );
      setGroupRevision((value) => value + 1);
      setEditedPreviews({});
      setGroupRefinementNote(
        report.pending_asset_ids.length
          ? `${report.refined_parent_group_ids.length} parent groups refined · ${report.pending_asset_ids.length} photos still waiting for analysis`
          : `${report.refined_parent_group_ids.length} parent groups refined · ${report.effective_groups.length} effective groups ready`,
      );
      setBackendError(null);
    } catch (error) {
      setBackendError(String(error));
    } finally {
      setGroupRefining(false);
    }
  };

  const setPhotoDecision = async (
    assetId: string,
    decision: CullingUserDecision | null,
  ) => {
    if (!bridge?.setCullingReview) return;
    try {
      await bridge.setCullingReview(assetId, decision);
      setCullingReviews((current) => {
        const next = { ...current };
        if (decision) next[assetId] = decision;
        else delete next[assetId];
        return next;
      });
      setEditedPreviews({});
      setBackendError(null);
    } catch (error) {
      setBackendError(String(error));
    }
  };

  const confirmVisibleSuggestions = async () => {
    if (!bridge?.setCullingReviews) return;

    const reviews: BackendCullingReview[] = visibleCulling.flatMap((group) =>
      group.recommendations
        .filter(
          (item) =>
            !cullingReviews[item.asset_id] &&
            !(
              item.decision === "REJECT_SUGGESTION" &&
              selectedReferenceAssetIds.has(item.asset_id)
            ),
        )
        .map((item) => ({
          asset_id: item.asset_id,
          decision:
            item.decision === "KEEP"
              ? "KEEP"
              : item.decision === "REVIEW"
                ? "REVIEW"
                : "REJECT",
        })),
    );
    if (reviews.length === 0) return;

    setCullBatchUpdating(true);
    try {
      await bridge.setCullingReviews(reviews);
      setCullingReviews((current) => ({
        ...current,
        ...Object.fromEntries(reviews.map((review) => [review.asset_id, review.decision])),
      }));
      setEditedPreviews({});
      setBackendError(null);
    } catch (error) {
      setBackendError(String(error));
    } finally {
      setCullBatchUpdating(false);
    }
  };

  const setReferencePhoto = async (groupId: string, assetId: string) => {
    if (!bridge?.setGroupReference) return;
    try {
      const binding = await bridge.setGroupReference(groupId, assetId);
      setReferenceBindings((current) => ({ ...current, [groupId]: binding }));
      setEditedPreviews({});
      setBackendError(null);
    } catch (error) {
      setBackendError(String(error));
    }
  };

  const clearReferencePhoto = async (groupId: string) => {
    if (!bridge?.clearGroupReference) return;
    try {
      await bridge.clearGroupReference(groupId);
      setReferenceBindings((current) => {
        const next = { ...current };
        delete next[groupId];
        return next;
      });
      setEditedPreviews({});
      setBackendError(null);
    } catch (error) {
      setBackendError(String(error));
    }
  };

  const saveReferenceStyle = async (
    groupId: string,
    exposureBiasEv: number,
    contrastPreference: number,
    saturationPreference: number,
  ) => {
    if (!bridge?.updateReferenceStyle) return;
    setStyleUpdating(groupId);
    try {
      const style = await bridge.updateReferenceStyle(
        groupId,
        exposureBiasEv,
        contrastPreference,
        saturationPreference,
      );
      setReferenceStyles((current) => ({ ...current, [groupId]: style }));
      setEditedPreviews({});
      setBackendError(null);
    } catch (error) {
      setBackendError(String(error));
    } finally {
      setStyleUpdating(null);
    }
  };

  const adjustReferenceStyle = (
    groupId: string,
    field: "exposure" | "contrast" | "saturation",
    delta: number,
  ) => {
    const profile = referenceStyles[groupId]?.style_profile;
    let exposure = profile?.exposure_bias_ev ?? 0;
    let contrast = profile?.contrast_preference ?? 0;
    let saturation = profile?.saturation_preference ?? 0;

    if (field === "exposure") exposure = Math.max(-3, Math.min(3, exposure + delta));
    if (field === "contrast") contrast = Math.max(-100, Math.min(100, contrast + delta));
    if (field === "saturation") saturation = Math.max(-100, Math.min(100, saturation + delta));

    void saveReferenceStyle(groupId, exposure, contrast, saturation);
  };

  const resetReferenceStyle = (groupId: string) => {
    void saveReferenceStyle(groupId, 0, 0, 0);
  };

  const saveRecipeReview = async (
    assetId: string,
    exposureDeltaEv: number,
    contrastDelta: number,
    saturationDelta: number,
  ) => {
    if (!bridge?.setRecipeReview) return;
    setReviewUpdating(assetId);
    try {
      const review = await bridge.setRecipeReview(
        assetId,
        exposureDeltaEv,
        contrastDelta,
        saturationDelta,
      );
      setRecipeReviews((current) => {
        const next = { ...current };
        const neutral =
          Math.abs(review.exposure_delta_ev) <= 0.0001 &&
          Math.abs(review.contrast_delta) <= 0.0001 &&
          Math.abs(review.saturation_delta) <= 0.0001;
        if (neutral) delete next[assetId];
        else next[assetId] = review;
        return next;
      });
      setEditedPreviews((current) => {
        const next = { ...current };
        delete next[assetId];
        return next;
      });
      setBackendError(null);
    } catch (error) {
      setBackendError(String(error));
    } finally {
      setReviewUpdating(null);
    }
  };

  const adjustRecipeReview = (
    assetId: string,
    field: "exposure" | "contrast" | "saturation",
    delta: number,
  ) => {
    const current = recipeReviews[assetId];
    let exposure = current?.exposure_delta_ev ?? 0;
    let contrast = current?.contrast_delta ?? 0;
    let saturation = current?.saturation_delta ?? 0;

    if (field === "exposure") exposure = Math.max(-3, Math.min(3, exposure + delta));
    if (field === "contrast") contrast = Math.max(-100, Math.min(100, contrast + delta));
    if (field === "saturation") saturation = Math.max(-100, Math.min(100, saturation + delta));

    void saveRecipeReview(assetId, exposure, contrast, saturation);
  };

  const resetRecipeReview = async (assetId: string) => {
    if (!bridge?.clearRecipeReview) {
      void saveRecipeReview(assetId, 0, 0, 0);
      return;
    }
    setReviewUpdating(assetId);
    try {
      await bridge.clearRecipeReview(assetId);
      setRecipeReviews((current) => {
        const next = { ...current };
        delete next[assetId];
        return next;
      });
      setEditedPreviews((current) => {
        const next = { ...current };
        delete next[assetId];
        return next;
      });
      setBackendError(null);
    } catch (error) {
      setBackendError(String(error));
    } finally {
      setReviewUpdating(null);
    }
  };

  const renderEditedPreview = async (groupId: string, assetId: string) => {
    if (!bridge?.renderRecipePreview) return;
    setReviewUpdating(assetId);
    try {
      const result = await bridge.renderRecipePreview(groupId, assetId);
      setEditedPreviews((current) => ({ ...current, [assetId]: result }));
      setBackendError(null);
    } catch (error) {
      setBackendError(String(error));
    } finally {
      setReviewUpdating(null);
    }
  };

  const writeGroupXmp = async (groupId: string) => {
    if (!bridge?.writeGroupXmp) return;
    setHandoffRunning(groupId);
    try {
      const result = await bridge.writeGroupXmp(groupId);
      setHandoffResults((current) => ({ ...current, [groupId]: result }));
      setBackendError(null);
    } catch (error) {
      setBackendError(String(error));
    } finally {
      setHandoffRunning(null);
    }
  };

  const visibleViews: ReadonlyArray<readonly [WorkspaceView, string]> =
    mode === "companion"
      ? [
          ["library", "Library"],
          ["cull", "Cull"],
          ["groups", "Groups"],
          ["reference", "Reference"],
        ]
      : [
          ["library", "Library"],
          ["cull", "Cull"],
          ["groups", "Groups"],
          ["reference", "Reference"],
          ["review", "Review"],
          ["lightroom", "Lightroom"],
        ];

  const renderLibrary = () => (
    <>
      {photoContext && (
        <section className="photo-overview">
          <div className="overview-card">
            <span>RAW assets</span>
            <strong>{photoContext.assets.length}</strong>
          </div>
          <div className="overview-card">
            <span>Photo groups</span>
            <strong>{photoContext.groups.length}</strong>
          </div>
          <div className="overview-card">
            <span>Ready for review</span>
            <strong>{summary.done}</strong>
          </div>
          <div className="overview-card wide">
            <span>Next workflow</span>
            <strong>
              {mode === "workstation"
                ? "Cull → Groups → Reference → Review → XMP"
                : "Cull → Groups → Reference"}
            </strong>
          </div>
        </section>
      )}

      {photoContext && photoContext.groups.length > 0 && (
        <section className="group-strip">
          <div className="group-strip-head">
            <strong>Initial photo groups</strong>
            <span>Fast moment grouping; semantic refinement follows local analysis</span>
          </div>
          <div className="group-grid">
            {photoContext.groups.slice(0, 8).map((group, index) => (
              <div className="group-card" key={group.id}>
                <span>Group {index + 1}</span>
                <strong>{group.asset_ids.length} photos</strong>
                <small>{group.basis.replaceAll("_", " ").toLowerCase()}</small>
              </div>
            ))}
          </div>
        </section>
      )}

      {photoContext && photoContext.assets.length > 0 && (
        <section className="queue-card">
          <div className="queue-title">
            <strong>Capture metadata</strong>
            <span>RAW/EXIF first · filesystem time only as fallback</span>
          </div>
          <div className="metadata-list">
            {photoContext.assets.slice(0, 20).map((asset) => (
              <div className="metadata-row" key={asset.id}>
                <strong>{asset.filename}</strong>
                <span>{asset.camera_id ?? "Camera metadata unavailable"}</span>
                <small>{timelineLabel(asset.capture_time_ms, asset.file_time_ms)}</small>
                <small>{whiteBalanceEvidenceLabel(metadataEvidence.get(asset.id))}</small>
              </div>
            ))}
          </div>
        </section>
      )}

      <section className="queue-card">
        <div className="queue-title">
          <strong>{mode === "workstation" ? "RAW preparation queue" : "Project preparation state"}</strong>
          <span>
            {mode === "workstation"
              ? "Import and local analysis only · editing starts after photos are ready"
              : "Read-only preparation status from the workstation project"}
          </span>
        </div>
        <div className="job-list">
          {jobs.length > 0
            ? jobs.map((job) => <JobRow job={job} key={job.id} />)
            : (
                <div className="panel-note">
                  {mode === "workstation"
                    ? "Import an existing RAW folder. Source RAW files stay in place."
                    : "Transfer or synchronize a workstation project to this device before mobile culling/reference work."}
                </div>
              )}
        </div>
      </section>
    </>
  );

  const renderCull = () => (
    <>
      <section className="photo-overview">
        <div className="overview-card"><span>Keep</span><strong>{cullingSummary.keep}</strong></div>
        <div className="overview-card"><span>Review</span><strong>{cullingSummary.review}</strong></div>
        <div className="overview-card">
          <span>Reject / AI suggestion</span>
          <strong>{cullingSummary.reject} / {cullingSummary.rejectSuggestions}</strong>
        </div>
        <div className="overview-card"><span>Confirmed / pending</span><strong>{cullingSummary.confirmed} / {cullingSummary.pending}</strong></div>
      </section>

      <section className="queue-card">
        <div className="queue-title">
          <strong>Smart culling</strong>
          <span>Group-relative quality + near-duplicate evidence · never deletes originals</span>
        </div>
        <div className="cull-toolbar">
          <span>
            {cullViewMode === "TRIAGE"
              ? "Triage shows unconfirmed Review / Reject suggestions and analysis-pending photos first."
              : "All shows the complete culling set, including AI Keep and already confirmed photos."}
          </span>
          <div className="cull-toolbar-actions">
            {bridge?.setCullingReviews && (
              <button
                className="cull-batch-action"
                disabled={cullBatchUpdating || visibleSuggestionCount === 0}
                onClick={() => void confirmVisibleSuggestions()}
                title="Persist only currently visible unconfirmed AI suggestions; pending photos, existing photographer decisions and selected references are unchanged."
              >
                {cullBatchUpdating
                  ? "Confirming…"
                  : `Confirm visible (${visibleSuggestionCount})`}
              </button>
            )}
            <div className="cull-view-switch" role="group" aria-label="Culling view mode">
              {(["TRIAGE", "ALL"] as CullViewMode[]).map((viewMode) => (
                <button
                  className={cullViewMode === viewMode ? "active" : ""}
                  key={viewMode}
                  onClick={() => setCullViewMode(viewMode)}
                >
                  {viewMode === "TRIAGE" ? "Triage" : "All"}
                </button>
              ))}
            </div>
          </div>
        </div>
        {cullingLoading && <div className="panel-note">Refreshing cached culling evidence…</div>}
        {!cullingLoading && culling.length === 0 && (
          <div className="panel-note">Import and analyze RAW photos before culling.</div>
        )}
        <div className="cull-groups">
          {!cullingLoading && culling.length > 0 && visibleCulling.length === 0 && (
            <div className="panel-note">
              Triage is clear. Switch to All to review the complete culling set.
            </div>
          )}
          {visibleCulling.map((group) => (
            <div className="cull-group" key={group.group_id}>
              <div className="cull-group-head">
                <strong>Group {cullingGroupNumbers.get(group.group_id) ?? "—"}</strong>
                <span>
                  {group.recommendations.length} scored · {group.pending_asset_ids.length} pending
                </span>
              </div>
              <div className="cull-list">
                {group.recommendations.map((item) => {
                  const userDecision = cullingReviews[item.asset_id];
                  return (
                    <div className="cull-row" key={item.asset_id}>
                      <div className="cull-preview">
                        {previewUrls.get(item.asset_id) ? (
                          <img
                            src={previewUrls.get(item.asset_id)}
                            alt={assetNames.get(item.asset_id) ?? "RAW preview"}
                            loading="lazy"
                          />
                        ) : (
                          <span>RAW</span>
                        )}
                        <small>#{item.group_rank}</small>
                      </div>
                      <div className="cull-name">
                        <strong>{assetNames.get(item.asset_id) ?? item.asset_id.slice(0, 8)}</strong>
                        <small>
                          Quality {Math.round(item.quality_score * 100)}% · Suggested {cullingLabel(item.decision)}
                        </small>
                        {!!item.reasons?.length && (
                          <small className="cull-reasons">
                            {item.reasons.map(cullingReasonLabel).join(" · ")}
                          </small>
                        )}
                        {item.portrait_evidence && (
                          <small>
                            People {item.portrait_evidence.person_count} · Faces {item.portrait_evidence.face_count}
                            {" · "}Subject {Math.round(item.portrait_evidence.primary_subject_ratio * 100)}%
                            {" · "}People confidence {Math.round(item.portrait_evidence.people_confidence * 100)}%
                          </small>
                        )}
                      </div>
                      <div className="cull-review-controls">
                        {(["KEEP", "REVIEW", "REJECT"] as CullingUserDecision[]).map((decision) => (
                          <button
                            className={`review-choice ${userDecision === decision ? "active" : ""}`}
                            key={decision}
                            onClick={() => void setPhotoDecision(item.asset_id, decision)}
                          >
                            {decision === "REJECT" ? "Reject" : decision === "KEEP" ? "Keep" : "Review"}
                          </button>
                        ))}
                        {userDecision && (
                          <button
                            className="review-choice clear"
                            onClick={() => void setPhotoDecision(item.asset_id, null)}
                          >
                            Use suggestion
                          </button>
                        )}
                      </div>
                      <span className={`cull-decision ${userDecision ? "decision-user" : ""}`}>
                        {userDecision ? `Your: ${userDecisionLabel(userDecision)}` : `AI: ${cullingLabel(item.decision)}`}
                      </span>
                    </div>
                  );
                })}
                {group.pending_asset_ids.map((assetId) => {
                  const userDecision = cullingReviews[assetId];
                  return (
                    <div className="cull-row pending" key={assetId}>
                      <div className="cull-preview">
                        {previewUrls.get(assetId) ? (
                          <img
                            src={previewUrls.get(assetId)}
                            alt={assetNames.get(assetId) ?? "RAW preview"}
                            loading="lazy"
                          />
                        ) : (
                          <span>RAW</span>
                        )}
                        <small>—</small>
                      </div>
                      <div className="cull-name">
                        <strong>{assetNames.get(assetId) ?? assetId.slice(0, 8)}</strong>
                        <small>Waiting for local analysis evidence</small>
                      </div>
                      <div className="cull-review-controls">
                        {(["KEEP", "REVIEW", "REJECT"] as CullingUserDecision[]).map((decision) => (
                          <button
                            className={`review-choice ${userDecision === decision ? "active" : ""}`}
                            key={decision}
                            onClick={() => void setPhotoDecision(assetId, decision)}
                          >
                            {decision === "REJECT" ? "Reject" : decision === "KEEP" ? "Keep" : "Review"}
                          </button>
                        ))}
                        {userDecision && (
                          <button className="review-choice clear" onClick={() => void setPhotoDecision(assetId, null)}>
                            Clear
                          </button>
                        )}
                      </div>
                      <span className="cull-decision">
                        {userDecision ? `Your: ${userDecisionLabel(userDecision)}` : "Pending"}
                      </span>
                    </div>
                  );
                })}
              </div>
            </div>
          ))}
        </div>
      </section>
    </>
  );

  const renderGroups = () => (
    <section className="queue-card">
      <div className="queue-title">
        <strong>Photo Groups</strong>
        <div className="group-refine-actions">
          <span>Moment parents are preserved; semantic children become the effective editing groups</span>
          {mode === "workstation" && bridge?.refineGroups && activeBatch && (
            <button
              className="button secondary"
              disabled={groupRefining}
              onClick={() => void refineGroups()}
            >
              {groupRefining ? "Refining…" : "Refine semantic groups"}
            </button>
          )}
        </div>
      </div>
      {groupRefinementNote && <div className="group-refine-note">{groupRefinementNote}</div>}
      <div className="group-detail-grid">
        {photoContext?.groups.map((group, index) => (
          <div className="group-detail-card" key={group.id}>
            <span>Group {index + 1}</span>
            <strong>{group.asset_ids.length} photos</strong>
            <small>{group.kind.toLowerCase()} · {group.basis.replaceAll("_", " ").toLowerCase()}</small>
            {group.manual_locked && <em>Manual lock</em>}
          </div>
        ))}
        {!photoContext?.groups.length && <div className="panel-note">No photo groups yet.</div>}
      </div>
    </section>
  );

  const renderReference = () => (
    <section className="queue-card">
      <div className="queue-title">
        <strong>Reference look</strong>
        <span>Selection is saved now; adaptive edits wait for reliable color evidence</span>
      </div>
      <div className="reference-groups">
        {photoContext?.groups.map((group, index) => {
          const binding = referenceBindings[group.id];
          const style = referenceStyles[group.id]?.style_profile;
          const preview = referencePreviews[group.id];
          const candidates = referenceCandidatesForGroup(group.asset_ids);
          const exposureValues = preview?.recipes
            .map((recipe) => recipe.adjustments.exposure)
            .filter((value): value is number => value != null) ?? [];
          const minExposure = exposureValues.length ? Math.min(...exposureValues) : null;
          const maxExposure = exposureValues.length ? Math.max(...exposureValues) : null;
          const writesWhiteBalance = preview?.recipes.some(
            (recipe) =>
              recipe.adjustments.temperature != null || recipe.adjustments.tint != null,
          ) ?? false;
          return (
            <div className="reference-group" key={group.id}>
              <div className="reference-group-head">
                <div>
                  <span>Group {index + 1}</span>
                  <strong>{group.asset_ids.length} photos</strong>
                </div>
                <div className="reference-current">
                  <small>Selected reference</small>
                  <strong>
                    {binding
                      ? assetNames.get(binding.selected_reference_asset_id) ??
                        binding.selected_reference_asset_id.slice(0, 8)
                      : "Not selected"}
                  </strong>
                  {binding && bridge?.clearGroupReference && (
                    <button
                      className="review-choice clear"
                      onClick={() => void clearReferencePhoto(group.id)}
                    >
                      Clear
                    </button>
                  )}
                  {binding && (
                    <small className="reference-preview-status">
                      {preview?.pending_asset_id
                        ? `Waiting for exposure evidence: ${assetNames.get(preview.pending_asset_id) ?? preview.pending_asset_id.slice(0, 8)}`
                        : preview?.recipes.length
                          ? `${preview.recipes.length} adaptive Recipes · exposure ${minExposure?.toFixed(2)} to ${maxExposure?.toFixed(2)} EV · WB ${writesWhiteBalance ? "measured" : "untouched"}`
                          : "Preparing adaptive preview"}
                    </small>
                  )}
                </div>
              </div>

              {binding && mode === "workstation" && (
                <div className="reference-style">
                  <div className="style-control">
                    <span>Exposure bias</span>
                    <div>
                      <button
                        className="style-step"
                        disabled={styleUpdating != null}
                        onClick={() => adjustReferenceStyle(group.id, "exposure", -0.1)}
                      >−</button>
                      <strong>{signed(style?.exposure_bias_ev ?? 0)} EV</strong>
                      <button
                        className="style-step"
                        disabled={styleUpdating != null}
                        onClick={() => adjustReferenceStyle(group.id, "exposure", 0.1)}
                      >+</button>
                    </div>
                  </div>
                  <div className="style-control">
                    <span>Contrast</span>
                    <div>
                      <button
                        className="style-step"
                        disabled={styleUpdating != null}
                        onClick={() => adjustReferenceStyle(group.id, "contrast", -5)}
                      >−</button>
                      <strong>{signed(style?.contrast_preference ?? 0, 0)}</strong>
                      <button
                        className="style-step"
                        disabled={styleUpdating != null}
                        onClick={() => adjustReferenceStyle(group.id, "contrast", 5)}
                      >+</button>
                    </div>
                  </div>
                  <div className="style-control">
                    <span>Saturation</span>
                    <div>
                      <button
                        className="style-step"
                        disabled={styleUpdating != null}
                        onClick={() => adjustReferenceStyle(group.id, "saturation", -5)}
                      >−</button>
                      <strong>{signed(style?.saturation_preference ?? 0, 0)}</strong>
                      <button
                        className="style-step"
                        disabled={styleUpdating != null}
                        onClick={() => adjustReferenceStyle(group.id, "saturation", 5)}
                      >+</button>
                    </div>
                  </div>
                  <button
                    className="review-choice clear"
                    disabled={styleUpdating != null}
                    onClick={() => resetReferenceStyle(group.id)}
                  >
                    Reset style
                  </button>
                  <small>
                    White balance controls remain locked until reliable RAW/metadata WB evidence exists.
                  </small>
                </div>
              )}

              <div className="reference-candidates">
                {candidates.slice(0, 10).map((assetId) => {
                  const user = cullingReviews[assetId];
                  const recommendation = cullingRecommendations.get(assetId);
                  const selected = binding?.selected_reference_asset_id === assetId;
                  const evidence =
                    user != null
                      ? `Your ${userDecisionLabel(user)}`
                      : recommendation
                        ? `AI ${cullingLabel(recommendation.decision)} · #${recommendation.group_rank}`
                        : "Pending evidence";
                  return (
                    <button
                      className={`reference-candidate ${selected ? "selected" : ""}`}
                      key={assetId}
                      disabled={!bridge?.setGroupReference}
                      onClick={() => void setReferencePhoto(group.id, assetId)}
                    >
                      {previewUrls.get(assetId) ? (
                        <img
                          className="reference-candidate-image"
                          src={previewUrls.get(assetId)}
                          alt={assetNames.get(assetId) ?? "RAW preview"}
                          loading="lazy"
                        />
                      ) : (
                        <div className="reference-candidate-placeholder">RAW</div>
                      )}
                      <span>{assetNames.get(assetId) ?? assetId.slice(0, 8)}</span>
                      <small>{evidence}</small>
                    </button>
                  );
                })}
                {candidates.length === 0 && (
                  <div className="panel-note">
                    No usable reference candidate. Review this group in Cull first.
                  </div>
                )}
              </div>
            </div>
          );
        })}
        {!photoContext?.groups.length && (
          <div className="panel-note">Import and group RAW photos before choosing references.</div>
        )}
      </div>
    </section>
  );

  const renderReview = () => (
    <section className="queue-card">
      <div className="queue-title">
        <strong>Recipe review</strong>
        <span>Group style stays shared · only photo-specific exceptions are stored here</span>
      </div>
      <div className="recipe-review-groups">
        {photoContext?.groups.map((group, groupIndex) => {
          const preview = referencePreviews[group.id];
          if (!preview) {
            return (
              <div className="recipe-review-group" key={group.id}>
                <div className="recipe-review-head">
                  <strong>Group {groupIndex + 1}</strong>
                  <span>Choose a reference before reviewing adaptive Recipes</span>
                </div>
              </div>
            );
          }
          if (preview.pending_asset_id) {
            return (
              <div className="recipe-review-group" key={group.id}>
                <div className="recipe-review-head">
                  <strong>Group {groupIndex + 1}</strong>
                  <span>Waiting for local exposure evidence</span>
                </div>
              </div>
            );
          }

          return (
            <div className="recipe-review-group" key={group.id}>
              <div className="recipe-review-head">
                <strong>Group {groupIndex + 1}</strong>
                <span>{preview.recipes.length} adaptive Recipes</span>
              </div>
              <div className="recipe-review-grid">
                {preview.recipes.map((recipe) => {
                  const assetId = recipe.target_asset_id;
                  if (!assetId) return null;
                  const review = recipeReviews[assetId];
                  const updating = reviewUpdating === assetId;
                  return (
                    <article className="recipe-review-card" key={recipe.id}>
                      <div className="recipe-compare">
                        <div className="recipe-compare-pane">
                          <small>Before</small>
                          {previewUrls.get(assetId) ? (
                            <img
                              className="recipe-review-image"
                              src={previewUrls.get(assetId)}
                              alt={assetNames.get(assetId) ?? "RAW preview"}
                              loading="lazy"
                            />
                          ) : (
                            <div className="recipe-review-placeholder">RAW</div>
                          )}
                        </div>
                        <div className="recipe-compare-pane">
                          <small>After preview</small>
                          {editedPreviews[assetId]?.preview_url ? (
                            <img
                              className="recipe-review-image"
                              src={editedPreviews[assetId].preview_url}
                              alt={`${assetNames.get(assetId) ?? "RAW"} edited preview`}
                              loading="lazy"
                            />
                          ) : (
                            <button
                              className="recipe-preview-button"
                              disabled={updating || !bridge?.renderRecipePreview}
                              onClick={() => void renderEditedPreview(group.id, assetId)}
                            >
                              {updating ? "Rendering…" : "Render edited preview"}
                            </button>
                          )}
                        </div>
                      </div>
                      <div className="recipe-review-info">
                        <strong>{assetNames.get(assetId) ?? assetId.slice(0, 8)}</strong>
                        <small>
                          Final exposure {signed(recipe.adjustments.exposure ?? 0)} EV
                          {review ? ` · override ${signed(review.exposure_delta_ev)} EV` : ""}
                        </small>
                      </div>
                      <small className="recipe-preview-note">
                        Edited preview is a lightweight embedded-JPEG approximation; Lightroom/RAW rendering remains authoritative.
                      </small>
                      <div className="recipe-review-controls">
                        <div className="mini-adjust">
                          <span>Exposure</span>
                          <button disabled={updating} onClick={() => adjustRecipeReview(assetId, "exposure", -0.1)}>−</button>
                          <strong>{signed(review?.exposure_delta_ev ?? 0)} EV</strong>
                          <button disabled={updating} onClick={() => adjustRecipeReview(assetId, "exposure", 0.1)}>+</button>
                        </div>
                        <div className="mini-adjust">
                          <span>Contrast</span>
                          <button disabled={updating} onClick={() => adjustRecipeReview(assetId, "contrast", -5)}>−</button>
                          <strong>{signed(review?.contrast_delta ?? 0, 0)}</strong>
                          <button disabled={updating} onClick={() => adjustRecipeReview(assetId, "contrast", 5)}>+</button>
                        </div>
                        <div className="mini-adjust">
                          <span>Saturation</span>
                          <button disabled={updating} onClick={() => adjustRecipeReview(assetId, "saturation", -5)}>−</button>
                          <strong>{signed(review?.saturation_delta ?? 0, 0)}</strong>
                          <button disabled={updating} onClick={() => adjustRecipeReview(assetId, "saturation", 5)}>+</button>
                        </div>
                      </div>
                      {review && (
                        <button
                          className="review-choice clear recipe-reset"
                          disabled={updating}
                          onClick={() => void resetRecipeReview(assetId)}
                        >
                          Clear exception
                        </button>
                      )}
                    </article>
                  );
                })}
              </div>
            </div>
          );
        })}
        {!photoContext?.groups.length && (
          <div className="panel-note">Import, analyze and choose references before Recipe review.</div>
        )}
      </div>
    </section>
  );

  const renderLightroom = () => (
    <section className="queue-card">
      <div className="queue-title">
        <strong>Lightroom XMP handoff</strong>
        <span>Explicit write only · original RAW stays untouched · existing XMP aborts the whole group</span>
      </div>
      <div className="handoff-groups">
        {photoContext?.groups.map((group, index) => {
          const binding = referenceBindings[group.id];
          const preview = referencePreviews[group.id];
          const result = handoffResults[group.id];
          const ready =
            binding != null &&
            preview != null &&
            preview.pending_asset_id == null &&
            preview.recipes.length > 0;
          const writesWhiteBalance =
            preview?.recipes.some(
              (recipe) =>
                recipe.adjustments.temperature != null || recipe.adjustments.tint != null,
            ) ?? false;

          return (
            <div className="handoff-card" key={group.id}>
              <div className="handoff-card-main">
                <span>Group {index + 1}</span>
                <strong>{group.asset_ids.length} source photos</strong>
                <small>
                  {binding
                    ? `Reference: ${assetNames.get(binding.selected_reference_asset_id) ?? binding.selected_reference_asset_id.slice(0, 8)}`
                    : "Choose a reference first"}
                </small>
                <small>
                  {preview?.pending_asset_id
                    ? "Waiting for exposure evidence"
                    : preview?.recipes.length
                      ? `${preview.recipes.length} XMP targets · WB ${writesWhiteBalance ? "measured" : "untouched"}`
                      : "No adaptive Recipe preview yet"}
                </small>
                <small>Explicit Reject photos are excluded; AI suggestions alone never delete or exclude files.</small>
              </div>

              <div className="handoff-actions">
                {result ? (
                  <>
                    <strong>{result.written_sidecars.length} XMP written</strong>
                    <small>Sidecars created beside the original RAW files</small>
                  </>
                ) : (
                  <>
                    <button
                      className="button primary"
                      disabled={!ready || !bridge?.writeGroupXmp || handoffRunning != null}
                      onClick={() => void writeGroupXmp(group.id)}
                    >
                      {handoffRunning === group.id
                        ? "Writing XMP…"
                        : ready
                          ? `Write ${preview.recipes.length} XMP`
                          : "Not ready"}
                    </button>
                    <small>No overwrite: any existing same-basename XMP stops the group before writing.</small>
                  </>
                )}
              </div>
            </div>
          );
        })}
        {!photoContext?.groups.length && (
          <div className="panel-note">Import, analyze and choose a reference before Lightroom handoff.</div>
        )}
      </div>
    </section>
  );

  const renderFutureView = (title: string, body: string) => (
    <section className="queue-card">
      <div className="queue-title"><strong>{title}</strong></div>
      <div className="panel-note">{body}</div>
    </section>
  );

  return (
    <div className={`app-shell mode-${mode}`}>
      <header className="topbar">
        <div>
          <div className="brand">Photo-Cake</div>
          <div className="subtitle">
            {mode === "workstation"
              ? "RAW → adaptive recipe → Lightroom XMP"
              : "Mobile selection & reference companion"}
          </div>
        </div>
        <div className="top-actions">
          {mode === "workstation" && bridge?.importRawDirectory && (
            <button className="button primary" disabled={importing} onClick={() => void importRawDirectory()}>
              {importing ? "Importing…" : "Import RAW Folder"}
            </button>
          )}
        </div>
      </header>

      <aside className="sidebar">
        <nav>
          {visibleViews.map(([view, label]) => (
            <button
              className={`nav-item ${activeView === view ? "active" : ""}`}
              key={view}
              onClick={() => setActiveView(view)}
            >
              {label}
            </button>
          ))}
        </nav>
        <div className="sidebar-foot">
          <span>{mode === "workstation" ? "Workstation" : "Companion"}</span>
          <strong>Local</strong>
        </div>
      </aside>

      <main className="workspace">
        <section className="batch-head">
          <div>
            <p className="eyebrow">{activeView === "library" ? "RAW PREPARATION" : activeView.toUpperCase()}</p>
            <h1>
              {activeBatch?.name ??
                (bridge
                  ? mode === "workstation"
                    ? "Import a RAW folder"
                    : "No companion project yet"
                  : "Photography workflow")}
            </h1>
            <p>{summary.done}/{summary.total} ready · {summary.running} analyzing · {summary.failed} failed</p>
            {importNote && <p className="success-text">{importNote}</p>}
            {backendError && <p className="error-text">{backendError}</p>}
          </div>
          <div className="batch-controls">
            {mode === "workstation" && bridge?.runBatch && activeBatch && summary.pending > 0 && summary.running === 0 && !isPaused && (
              <button className="button primary" onClick={startOrContinue}>Analyze / Continue</button>
            )}
            {mode === "workstation" && bridge?.retryFailed && summary.failed > 0 && <button className="button secondary" onClick={retryFailed}>Retry failed</button>}
            {mode === "workstation" && bridge?.pauseBatch && bridge?.resumeBatch && jobs.length > 0 && (
              <button className="button secondary" onClick={togglePause}>
                {isPaused ? "Resume" : "Pause"}
              </button>
            )}
            {mode === "workstation" && bridge?.cancelBatch && activeBatch && jobs.some((job) => !["DONE", "CANCELLED"].includes(job.status)) && (
              <button className="button secondary" onClick={cancelBatch}>Cancel</button>
            )}
          </div>
        </section>

        {activeView === "library" && renderLibrary()}
        {activeView === "cull" && renderCull()}
        {activeView === "groups" && renderGroups()}
        {activeView === "reference" && renderReference()}
        {activeView === "review" && renderReview()}
        {activeView === "lightroom" && renderLightroom()}
      </main>

      <aside className="automation-panel">
        <p className="eyebrow">PHOTOGRAPHY WORKFLOW</p>
        <h2>Semi-automatic, reference driven</h2>
        <p className="panel-note">
          {mode === "workstation"
            ? "Background analysis prepares reusable evidence. Photo-Cake then helps you review groups, choose a reference look, adapt it per photo and hand tiny XMP sidecars to Lightroom."
            : "Companion reuses the same project decisions for mobile culling and reference selection. RAW analysis, Recipe editing and Lightroom handoff remain on the workstation."}
        </p>

        <div className="workflow-list">
          {(mode === "workstation"
            ? [
                ["1", "Import & analyze", "Keep RAW untouched"],
                ["2", "Cull", "Quality + near-duplicate evidence"],
                ["3", "Group", "Moment → semantic similarity"],
                ["4", "Reference look", "Your preferred photo/style"],
                ["5", "Adaptive recipe", "Different correction per photo"],
                ["6", "Review exceptions", "Persist only per-photo corrections"],
                ["7", "Lightroom XMP", "Or direct export on demand"],
              ]
            : [
                ["1", "Library", "Synced project context"],
                ["2", "Cull", "Keep / Review / Reject"],
                ["3", "Groups", "Use effective project groups"],
                ["4", "Reference", "Choose the preferred photo"],
              ]
          ).map(([number, title, detail]) => (
            <div className="workflow-step" key={number}>
              <span className="workflow-number">{number}</span>
              <div>
                <strong>{title}</strong>
                <small>{detail}</small>
              </div>
            </div>
          ))}
        </div>

        <div className="storage-note">
          <strong>Default storage</strong>
          <span>
            {mode === "workstation"
              ? "Original RAW + small XMP. No automatic TIFF/JPEG working copies."
              : "Companion stores project decisions and preview context; original RAW stays workstation-owned."}
          </span>
        </div>
      </aside>
    </div>
  );
}
