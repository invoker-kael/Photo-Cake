import { useEffect, useMemo, useState } from "react";
import {
  demoJobs,
  type BackendBatch,
  type BackendBatchItem,
  type BackendCullingReview,
  type BackendGroupCullingResult,
  type BackendGroupReferencePreview,
  type BackendGroupReferenceStyle,
  type BackendLightroomBatchHandoffResult,
  type BackendLightroomHandoffPreflight,
  type BackendLightroomHandoffResult,
  type BackendMomentQuickCullBatchResult,
  type BackendMomentQuickCullOperation,
  type BackendPhotoContext,
  type BackendRawMetadataEvidence,
  type BackendReferenceBatchItem,
  type BackendReferenceBatchResult,
  type BackendReferenceBinding,
  type BackendRecipeReviewBatchItem,
  type BackendRecipeReviewBatchResult,
  type BackendRecipeReviewGroupBatchResult,
  type BackendRecipeReviewOverride,
  type BackendRecipeReviewSyncFields,
  type BackendRecipeReviewSyncResult,
  type BackendReviewRenderResult,
  type BackendSemanticRefinementReport,
  type BackendWorkflowStatus,
  type BackendStyleProfile,
  type BackendSceneTag,
  type BatchJob,
  type BatchStage,
  type CullingDecision,
  type CullingReason,
  type CullingUserDecision,
  type PhotoCakeBridge,
  type WorkflowFocus,
} from "./batch";

export type {
  BackendBatch,
  BackendCullingReview,
  BackendGroupCullingResult,
  BackendGroupReferencePreview,
  BackendGroupReferenceStyle,
  BackendGroupReferenceStyleBatchResult,
  BackendLightroomBatchHandoffResult,
  BackendLightroomHandoffPreflight,
  BackendLightroomHandoffResult,
  BackendMomentQuickCullBatchResult,
  BackendMomentQuickCullOperation,
  BackendPhotoContext,
  BackendRawImportResult,
  BackendReferenceBatchItem,
  BackendReferenceBatchResult,
  BackendReferenceBinding,
  BackendRecipeReviewBatchItem,
  BackendRecipeReviewBatchResult,
  BackendRecipeReviewGroupBatchResult,
  BackendRecipeReviewOverride,
  BackendRecipeReviewSyncFields,
  BackendRecipeReviewSyncResult,
  BackendReviewRenderResult,
  BackendSemanticRefinementReport,
  BackendWorkflowStatus,
  BatchWorkerEvent,
  PhotoCakeBridge,
  WorkflowFocus,
} from "./batch";

type WorkspaceView = "library" | "cull" | "groups" | "reference" | "review" | "lightroom";
type CullViewMode = "TRIAGE" | "ALL";
type ReviewViewMode = "TRIAGE" | "ALL";
type LightroomViewMode = "NEEDS_ACTION" | "ALL";

const workflowFocusOrder: Record<WorkflowFocus, number> = {
  PREPARE: 0,
  CULL: 1,
  REFERENCE: 3,
  REVIEW: 5,
  LIGHTROOM: 6,
  COMPLETE: 7,
};

function workflowFocusLabel(focus: WorkflowFocus) {
  const labels: Record<WorkflowFocus, string> = {
    PREPARE: "Import & analyze",
    CULL: "Cull attention",
    REFERENCE: "Choose references",
    REVIEW: "Review exceptions",
    LIGHTROOM: "Lightroom handoff",
    COMPLETE: "Delivery current",
  };
  return labels[focus];
}

function workflowFocusView(focus: WorkflowFocus): WorkspaceView {
  switch (focus) {
    case "PREPARE":
      return "library";
    case "CULL":
      return "cull";
    case "REFERENCE":
      return "reference";
    case "REVIEW":
      return "review";
    case "LIGHTROOM":
    case "COMPLETE":
      return "lightroom";
  }
}

function workflowFocusDetail(status: BackendWorkflowStatus) {
  const facts = status.facts;
  switch (status.next_focus) {
    case "PREPARE":
      return facts.preparation_failed > 0
        ? `${facts.preparation_failed} failed · ${facts.preparation_active} still preparing`
        : `${facts.preparation_active} photos still preparing`;
    case "CULL":
      return `${facts.cull_attention} cull attention · ${facts.cull_pending} waiting for evidence`;
    case "REFERENCE":
      return `${facts.reference_attention_groups} groups need a usable Reference`;
    case "REVIEW":
      return `${facts.review_attention} Recipe attention · ${facts.review_pending_groups} groups waiting for evidence`;
    case "LIGHTROOM":
      return `${facts.lightroom_hdr_merge_groups} HDR merge · ${facts.lightroom_conflict_groups} conflict groups · ${facts.lightroom_missing_sidecars} missing XMP · ${facts.lightroom_unresolved_groups} unresolved`;
    case "COMPLETE":
      return `${facts.lightroom_current_groups} groups are current for Lightroom delivery`;
  }
}

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
    EXPOSURE_BRACKET_MEMBER: "Exposure bracket · preserve frame",
    NEAR_DUPLICATE: "Near duplicate",
    LOW_TECHNICAL_QUALITY: "Low technical quality",
  };
  return labels[reason];
}

function sceneTagLabel(tag: BackendSceneTag) {
  const labels: Record<BackendSceneTag, string> = {
    LANDSCAPE: "Landscape",
    ARCHITECTURE: "Architecture",
    FOOD: "Food",
    NIGHT: "Night",
    DOCUMENT: "Document",
    OTHER: "Other",
  };
  return labels[tag];
}

function signed(value: number, decimals = 1) {
  return `${value > 0 ? "+" : ""}${value.toFixed(decimals)}`;
}

function sameOptionalNumber(left: number | null, right: number | null) {
  if (left == null || right == null) return left === right;
  return Math.abs(left - right) <= 0.0001;
}

function sameVisualStyle(
  left: BackendStyleProfile | undefined,
  right: BackendStyleProfile | undefined,
) {
  if (!left || !right) return false;
  return (
    sameOptionalNumber(left.exposure_bias_ev, right.exposure_bias_ev) &&
    sameOptionalNumber(left.temperature_bias, right.temperature_bias) &&
    sameOptionalNumber(left.tint_bias, right.tint_bias) &&
    sameOptionalNumber(left.contrast_preference, right.contrast_preference) &&
    sameOptionalNumber(left.saturation_preference, right.saturation_preference)
  );
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
  const [workflowStatus, setWorkflowStatus] = useState<BackendWorkflowStatus | null>(null);
  const [culling, setCulling] = useState<BackendGroupCullingResult[]>([]);
  const [cullingReviews, setCullingReviews] = useState<Record<string, CullingUserDecision>>({});
  const [cullingLoading, setCullingLoading] = useState(false);
  const [cullViewMode, setCullViewMode] = useState<CullViewMode>("TRIAGE");
  const [reviewViewMode, setReviewViewMode] = useState<ReviewViewMode>("TRIAGE");
  const [lightroomViewMode, setLightroomViewMode] = useState<LightroomViewMode>("NEEDS_ACTION");
  const [cullBatchUpdating, setCullBatchUpdating] = useState(false);
  const [momentQuickCullTargets, setMomentQuickCullTargets] = useState<string[]>([]);
  const [momentQuickCullUpdating, setMomentQuickCullUpdating] = useState(false);
  const [momentQuickCullUndoing, setMomentQuickCullUndoing] = useState(false);
  const [latestMomentQuickCull, setLatestMomentQuickCull] =
    useState<BackendMomentQuickCullOperation | null>(null);
  const [momentQuickCullNote, setMomentQuickCullNote] = useState<string | null>(null);
  const [groupRevision, setGroupRevision] = useState(0);
  const [groupRefining, setGroupRefining] = useState(false);
  const [groupRefinementNote, setGroupRefinementNote] = useState<string | null>(null);
  const [groupMutationRunning, setGroupMutationRunning] = useState(false);
  const [groupMergeSelection, setGroupMergeSelection] = useState<string[]>([]);
  const [groupSplitPoints, setGroupSplitPoints] = useState<Record<string, string>>({});
  const [referenceBindings, setReferenceBindings] = useState<Record<string, BackendReferenceBinding>>({});
  const [referenceBatchTargets, setReferenceBatchTargets] = useState<string[]>([]);
  const [referenceBatchUpdating, setReferenceBatchUpdating] = useState(false);
  const [referenceBatchNote, setReferenceBatchNote] = useState<string | null>(null);
  const [referenceStyles, setReferenceStyles] = useState<Record<string, BackendGroupReferenceStyle>>({});
  const [referencePreviews, setReferencePreviews] = useState<Record<string, BackendGroupReferencePreview>>({});
  const [recipeReviews, setRecipeReviews] = useState<Record<string, BackendRecipeReviewOverride>>({});
  const [editedPreviews, setEditedPreviews] = useState<Record<string, BackendReviewRenderResult>>({});
  const [styleUpdating, setStyleUpdating] = useState<string | null>(null);
  const [styleBatchSource, setStyleBatchSource] = useState("");
  const [styleBatchTargets, setStyleBatchTargets] = useState<string[]>([]);
  const [styleBatchUpdating, setStyleBatchUpdating] = useState(false);
  const [styleBatchNote, setStyleBatchNote] = useState<string | null>(null);
  const [reviewUpdating, setReviewUpdating] = useState<string | null>(null);
  const [reviewBatchUpdating, setReviewBatchUpdating] = useState(false);
  const [reviewBatchNote, setReviewBatchNote] = useState<string | null>(null);
  const [reviewSyncSource, setReviewSyncSource] = useState<{ groupId: string; assetId: string } | null>(null);
  const [reviewSyncTargets, setReviewSyncTargets] = useState<string[]>([]);
  const [reviewSyncFields, setReviewSyncFields] = useState<BackendRecipeReviewSyncFields>({
    exposure: true,
    contrast: true,
    saturation: true,
  });
  const [reviewSyncUpdating, setReviewSyncUpdating] = useState(false);
  const [reviewSyncNote, setReviewSyncNote] = useState<string | null>(null);
  const [handoffPreflights, setHandoffPreflights] = useState<Record<string, BackendLightroomHandoffPreflight>>({});
  const [handoffPreflightErrors, setHandoffPreflightErrors] = useState<Record<string, string>>({});
  const [handoffPreflightLoading, setHandoffPreflightLoading] = useState(false);
  const [handoffPreflightRevision, setHandoffPreflightRevision] = useState(0);
  const [handoffResults, setHandoffResults] = useState<Record<string, BackendLightroomHandoffResult>>({});
  const [handoffRunning, setHandoffRunning] = useState<string | null>(null);
  const [hdrMergeUpdating, setHdrMergeUpdating] = useState<string | null>(null);
  const [handoffBatchRunning, setHandoffBatchRunning] = useState(false);
  const [handoffBatchTargets, setHandoffBatchTargets] = useState<string[]>([]);
  const [handoffBatchNote, setHandoffBatchNote] = useState<string | null>(null);

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
    setGroupMergeSelection([]);
    setGroupSplitPoints({});
    setGroupRefinementNote(null);
    setReferenceBatchTargets([]);
    setReferenceBatchNote(null);
    setReviewBatchNote(null);
    setReviewSyncSource(null);
    setReviewSyncTargets([]);
    setReviewSyncFields({ exposure: true, contrast: true, saturation: true });
    setReviewSyncNote(null);
    setHandoffPreflightErrors({});
    setHandoffBatchTargets([]);
    setHandoffBatchNote(null);
  }, [activeBatchId]);

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
    if (!bridge?.loadLatestMomentQuickCull || !activeBatchId) {
      setLatestMomentQuickCull(null);
      return;
    }

    let disposed = false;
    bridge
      .loadLatestMomentQuickCull(activeBatchId)
      .then((operation) => {
        if (!disposed) setLatestMomentQuickCull(operation);
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

  useEffect(() => {
    if (
      activeView !== "lightroom" ||
      !bridge?.preflightGroupXmp ||
      !photoContext?.groups.length
    ) {
      if (activeView !== "lightroom") {
        setHandoffPreflights({});
        setHandoffPreflightErrors({});
      }
      return;
    }

    const bracketGroupIds = new Set(
      culling
        .filter((group) => (group.exposure_brackets?.length ?? 0) > 0)
        .map((group) => group.group_id),
    );
    const groups = photoContext.groups.filter((group) => {
      if (bracketGroupIds.has(group.id)) return true;
      const preview = referencePreviews[group.id];
      return (
        referenceBindings[group.id] != null &&
        preview != null &&
        preview.pending_asset_id == null &&
        preview.recipes.length > 0
      );
    });
    if (groups.length === 0) {
      setHandoffPreflights({});
      setHandoffPreflightErrors({});
      return;
    }

    let disposed = false;
    setHandoffPreflightLoading(true);
    Promise.allSettled(
      groups.map(async (group) => {
        const preflight = await bridge.preflightGroupXmp!(group.id);
        return [group.id, preflight] as const;
      }),
    )
      .then((results) => {
        if (disposed) return;
        const preflights: Record<string, BackendLightroomHandoffPreflight> = {};
        const errors: Record<string, string> = {};
        results.forEach((result, index) => {
          const groupId = groups[index].id;
          if (result.status === "fulfilled") {
            const [resolvedGroupId, preflight] = result.value;
            preflights[resolvedGroupId] = preflight;
          } else {
            errors[groupId] = String(result.reason);
          }
        });
        setHandoffPreflights(preflights);
        setHandoffPreflightErrors(errors);
        setBackendError(null);
      })
      .finally(() => {
        if (!disposed) setHandoffPreflightLoading(false);
      });

    return () => {
      disposed = true;
    };
  }, [
    activeView,
    bridge,
    culling,
    cullingReviews,
    handoffPreflightRevision,
    photoContext,
    recipeReviews,
    referenceBindings,
    referencePreviews,
  ]);

  useEffect(() => {
    setHandoffResults({});
    setHandoffBatchTargets([]);
    setHandoffBatchNote(null);
  }, [activeBatchId, cullingReviews, recipeReviews, referenceBindings, referenceStyles, groupRevision]);

  useEffect(() => {
    setMomentQuickCullTargets([]);
    setMomentQuickCullNote(null);
  }, [activeBatchId, groupRevision]);

  useEffect(() => {
    if (!bridge?.loadWorkflowStatus || !activeBatchId || mode !== "workstation") {
      setWorkflowStatus(null);
      return;
    }

    let disposed = false;
    bridge
      .loadWorkflowStatus(activeBatchId)
      .then((status) => {
        if (!disposed) {
          setWorkflowStatus(status);
          setBackendError(null);
        }
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
    groupRevision,
    handoffPreflights,
    handoffResults,
    mode,
    recipeReviews,
    referenceBindings,
    referencePreviews,
    referenceStyles,
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

  const reviewedRecipeAssetIds = useMemo(
    () =>
      new Set(
        Object.values(referencePreviews).flatMap(
          (preview) => preview.reviewed_asset_ids ?? [],
        ),
      ),
    [referencePreviews],
  );

  const recipeReviewPriority = (assetId: string) => {
    const userDecision = cullingReviews[assetId];
    if (userDecision === "REJECT") return 99;
    if (reviewedRecipeAssetIds.has(assetId)) return 90;
    if (recipeReviews[assetId]) return 0;
    if (userDecision === "REVIEW") return 1;

    const recommendation = cullingRecommendations.get(assetId);
    if (!userDecision && recommendation?.decision === "REJECT_SUGGESTION") return 2;
    if (!userDecision && recommendation?.decision === "REVIEW") return 3;
    return 10;
  };

  const recipeReviewLabel = (assetId: string) => {
    const userDecision = cullingReviews[assetId];
    if (userDecision === "REJECT") return "Photographer Reject · excluded from Lightroom";
    if (reviewedRecipeAssetIds.has(assetId)) return "Reviewed · current Recipe looks good";
    if (recipeReviews[assetId]) return "Saved per-photo exception";
    if (userDecision === "REVIEW") return "Photographer marked Review";

    const recommendation = cullingRecommendations.get(assetId);
    if (!userDecision && recommendation?.decision === "REJECT_SUGGESTION") {
      return "AI cull: Reject suggestion";
    }
    if (!userDecision && recommendation?.decision === "REVIEW") {
      return "AI cull: Review";
    }
    return "Adaptive Recipe";
  };

  const recipeReviewItems = (photoContext?.groups ?? []).flatMap((group) => {
    const preview = referencePreviews[group.id];
    if (!preview || preview.pending_asset_id) return [];
    return preview.recipes.flatMap((recipe) => {
      const assetId = recipe.target_asset_id;
      if (!assetId || cullingReviews[assetId] === "REJECT") return [];
      return [{
        group_id: group.id,
        asset_id: assetId,
        reviewed: reviewedRecipeAssetIds.has(assetId),
        attention: recipeReviewPriority(assetId) < 10,
        exception: recipeReviews[assetId] != null,
      }];
    });
  });

  const visibleRecipeReviewItems: BackendRecipeReviewBatchItem[] = recipeReviewItems
    .filter(
      (item) =>
        !item.reviewed &&
        (reviewViewMode === "ALL" || item.attention),
    )
    .map(({ group_id, asset_id }) => ({ group_id, asset_id }));

  const rejectedReviewCount = new Set(
    (photoContext?.groups ?? []).flatMap((group) =>
      group.asset_ids.filter((assetId) => cullingReviews[assetId] === "REJECT"),
    ),
  ).size;

  const recipeReviewSummary = {
    total: recipeReviewItems.length,
    attention: recipeReviewItems.filter((item) => item.attention).length,
    confirmed: recipeReviewItems.filter((item) => item.reviewed).length,
    exceptions: recipeReviewItems.filter((item) => item.exception).length,
    rejected: rejectedReviewCount,
  };

  const clearRecipeReviewGroups = (photoContext?.groups ?? []).flatMap((group) => {
    const preview = referencePreviews[group.id];
    const cullingGroup = culling.find((value) => value.group_id === group.id);
    if (!preview || preview.pending_asset_id || !cullingGroup) return [];

    const pending = new Set(cullingGroup.pending_asset_ids);
    const assetIds = preview.recipes.flatMap((recipe) => {
      const assetId = recipe.target_asset_id;
      if (
        !assetId ||
        cullingReviews[assetId] === "REJECT" ||
        reviewedRecipeAssetIds.has(assetId)
      ) return [];
      return [assetId];
    });
    if (assetIds.length === 0) return [];

    const clear = assetIds.every((assetId) => {
      const userDecision = cullingReviews[assetId];
      if (!userDecision && (pending.has(assetId) || !cullingRecommendations.has(assetId))) {
        return false;
      }
      return recipeReviewPriority(assetId) >= 10;
    });
    return clear ? [{ group_id: group.id, asset_ids: assetIds }] : [];
  });
  const clearRecipeReviewAssetCount = clearRecipeReviewGroups.reduce(
    (total, group) => total + group.asset_ids.length,
    0,
  );

  const cullingGroupNumbers = useMemo(
    () => new Map(culling.map((group, index) => [group.group_id, index + 1])),
    [culling],
  );

  const cullingGroupsById = useMemo(
    () => new Map(culling.map((group) => [group.group_id, group])),
    [culling],
  );
  const bracketSetsForGroup = (groupId: string) =>
    cullingGroupsById.get(groupId)?.exposure_brackets ?? [];

  const momentQuickCullEligibleGroups = culling.filter((group) => {
    const plan = group.moment_quick_cull;
    if (!plan || referenceBindings[group.group_id] != null) return false;
    return [...plan.keep_asset_ids, ...plan.review_asset_ids, ...plan.reject_asset_ids]
      .every((assetId) => cullingReviews[assetId] == null);
  });
  const eligibleMomentQuickCullGroupIds = new Set(
    momentQuickCullEligibleGroups.map((group) => group.group_id),
  );
  const selectedMomentQuickCullGroups = momentQuickCullEligibleGroups.filter((group) =>
    momentQuickCullTargets.includes(group.group_id),
  );
  const selectedMomentQuickCullCounts = selectedMomentQuickCullGroups.reduce(
    (total, group) => {
      const plan = group.moment_quick_cull!;
      total.keep += plan.keep_asset_ids.length;
      total.review += plan.review_asset_ids.length;
      total.reject += plan.reject_asset_ids.length;
      return total;
    },
    { keep: 0, review: 0, reject: 0 },
  );
  const selectedMomentQuickCullPhotoCount =
    selectedMomentQuickCullCounts.keep +
    selectedMomentQuickCullCounts.review +
    selectedMomentQuickCullCounts.reject;
  const momentQuickCullPeopleGroupCount = momentQuickCullEligibleGroups.filter(
    (group) => group.moment_quick_cull?.contains_people,
  ).length;
  const momentQuickCullLandscapeGroupCount = momentQuickCullEligibleGroups.filter(
    (group) => group.moment_quick_cull?.shared_scene_tags.includes("LANDSCAPE"),
  ).length;
  const momentQuickCullConservativeGroupCount = momentQuickCullEligibleGroups.filter(
    (group) => {
      const plan = group.moment_quick_cull;
      return !!plan && (
        !plan.people_evidence_complete ||
        !plan.scene_evidence_complete ||
        (!plan.contains_people && !plan.scene_consistent)
      );
    },
  ).length;

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

  const selectedReferenceAssetIds = useMemo(
    () =>
      new Set(
        Object.values(referenceBindings).map(
          (binding) => binding.selected_reference_asset_id,
        ),
      ),
    [referenceBindings],
  );

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

  const referenceCandidatesForGroup = (groupId: string, assetIds: string[]) => {
    const bracketCenters = new Set(
      bracketSetsForGroup(groupId).map((set) => set.center_asset_id),
    );
    return assetIds
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

        const byBracketCenter =
          Number(bracketCenters.has(right)) - Number(bracketCenters.has(left));
        if (byBracketCenter !== 0) return byBracketCenter;

        const byQuality =
          (cullingRecommendations.get(right)?.quality_score ?? -1) -
          (cullingRecommendations.get(left)?.quality_score ?? -1);
        if (byQuality !== 0) return byQuality;

        return (
          (cullingRecommendations.get(left)?.group_rank ?? Number.MAX_SAFE_INTEGER) -
          (cullingRecommendations.get(right)?.group_rank ?? Number.MAX_SAFE_INTEGER)
        );
      });
  };

  const batchReferenceCandidateForGroup = (groupId: string, assetIds: string[]) => {
    const candidates = referenceCandidatesForGroup(groupId, assetIds);
    const explicitKeep = candidates.find(
      (assetId) => cullingReviews[assetId] === "KEEP",
    );
    if (explicitKeep) return explicitKeep;

    const bracketCenters = new Set(
      bracketSetsForGroup(groupId).map((set) => set.center_asset_id),
    );
    const bracketCenter = candidates.find((assetId) => {
      if (!bracketCenters.has(assetId)) return false;
      const recommendation = cullingRecommendations.get(assetId);
      return recommendation != null && recommendation.decision !== "REJECT_SUGGESTION";
    });
    if (bracketCenter) return bracketCenter;

    return candidates.find((assetId) => {
      const user = cullingReviews[assetId];
      if (user === "KEEP" || user === "REVIEW") return true;
      if (user === "REJECT") return false;
      const recommendation = cullingRecommendations.get(assetId);
      return (
        recommendation != null &&
        recommendation.decision !== "REJECT_SUGGESTION"
      );
    });
  };

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
      setGroupMergeSelection([]);
      setGroupSplitPoints({});
      setGroupRevision((value) => value + 1);
      setEditedPreviews({});
      const bracketNote = report.protected_bracket_parent_group_ids.length
        ? ` · ${report.protected_bracket_parent_group_ids.length} exposure-bracket group(s) preserved`
        : "";
      setGroupRefinementNote(
        report.pending_asset_ids.length
          ? `${report.refined_parent_group_ids.length} parent groups refined · ${report.pending_asset_ids.length} photos still waiting for analysis${bracketNote}`
          : `${report.refined_parent_group_ids.length} parent groups refined · ${report.effective_groups.length} effective groups ready${bracketNote}`,
      );
      setBackendError(null);
    } catch (error) {
      setBackendError(String(error));
    } finally {
      setGroupRefining(false);
    }
  };

  const applyGroupMutation = (
    groups: BackendPhotoContext["groups"],
    note: string,
  ) => {
    setPhotoContext((current) => (current ? { ...current, groups } : current));
    setGroupMergeSelection([]);
    setGroupSplitPoints({});
    setGroupRevision((value) => value + 1);
    setEditedPreviews({});
    setGroupRefinementNote(note);
    setBackendError(null);
  };

  const runGroupMutation = async (
    action: () => Promise<BackendPhotoContext["groups"]>,
    note: string,
  ) => {
    setGroupMutationRunning(true);
    try {
      applyGroupMutation(await action(), note);
    } catch (error) {
      setBackendError(String(error));
    } finally {
      setGroupMutationRunning(false);
    }
  };

  const keepMomentTogether = (groupId: string) => {
    if (!bridge?.keepMomentTogether || !activeBatch) return;
    void runGroupMutation(
      () => bridge.keepMomentTogether!(activeBatch.id, groupId),
      "Semantic split reverted. The original moment is now locked together.",
    );
  };

  const allowGroupRefinement = (groupId: string) => {
    if (!bridge?.allowGroupRefinement || !activeBatch) return;
    void runGroupMutation(
      () => bridge.allowGroupRefinement!(activeBatch.id, groupId),
      "Moment unlocked. Run semantic refinement when you want Photo-Cake to split it again.",
    );
  };

  const mergeSelectedGroups = () => {
    if (!bridge?.mergeGroups || !activeBatch || groupMergeSelection.length < 2) return;
    void runGroupMutation(
      () => bridge.mergeGroups!(activeBatch.id, groupMergeSelection),
      `Merged ${groupMergeSelection.length} adjacent groups into one manual-locked editing context.`,
    );
  };

  const splitGroupAtSelectedPhoto = (groupId: string, fallbackAssetId: string) => {
    if (!bridge?.splitGroup || !activeBatch) return;
    const splitBeforeAssetId = groupSplitPoints[groupId] ?? fallbackAssetId;
    if (!splitBeforeAssetId) return;
    void runGroupMutation(
      () => bridge.splitGroup!(activeBatch.id, groupId, splitBeforeAssetId),
      "Group split into two manual-locked editing contexts.",
    );
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

  const confirmSelectedMomentQuickCull = async () => {
    if (
      !bridge?.confirmMomentQuickCull ||
      !activeBatch ||
      selectedMomentQuickCullGroups.length === 0
    ) {
      return;
    }

    setMomentQuickCullUpdating(true);
    setMomentQuickCullNote(null);
    try {
      const result: BackendMomentQuickCullBatchResult =
        await bridge.confirmMomentQuickCull(
          activeBatch.id,
          selectedMomentQuickCullGroups.map((group) => group.group_id),
        );
      setCullingReviews((current) => ({
        ...current,
        ...Object.fromEntries(
          result.reviews.map((review) => [review.asset_id, review.decision]),
        ),
      }));
      setMomentQuickCullTargets([]);
      setLatestMomentQuickCull(result.operation);
      const keepCount = result.reviews.filter((review) => review.decision === "KEEP").length;
      const reviewCount = result.reviews.filter((review) => review.decision === "REVIEW").length;
      const rejectCount = result.reviews.filter((review) => review.decision === "REJECT").length;
      setMomentQuickCullNote(
        `Quick-culled ${result.group_ids.length} moments: ${keepCount} primary Keep · ${reviewCount} protected Review · ${rejectCount} clear duplicate Reject.`,
      );
      setEditedPreviews({});
      setBackendError(null);
    } catch (error) {
      setBackendError(String(error));
    } finally {
      setMomentQuickCullUpdating(false);
    }
  };

  const undoLatestMomentQuickCull = async () => {
    if (
      !bridge?.undoMomentQuickCull ||
      !activeBatch ||
      !latestMomentQuickCull
    ) {
      return;
    }

    setMomentQuickCullUndoing(true);
    setMomentQuickCullNote(null);
    try {
      const undone = await bridge.undoMomentQuickCull(
        activeBatch.id,
        latestMomentQuickCull.operation_id,
      );
      setCullingReviews((current) => {
        const next = { ...current };
        undone.reviews.forEach((review) => delete next[review.asset_id]);
        return next;
      });
      setLatestMomentQuickCull(
        bridge.loadLatestMomentQuickCull
          ? await bridge.loadLatestMomentQuickCull(activeBatch.id)
          : null,
      );
      setMomentQuickCullTargets([]);
      setMomentQuickCullNote(
        `Undid Quick Cull for ${undone.group_ids.length} moments and restored ${undone.reviews.length} photos to AI suggestions.`,
      );
      setEditedPreviews({});
      setBackendError(null);
    } catch (error) {
      setBackendError(String(error));
    } finally {
      setMomentQuickCullUndoing(false);
    }
  };

  const setSuggestedReferences = async (
    items: BackendReferenceBatchItem[],
  ) => {
    if (!bridge?.setGroupReferences || !activeBatch || items.length === 0) return;
    setReferenceBatchUpdating(true);
    setReferenceBatchNote(null);
    try {
      const result: BackendReferenceBatchResult = await bridge.setGroupReferences(
        activeBatch.id,
        items,
      );
      setReferenceBindings((current) => ({
        ...current,
        ...Object.fromEntries(
          result.bindings.map((binding) => [binding.group_id, binding]),
        ),
      }));
      setReferenceBatchTargets([]);
      setReferenceBatchNote(
        `Set ${result.bindings.length} suggested References. Review or replace any group individually below.`,
      );
      setEditedPreviews({});
      setBackendError(null);
    } catch (error) {
      setBackendError(String(error));
    } finally {
      setReferenceBatchUpdating(false);
    }
  };

  const setReferencePhoto = async (groupId: string, assetId: string) => {
    if (!bridge?.setGroupReference) return;
    try {
      const binding = await bridge.setGroupReference(groupId, assetId);
      setReferenceBindings((current) => ({ ...current, [groupId]: binding }));
      setReferenceBatchTargets((current) =>
        current.filter((value) => value !== groupId),
      );
      setReferenceBatchNote(null);
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
      setReferenceBatchNote(null);
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

  const copyReferenceStyleToSelected = async (
    sourceGroupId: string,
    targetGroupIds: string[],
  ) => {
    if (!bridge?.copyReferenceStyleToGroups || !sourceGroupId || targetGroupIds.length === 0) {
      return;
    }

    setStyleBatchUpdating(true);
    setStyleBatchNote(null);
    try {
      const result = await bridge.copyReferenceStyleToGroups(
        sourceGroupId,
        targetGroupIds,
      );
      setReferenceStyles((current) => ({
        ...current,
        ...Object.fromEntries(result.styles.map((style) => [style.group_id, style])),
      }));
      setStyleBatchTargets([]);
      setStyleBatchNote(
        `Synced this look to ${result.styles.length} groups; each target kept its own Reference and adaptive baseline.`,
      );
      setEditedPreviews({});
      setBackendError(null);
    } catch (error) {
      setBackendError(String(error));
    } finally {
      setStyleBatchUpdating(false);
    }
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
      const neutral =
        Math.abs(review.exposure_delta_ev) <= 0.0001 &&
        Math.abs(review.contrast_delta) <= 0.0001 &&
        Math.abs(review.saturation_delta) <= 0.0001;
      setRecipeReviews((current) => {
        const next = { ...current };
        if (neutral) delete next[assetId];
        else next[assetId] = review;
        return next;
      });
      if (neutral && reviewSyncSource?.assetId === assetId) {
        setReviewSyncSource(null);
        setReviewSyncTargets([]);
        setReviewSyncNote(null);
      }
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

  const syncRecipeReviewException = async () => {
    if (
      !bridge?.syncRecipeReviewException ||
      !reviewSyncSource ||
      reviewSyncTargets.length === 0 ||
      !Object.values(reviewSyncFields).some(Boolean)
    ) {
      return;
    }

    setReviewSyncUpdating(true);
    setReviewSyncNote(null);
    try {
      const result: BackendRecipeReviewSyncResult = await bridge.syncRecipeReviewException(
        reviewSyncSource.groupId,
        reviewSyncSource.assetId,
        reviewSyncTargets,
        reviewSyncFields,
      );
      const changedAssetIds = new Set(result.overrides.map((review) => review.asset_id));
      setRecipeReviews((current) => {
        const next = { ...current };
        for (const review of result.overrides) {
          const neutral =
            Math.abs(review.exposure_delta_ev) <= 0.0001 &&
            Math.abs(review.contrast_delta) <= 0.0001 &&
            Math.abs(review.saturation_delta) <= 0.0001;
          if (neutral) delete next[review.asset_id];
          else next[review.asset_id] = review;
        }
        return next;
      });
      setReferencePreviews((current) => {
        const preview = current[result.group_id];
        if (!preview || changedAssetIds.size === 0) return current;
        return {
          ...current,
          [result.group_id]: {
            ...preview,
            reviewed_asset_ids: (preview.reviewed_asset_ids ?? []).filter(
              (assetId) => !changedAssetIds.has(assetId),
            ),
          },
        };
      });
      setEditedPreviews((current) => {
        if (changedAssetIds.size === 0) return current;
        const next = { ...current };
        for (const assetId of changedAssetIds) delete next[assetId];
        return next;
      });
      setReviewSyncTargets([]);
      setReviewSyncNote(
        result.overrides.length > 0
          ? `Synced selected exception fields to ${result.overrides.length} photos; changed Recipes returned to Review.`
          : "Selected photos already had the same exception values.",
      );
      setBackendError(null);
    } catch (error) {
      setBackendError(String(error));
    } finally {
      setReviewSyncUpdating(false);
    }
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
      if (reviewSyncSource?.assetId === assetId) {
        setReviewSyncSource(null);
        setReviewSyncTargets([]);
        setReviewSyncNote(null);
      }
      setBackendError(null);
    } catch (error) {
      setBackendError(String(error));
    } finally {
      setReviewUpdating(null);
    }
  };

  const setRecipeReviewed = async (
    groupId: string,
    assetId: string,
    reviewed: boolean,
  ) => {
    if (reviewed ? !bridge?.setRecipeReviewed : !bridge?.clearRecipeReviewed) return;
    setReviewUpdating(assetId);
    try {
      if (reviewed) await bridge!.setRecipeReviewed!(groupId, assetId);
      else await bridge!.clearRecipeReviewed!(assetId);
      setReferencePreviews((current) => {
        const preview = current[groupId];
        if (!preview) return current;
        const reviewedIds = new Set(preview.reviewed_asset_ids ?? []);
        if (reviewed) reviewedIds.add(assetId);
        else reviewedIds.delete(assetId);
        return {
          ...current,
          [groupId]: {
            ...preview,
            reviewed_asset_ids: Array.from(reviewedIds),
          },
        };
      });
      setBackendError(null);
    } catch (error) {
      setBackendError(String(error));
    } finally {
      setReviewUpdating(null);
    }
  };

  const applyConfirmedRecipeAssets = (assetIds: string[]) => {
    const confirmed = new Set(assetIds);
    setReferencePreviews((current) => {
      const next = { ...current };
      for (const [groupId, preview] of Object.entries(current)) {
        const reviewedIds = new Set(preview.reviewed_asset_ids ?? []);
        let changed = false;
        for (const recipe of preview.recipes) {
          const assetId = recipe.target_asset_id;
          if (assetId && confirmed.has(assetId) && !reviewedIds.has(assetId)) {
            reviewedIds.add(assetId);
            changed = true;
          }
        }
        if (changed) {
          next[groupId] = { ...preview, reviewed_asset_ids: Array.from(reviewedIds) };
        }
      }
      return next;
    });
  };

  const confirmVisibleRecipeReviews = async () => {
    if (!bridge?.confirmRecipeReviews || visibleRecipeReviewItems.length === 0) return;
    setReviewBatchUpdating(true);
    setReviewBatchNote(null);
    try {
      const result: BackendRecipeReviewBatchResult =
        await bridge.confirmRecipeReviews(visibleRecipeReviewItems);
      applyConfirmedRecipeAssets(result.asset_ids);
      setReviewBatchNote(`Confirmed ${result.asset_ids.length} visible Recipes.`);
      setBackendError(null);
    } catch (error) {
      setBackendError(String(error));
    } finally {
      setReviewBatchUpdating(false);
    }
  };

  const confirmClearRecipeGroups = async (groupIds: string[]) => {
    if (!bridge?.confirmRecipeReviewGroups || groupIds.length === 0) return;
    setReviewBatchUpdating(true);
    setReviewBatchNote(null);
    try {
      const result: BackendRecipeReviewGroupBatchResult =
        await bridge.confirmRecipeReviewGroups(groupIds);
      applyConfirmedRecipeAssets(result.asset_ids);
      setReviewBatchNote(
        `Confirmed ${result.asset_ids.length} straightforward Recipes across ${result.group_ids.length} clear groups.`,
      );
      setBackendError(null);
    } catch (error) {
      setBackendError(String(error));
    } finally {
      setReviewBatchUpdating(false);
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

  const setHdrMergeCompleted = async (groupId: string, merged: boolean) => {
    if (!bridge?.setHdrMerged) return;
    setHdrMergeUpdating(groupId);
    try {
      const completed = await bridge.setHdrMerged(groupId, merged);
      setHandoffPreflights((current) => {
        const previous = current[groupId];
        if (!previous) return current;
        return {
          ...current,
          [groupId]: {
            ...previous,
            hdr_merge_completed: completed,
            hdr_merge_required:
              previous.hdr_source_asset_ids.length > 0 && !completed,
          },
        };
      });
      setHandoffPreflightRevision((value) => value + 1);
      setBackendError(null);
    } catch (error) {
      setBackendError(String(error));
    } finally {
      setHdrMergeUpdating(null);
    }
  };

  const writeGroupXmp = async (groupId: string) => {
    if (!bridge?.writeGroupXmp) return;
    setHandoffRunning(groupId);
    try {
      const result = await bridge.writeGroupXmp(groupId);
      setHandoffResults((current) => ({ ...current, [groupId]: result }));
      setHandoffPreflights((current) => {
        const previous = current[groupId];
        if (!previous) return current;
        return {
          ...current,
          [groupId]: {
            ...previous,
            current_sidecars: previous.target_sidecars,
            missing_sidecars: [],
            conflicting_sidecars: [],
          },
        };
      });
      setBackendError(null);
    } catch (error) {
      setBackendError(String(error));
    } finally {
      setHandoffRunning(null);
    }
  };

  const writeBatchXmp = async (groupIds: string[]) => {
    if (!bridge?.writeBatchXmp || groupIds.length === 0) return;
    setHandoffBatchRunning(true);
    setHandoffBatchNote(null);
    try {
      const result: BackendLightroomBatchHandoffResult = await bridge.writeBatchXmp(groupIds);
      setHandoffResults((current) => ({
        ...current,
        ...Object.fromEntries(result.groups.map((group) => [group.group_id, group])),
      }));
      setHandoffPreflights((current) => {
        const next = { ...current };
        for (const group of result.groups) {
          const previous = next[group.group_id];
          if (!previous) continue;
          next[group.group_id] = {
            ...previous,
            current_sidecars: previous.target_sidecars,
            missing_sidecars: [],
            conflicting_sidecars: [],
          };
        }
        return next;
      });
      const completed = new Set(result.groups.map((group) => group.group_id));
      setHandoffBatchTargets((current) =>
        current.filter((groupId) => !completed.has(groupId)),
      );
      setHandoffBatchNote(
        `Verified ${result.groups.reduce((total, group) => total + group.verified_sidecar_count, 0)} XMP across ${result.groups.length} selected groups.`,
      );
      setBackendError(null);
    } catch (error) {
      setBackendError(String(error));
    } finally {
      setHandoffBatchRunning(false);
    }
  };

  const workflowNextView =
    workflowStatus && workflowStatus.next_focus !== "COMPLETE"
      ? workflowFocusView(workflowStatus.next_focus)
      : null;
  const workflowNextOrder = workflowStatus
    ? workflowFocusOrder[workflowStatus.next_focus]
    : -1;
  const workflowPanelSteps =
    mode === "workstation"
      ? [
          {
            number: "1",
            order: 0,
            focus: "PREPARE" as WorkflowFocus,
            title: "Import & analyze",
            detail: workflowStatus
              ? `${workflowStatus.facts.preparation_active} preparing · ${workflowStatus.facts.preparation_failed} failed`
              : "Keep RAW untouched",
          },
          {
            number: "2",
            order: 1,
            focus: "CULL" as WorkflowFocus,
            title: "Cull",
            detail: workflowStatus
              ? `${workflowStatus.facts.cull_attention} attention · ${workflowStatus.facts.cull_pending} pending`
              : "Quality + near-duplicate evidence",
          },
          {
            number: "3",
            order: 2,
            focus: null,
            title: "Group",
            detail: workflowStatus
              ? `${workflowStatus.facts.groups_total} effective editing groups`
              : "Moment → semantic similarity",
          },
          {
            number: "4",
            order: 3,
            focus: "REFERENCE" as WorkflowFocus,
            title: "Reference look",
            detail: workflowStatus
              ? `${workflowStatus.facts.reference_attention_groups} groups need attention`
              : "Your preferred photo/style",
          },
          {
            number: "5",
            order: 4,
            focus: null,
            title: "Adaptive recipe",
            detail: workflowStatus
              ? "Canonical Recipe per deliverable photo"
              : "Different correction per photo",
          },
          {
            number: "6",
            order: 5,
            focus: "REVIEW" as WorkflowFocus,
            title: "Review exceptions",
            detail: workflowStatus
              ? `${workflowStatus.facts.review_attention} attention · ${workflowStatus.facts.review_pending_groups} pending groups`
              : "Persist only per-photo corrections",
          },
          {
            number: "7",
            order: 6,
            focus: "LIGHTROOM" as WorkflowFocus,
            title: "Lightroom XMP",
            detail: workflowStatus
              ? `${workflowStatus.facts.lightroom_missing_sidecars} missing · ${workflowStatus.facts.lightroom_conflict_groups} conflicts · ${workflowStatus.facts.lightroom_current_groups} current`
              : "Verified non-destructive handoff",
          },
        ]
      : [
          { number: "1", order: 0, focus: null, title: "Library", detail: "Synced project context" },
          { number: "2", order: 1, focus: null, title: "Cull", detail: "Keep / Review / Reject" },
          { number: "3", order: 2, focus: null, title: "Groups", detail: "Use effective project groups" },
          { number: "4", order: 3, focus: null, title: "Reference", detail: "Choose the preferred photo" },
        ];

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
              {mode === "workstation" && workflowStatus
                ? workflowFocusLabel(workflowStatus.next_focus)
                : mode === "workstation"
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
        {mode === "workstation" &&
          bridge?.confirmMomentQuickCull &&
          (momentQuickCullEligibleGroups.length > 0 || latestMomentQuickCull) && (
            <div className="reference-batch-setup">
              <div className="reference-batch-setup-head">
                <div>
                  <span>Moment quick cull</span>
                  <strong>Best frame → Keep · uncertain alternates → Review · only clear non-people duplicates → Reject</strong>
                </div>
                <small>
                  Uses the existing group-relative Cull evidence only. People/family alternates are never batch-Rejected. Landscape and other scene photos can be auto-Rejected only when scene evidence agrees, embedding similarity is at least 98.5%, and the primary has a material quality lead. HDR brackets and analysis-pending groups remain individual work.
                </small>
              </div>
              <div className="reference-batch-setup-status">
                <span>{momentQuickCullEligibleGroups.length} eligible moments</span>
                <span>{momentQuickCullPeopleGroupCount} people-protected</span>
                <span>{momentQuickCullLandscapeGroupCount} landscape moments</span>
                {momentQuickCullConservativeGroupCount > 0 && (
                  <span>{momentQuickCullConservativeGroupCount} conservative · alternates stay Review</span>
                )}
                <span>{selectedMomentQuickCullGroups.length} selected</span>
                <span>
                  {selectedMomentQuickCullPhotoCount} photos · {selectedMomentQuickCullCounts.keep} Keep / {selectedMomentQuickCullCounts.review} Review / {selectedMomentQuickCullCounts.reject} Reject
                </span>
                {latestMomentQuickCull && (
                  <span>
                    Undo available · {latestMomentQuickCull.group_ids.length} moments / {latestMomentQuickCull.reviews.length} photos
                  </span>
                )}
              </div>
              <div className="batch-look-actions reference-batch-actions">
                <button
                  className="review-choice clear"
                  disabled={momentQuickCullUpdating || momentQuickCullEligibleGroups.length === 0}
                  onClick={() =>
                    setMomentQuickCullTargets(
                      momentQuickCullEligibleGroups.map((group) => group.group_id),
                    )
                  }
                >
                  Select eligible
                </button>
                <button
                  className="review-choice clear"
                  disabled={momentQuickCullUpdating || momentQuickCullTargets.length === 0}
                  onClick={() => setMomentQuickCullTargets([])}
                >
                  Clear
                </button>
                <button
                  className="button primary"
                  disabled={
                    momentQuickCullUpdating ||
                    momentQuickCullUndoing ||
                    selectedMomentQuickCullGroups.length === 0
                  }
                  onClick={() => void confirmSelectedMomentQuickCull()}
                >
                  {momentQuickCullUpdating
                    ? "Quick-culling selected moments…"
                    : `Apply to ${selectedMomentQuickCullGroups.length} moments`}
                </button>
                {latestMomentQuickCull && bridge?.undoMomentQuickCull && (
                  <button
                    className="review-choice clear"
                    disabled={momentQuickCullUpdating || momentQuickCullUndoing}
                    onClick={() => void undoLatestMomentQuickCull()}
                    title="Undo is transactional and is blocked if any Quick Cull decision, group membership, or downstream Reference changed."
                  >
                    {momentQuickCullUndoing ? "Undoing…" : "Undo last Quick Cull"}
                  </button>
                )}
              </div>
              {momentQuickCullNote && (
                <small className="success-text">{momentQuickCullNote}</small>
              )}
            </div>
          )}
        {momentQuickCullNote && momentQuickCullEligibleGroups.length === 0 && !latestMomentQuickCull && (
          <div className="group-refine-note">{momentQuickCullNote}</div>
        )}
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
                  {!!group.exposure_brackets?.length &&
                    ` · ${group.exposure_brackets.length} exposure bracket set(s)`}
                  {group.moment_quick_cull &&
                    ` · quick plan: 1 Keep / ${group.moment_quick_cull.review_asset_ids.length} Review / ${group.moment_quick_cull.reject_asset_ids.length} Reject`}
                </span>
                {eligibleMomentQuickCullGroupIds.has(group.group_id) && (
                  <label className="handoff-batch-check">
                    <input
                      type="checkbox"
                      checked={momentQuickCullTargets.includes(group.group_id)}
                      disabled={momentQuickCullUpdating}
                      onChange={() =>
                        setMomentQuickCullTargets((current) =>
                          current.includes(group.group_id)
                            ? current.filter((value) => value !== group.group_id)
                            : [...current, group.group_id],
                        )
                      }
                    />
                    <span>
                      Quick-cull moment
                      {group.moment_quick_cull?.contains_people
                        ? " · people alternates stay Review"
                        : !group.moment_quick_cull?.scene_evidence_complete
                          ? " · scene evidence incomplete · alternates stay Review"
                          : !group.moment_quick_cull?.scene_consistent
                            ? " · mixed/unknown scene · alternates stay Review"
                            : group.moment_quick_cull?.shared_scene_tags.includes("LANDSCAPE")
                              ? " · landscape guard · only ≥98.5% near duplicates can Reject"
                              : ` · scene guard: ${(group.moment_quick_cull?.shared_scene_tags ?? []).map(sceneTagLabel).join(", ")}`}
                    </span>
                  </label>
                )}
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
                        {item.duplicate_similarity != null && (
                          <small>
                            Near-match evidence {Math.round(item.duplicate_similarity * 1000) / 10}%
                          </small>
                        )}
                        {!!item.scene_tags?.length && (
                          <small>
                            Scene {item.scene_tags.map(sceneTagLabel).join(" · ")}
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

  const renderGroups = () => {
    const groups = photoContext?.groups ?? [];
    const groupingLockedByReference = Object.keys(referenceBindings).length > 0;
    const selectedGroupIndexes = groupMergeSelection
      .map((groupId) => groups.findIndex((group) => group.id === groupId))
      .filter((index) => index >= 0)
      .sort((left, right) => left - right);
    const mergeSelectionAdjacent =
      selectedGroupIndexes.length >= 2 &&
      selectedGroupIndexes.every(
        (index, position) =>
          position === 0 || index === selectedGroupIndexes[position - 1] + 1,
      ) &&
      groupMergeSelection.every(
        (groupId) =>
          groups.find((group) => group.id === groupId)?.basis !==
          "SEMANTIC_SIMILARITY",
      );

    const toggleMergeGroup = (groupId: string) => {
      setGroupMergeSelection((current) =>
        current.includes(groupId)
          ? current.filter((value) => value !== groupId)
          : [...current, groupId],
      );
    };

    return (
      <section className="queue-card">
        <div className="queue-title">
          <strong>Photo Groups</strong>
          <div className="group-refine-actions">
            <span>
              Automatic grouping handles the common case; manual corrections lock only the exceptions.
            </span>
            {mode === "workstation" && bridge?.refineGroups && activeBatch && (
              <button
                className="button secondary"
                disabled={
                  groupRefining ||
                  groupMutationRunning ||
                  groupingLockedByReference
                }
                onClick={() => void refineGroups()}
              >
                {groupRefining ? "Refining…" : "Refine semantic groups"}
              </button>
            )}
          </div>
        </div>

        {groupingLockedByReference && (
          <div className="group-refine-note">
            Grouping is locked because Reference selection has started. Clear References before merge, split or semantic regrouping.
          </div>
        )}
        {groupRefinementNote && (
          <div className="group-refine-note">{groupRefinementNote}</div>
        )}

        {mode === "workstation" && bridge?.mergeGroups && groups.length > 1 && (
          <div className="group-manual-toolbar">
            <div>
              <strong>Manual correction</strong>
              <small>
                Select adjacent non-semantic groups to merge. Semantic children can first be restored to their original moment.
              </small>
            </div>
            <button
              className="button secondary"
              disabled={
                groupingLockedByReference ||
                groupMutationRunning ||
                !mergeSelectionAdjacent
              }
              onClick={mergeSelectedGroups}
            >
              {groupMutationRunning
                ? "Updating groups…"
                : `Merge selected (${groupMergeSelection.length})`}
            </button>
          </div>
        )}

        <div className="group-detail-grid group-correction-grid">
          {groups.map((group, index) => {
            const semantic = group.basis === "SEMANTIC_SIMILARITY";
            const brackets = bracketSetsForGroup(group.id);
            const canMerge = !semantic;
            const splitBeforeAssetId =
              groupSplitPoints[group.id] ?? group.asset_ids[1] ?? "";
            return (
              <div className="group-detail-card group-correction-card" key={group.id}>
                <div className="group-correction-head">
                  <div>
                    <span>Group {index + 1}</span>
                    <strong>{group.asset_ids.length} photos</strong>
                    <small>
                      {group.kind.toLowerCase()} · {group.basis.replaceAll("_", " ").toLowerCase()}
                    </small>
                    {brackets.length > 0 && (
                      <small>
                        Exposure bracket · {brackets
                          .map((set) => `${set.members.length} frames / ${set.span_ev.toFixed(1)} EV`)
                          .join(" · ")}
                      </small>
                    )}
                  </div>
                  {mode === "workstation" && bridge?.mergeGroups && canMerge && (
                    <label className="group-merge-check">
                      <input
                        type="checkbox"
                        checked={groupMergeSelection.includes(group.id)}
                        disabled={groupMutationRunning || groupingLockedByReference}
                        onChange={() => toggleMergeGroup(group.id)}
                      />
                      <span>Merge</span>
                    </label>
                  )}
                </div>

                <div className="group-preview-strip">
                  {group.asset_ids.slice(0, 6).map((assetId) =>
                    previewUrls.get(assetId) ? (
                      <img
                        key={assetId}
                        src={previewUrls.get(assetId)}
                        alt={assetNames.get(assetId) ?? "RAW preview"}
                        loading="lazy"
                      />
                    ) : (
                      <div className="group-preview-placeholder" key={assetId}>RAW</div>
                    ),
                  )}
                  {group.asset_ids.length > 6 && (
                    <div className="group-preview-more">+{group.asset_ids.length - 6}</div>
                  )}
                </div>

                <div className="group-correction-actions">
                  {semantic && bridge?.keepMomentTogether && (
                    <button
                      className="review-choice clear"
                      disabled={groupMutationRunning || groupingLockedByReference}
                      onClick={() => keepMomentTogether(group.id)}
                    >
                      Keep original moment together
                    </button>
                  )}

                  {!semantic &&
                    group.manual_locked &&
                    group.kind !== "MANUAL" &&
                    bridge?.allowGroupRefinement && (
                      <button
                        className="review-choice clear"
                        disabled={groupMutationRunning || groupingLockedByReference}
                        onClick={() => allowGroupRefinement(group.id)}
                      >
                        Allow semantic refine again
                      </button>
                    )}

                  {!semantic && group.kind === "MANUAL" && (
                    <em>Manual correction locked</em>
                  )}

                  {!semantic &&
                    group.asset_ids.length > 1 &&
                    bridge?.splitGroup && (
                      <div className="group-split-control">
                        <select
                          value={splitBeforeAssetId}
                          disabled={groupMutationRunning || groupingLockedByReference}
                          onChange={(event) =>
                            setGroupSplitPoints((current) => ({
                              ...current,
                              [group.id]: event.target.value,
                            }))
                          }
                        >
                          {group.asset_ids.slice(1).map((assetId) => (
                            <option key={assetId} value={assetId}>
                              Split before {assetNames.get(assetId) ?? assetId.slice(0, 8)}
                            </option>
                          ))}
                        </select>
                        <button
                          className="review-choice keep"
                          disabled={
                            groupMutationRunning ||
                            groupingLockedByReference ||
                            !splitBeforeAssetId
                          }
                          onClick={() =>
                            splitGroupAtSelectedPhoto(
                              group.id,
                              group.asset_ids[1] ?? "",
                            )
                          }
                        >
                          Split
                        </button>
                      </div>
                    )}
                </div>
              </div>
            );
          })}
          {!groups.length && <div className="panel-note">No photo groups yet.</div>}
        </div>
      </section>
    );
  };

  const renderReference = () => {
    const groups = photoContext?.groups ?? [];
    const missingReferenceGroups = groups.filter(
      (group) => referenceBindings[group.id] == null,
    );
    const hdrReferenceGroups = missingReferenceGroups.filter(
      (group) => bracketSetsForGroup(group.id).length > 0,
    );
    const standardReferenceGroups = missingReferenceGroups.filter(
      (group) => bracketSetsForGroup(group.id).length === 0,
    );
    const eligibleReferenceItems: BackendReferenceBatchItem[] =
      standardReferenceGroups.flatMap((group) => {
        const assetId = batchReferenceCandidateForGroup(group.id, group.asset_ids);
        return assetId ? [{ group_id: group.id, asset_id: assetId }] : [];
      });
    const eligibleReferenceGroupIds = new Set(
      eligibleReferenceItems.map((item) => item.group_id),
    );
    const selectedReferenceItems = eligibleReferenceItems.filter((item) =>
      referenceBatchTargets.includes(item.group_id),
    );
    const blockedReferenceGroupCount =
      standardReferenceGroups.length - eligibleReferenceItems.length;

    const toggleReferenceBatchTarget = (groupId: string) => {
      setReferenceBatchTargets((current) =>
        current.includes(groupId)
          ? current.filter((value) => value !== groupId)
          : [...current, groupId],
      );
    };

    const lookGroups = (photoContext?.groups ?? []).filter(
      (group) =>
        referenceBindings[group.id] != null &&
        referenceStyles[group.id] != null,
    );
    const selectedBatchSource =
      lookGroups.some((group) => group.id === styleBatchSource)
        ? styleBatchSource
        : lookGroups[0]?.id ?? "";
    const sourceProfile =
      selectedBatchSource
        ? referenceStyles[selectedBatchSource]?.style_profile
        : undefined;
    const eligibleLookTargets = lookGroups.filter(
      (group) =>
        group.id !== selectedBatchSource &&
        !sameVisualStyle(
          sourceProfile,
          referenceStyles[group.id]?.style_profile,
        ),
    );
    const eligibleTargetIds = new Set(
      eligibleLookTargets.map((group) => group.id),
    );
    const selectedLookTargets = styleBatchTargets.filter((groupId) =>
      eligibleTargetIds.has(groupId),
    );
    const alreadyMatchingCount = lookGroups.filter(
      (group) =>
        group.id !== selectedBatchSource &&
        sameVisualStyle(
          sourceProfile,
          referenceStyles[group.id]?.style_profile,
        ),
    ).length;

    const toggleLookTarget = (groupId: string) => {
      setStyleBatchTargets((current) =>
        current.includes(groupId)
          ? current.filter((value) => value !== groupId)
          : [...current, groupId],
      );
    };

    return (
    <section className="queue-card">
      <div className="queue-title">
        <strong>Reference look</strong>
        <span>Cull decisions lead the shortlist; measured technical quality and group rank break ties. Selection stays explicit and saved.</span>
      </div>

      {mode === "workstation" &&
        bridge?.setGroupReferences &&
        missingReferenceGroups.length > 0 && (
          <div className="reference-batch-setup">
            <div className="reference-batch-setup-head">
              <div>
                <span>Batch Reference setup</span>
                <strong>Suggested candidate → selected groups</strong>
              </div>
              <small>
                Uses the existing per-group shortlist only. Exposure-bracket groups are intentionally excluded from batch Reference setup because their RAWs must preserve capture exposure for HDR merge. AI Reject suggestions and evidence-pending candidates remain individual-review work.
              </small>
            </div>
            <div className="reference-batch-setup-status">
              <span>{missingReferenceGroups.length} missing</span>
              <span>{eligibleReferenceItems.length} eligible</span>
              <span>{blockedReferenceGroupCount} need Cull review</span>
              {hdrReferenceGroups.length > 0 && (
                <span>{hdrReferenceGroups.length} HDR merge first</span>
              )}
            </div>
            <div className="batch-look-actions reference-batch-actions">
              <button
                className="review-choice clear"
                disabled={referenceBatchUpdating || eligibleReferenceItems.length === 0}
                onClick={() =>
                  setReferenceBatchTargets(
                    eligibleReferenceItems.map((item) => item.group_id),
                  )
                }
              >
                Select eligible
              </button>
              <button
                className="review-choice clear"
                disabled={referenceBatchUpdating || selectedReferenceItems.length === 0}
                onClick={() => setReferenceBatchTargets([])}
              >
                Clear
              </button>
              <button
                className="button primary"
                disabled={
                  referenceBatchUpdating ||
                  selectedReferenceItems.length === 0
                }
                onClick={() => void setSuggestedReferences(selectedReferenceItems)}
              >
                {referenceBatchUpdating
                  ? "Setting References…"
                  : `Set suggested References (${selectedReferenceItems.length})`}
              </button>
            </div>
            {referenceBatchNote && (
              <small className="success-text">{referenceBatchNote}</small>
            )}
          </div>
        )}

      {mode === "workstation" &&
        lookGroups.length > 1 &&
        bridge?.copyReferenceStyleToGroups && (
          <div className="reference-batch-look">
            <div className="reference-batch-look-head">
              <div>
                <span>Batch look sync</span>
                <strong>Reference → selected groups → exception review</strong>
              </div>
              <small>
                Same-look groups are skipped automatically. Targets keep their own Reference photos and scene-adaptive exposure baselines.
              </small>
            </div>
            <div className="reference-batch-controls">
              <label className="batch-look-source">
                <span>Source look</span>
                <select
                  value={selectedBatchSource}
                  disabled={styleBatchUpdating || styleUpdating != null}
                  onChange={(event) => {
                    setStyleBatchSource(event.target.value);
                    setStyleBatchTargets([]);
                    setStyleBatchNote(null);
                  }}
                >
                  {lookGroups.map((group) => (
                    <option key={group.id} value={group.id}>
                      Group {(photoContext?.groups ?? []).findIndex((item) => item.id === group.id) + 1}
                    </option>
                  ))}
                </select>
              </label>

              <div className="batch-look-targets">
                <div className="batch-look-target-head">
                  <span>Sync to selected</span>
                  <small>
                    {eligibleLookTargets.length} eligible
                    {alreadyMatchingCount ? ` · ${alreadyMatchingCount} already matching` : ""}
                  </small>
                </div>
                <div className="batch-look-target-grid">
                  {eligibleLookTargets.map((group) => (
                    <label key={group.id}>
                      <input
                        type="checkbox"
                        checked={selectedLookTargets.includes(group.id)}
                        disabled={styleBatchUpdating || styleUpdating != null}
                        onChange={() => toggleLookTarget(group.id)}
                      />
                      <span>
                        Group {(photoContext?.groups ?? []).findIndex((item) => item.id === group.id) + 1}
                        <small>{group.asset_ids.length} photos</small>
                      </span>
                    </label>
                  ))}
                  {eligibleLookTargets.length === 0 && (
                    <small className="success-text">
                      Every other referenced group already uses this visual look.
                    </small>
                  )}
                </div>
              </div>

              <div className="batch-look-actions">
                <button
                  className="review-choice clear"
                  disabled={styleBatchUpdating || eligibleLookTargets.length === 0}
                  onClick={() =>
                    setStyleBatchTargets(eligibleLookTargets.map((group) => group.id))
                  }
                >
                  Select eligible
                </button>
                <button
                  className="review-choice clear"
                  disabled={styleBatchUpdating || selectedLookTargets.length === 0}
                  onClick={() => setStyleBatchTargets([])}
                >
                  Clear
                </button>
                <button
                  className="button primary"
                  disabled={
                    styleBatchUpdating ||
                    styleUpdating != null ||
                    !selectedBatchSource ||
                    selectedLookTargets.length === 0
                  }
                  onClick={() =>
                    void copyReferenceStyleToSelected(
                      selectedBatchSource,
                      selectedLookTargets,
                    )
                  }
                >
                  {styleBatchUpdating
                    ? "Syncing look…"
                    : `Sync look to selected (${selectedLookTargets.length})`}
                </button>
              </div>
            </div>
            {styleBatchNote && <small className="success-text">{styleBatchNote}</small>}
          </div>
        )}

      <div className="reference-groups">
        {photoContext?.groups.map((group, index) => {
          const binding = referenceBindings[group.id];
          const style = referenceStyles[group.id]?.style_profile;
          const preview = referencePreviews[group.id];
          const candidates = referenceCandidatesForGroup(group.id, group.asset_ids);
          const recommendedReferenceAssetId =
            candidates.find((assetId) => {
              const user = cullingReviews[assetId];
              if (user === "KEEP" || user === "REVIEW") return true;
              const recommendation = cullingRecommendations.get(assetId);
              return recommendation?.decision !== "REJECT_SUGGESTION";
            }) ?? candidates[0];
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
                  {!binding && eligibleReferenceGroupIds.has(group.id) && (
                    <label className="reference-batch-check">
                      <input
                        type="checkbox"
                        checked={referenceBatchTargets.includes(group.id)}
                        disabled={referenceBatchUpdating}
                        onChange={() => toggleReferenceBatchTarget(group.id)}
                      />
                      <span>Batch suggested Reference</span>
                    </label>
                  )}
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
                      disabled={referenceBatchUpdating}
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
                        disabled={styleUpdating != null || styleBatchUpdating}
                        onClick={() => adjustReferenceStyle(group.id, "exposure", -0.1)}
                      >−</button>
                      <strong>{signed(style?.exposure_bias_ev ?? 0)} EV</strong>
                      <button
                        className="style-step"
                        disabled={styleUpdating != null || styleBatchUpdating}
                        onClick={() => adjustReferenceStyle(group.id, "exposure", 0.1)}
                      >+</button>
                    </div>
                  </div>
                  <div className="style-control">
                    <span>Contrast</span>
                    <div>
                      <button
                        className="style-step"
                        disabled={styleUpdating != null || styleBatchUpdating}
                        onClick={() => adjustReferenceStyle(group.id, "contrast", -5)}
                      >−</button>
                      <strong>{signed(style?.contrast_preference ?? 0, 0)}</strong>
                      <button
                        className="style-step"
                        disabled={styleUpdating != null || styleBatchUpdating}
                        onClick={() => adjustReferenceStyle(group.id, "contrast", 5)}
                      >+</button>
                    </div>
                  </div>
                  <div className="style-control">
                    <span>Saturation</span>
                    <div>
                      <button
                        className="style-step"
                        disabled={styleUpdating != null || styleBatchUpdating}
                        onClick={() => adjustReferenceStyle(group.id, "saturation", -5)}
                      >−</button>
                      <strong>{signed(style?.saturation_preference ?? 0, 0)}</strong>
                      <button
                        className="style-step"
                        disabled={styleUpdating != null || styleBatchUpdating}
                        onClick={() => adjustReferenceStyle(group.id, "saturation", 5)}
                      >+</button>
                    </div>
                  </div>
                  <button
                    className="review-choice clear"
                    disabled={styleUpdating != null || styleBatchUpdating}
                    onClick={() => resetReferenceStyle(group.id)}
                  >
                    Reset style
                  </button>
                  <small>
                    Shared look preferences stay separate from this group's Reference and adaptive baseline. Batch look sync above reuses the same StyleProfile path; white balance controls remain locked until reliable RAW/metadata WB evidence exists.
                  </small>
                </div>
              )}

              <div className="reference-candidates">
                {candidates.slice(0, 10).map((assetId) => {
                  const user = cullingReviews[assetId];
                  const recommendation = cullingRecommendations.get(assetId);
                  const selected = binding?.selected_reference_asset_id === assetId;
                  const recommended = recommendedReferenceAssetId === assetId;
                  const evidence =
                    user != null
                      ? `Your ${userDecisionLabel(user)}`
                      : recommendation
                        ? `AI ${cullingLabel(recommendation.decision)} · #${recommendation.group_rank}`
                        : "Pending evidence";
                  const technicalEvidence = recommendation
                    ? `Technical ${Math.round(recommendation.quality_score * 100)}/100`
                    : null;
                  const reasonEvidence = recommendation?.reasons
                    ?.slice(0, 2)
                    .map(cullingReasonLabel)
                    .join(" · ");
                  const portraitEvidence = recommendation?.portrait_evidence;
                  const peopleEvidence =
                    portraitEvidence && (portraitEvidence.person_count > 0 || portraitEvidence.face_count > 0)
                      ? `${portraitEvidence.person_count} people · ${portraitEvidence.face_count} faces`
                      : null;
                  return (
                    <button
                      className={`reference-candidate ${selected ? "selected" : ""} ${recommended ? "recommended" : ""}`}
                      key={assetId}
                      disabled={!bridge?.setGroupReference || referenceBatchUpdating}
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
                      <div className="reference-candidate-badges">
                        {recommended && <em>Best starting point</em>}
                        {!binding &&
                          bracketSetsForGroup(group.id).length === 0 &&
                          batchReferenceCandidateForGroup(group.id, group.asset_ids) === assetId && (
                            <em>Batch eligible</em>
                          )}
                        {selected && <em>Selected</em>}
                      </div>
                      <span>{assetNames.get(assetId) ?? assetId.slice(0, 8)}</span>
                      <small>{evidence}</small>
                      {technicalEvidence && (
                        <small className="reference-candidate-metric">
                          {technicalEvidence}{reasonEvidence ? ` · ${reasonEvidence}` : ""}
                        </small>
                      )}
                      {peopleEvidence && (
                        <small className="reference-candidate-context">
                          {peopleEvidence} · context only
                        </small>
                      )}
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
  };

  const renderReview = () => (
    <section className="queue-card">
      <div className="queue-title">
        <strong>Recipe review</strong>
        <span>Group style stays shared · only photo-specific exceptions are stored here</span>
      </div>
      <div className="review-summary">
        <div><span>Attention</span><strong>{recipeReviewSummary.attention}</strong></div>
        <div><span>Confirmed</span><strong>{recipeReviewSummary.confirmed}</strong></div>
        <div><span>Exceptions</span><strong>{recipeReviewSummary.exceptions}</strong></div>
        <div><span>Reject skipped</span><strong>{recipeReviewSummary.rejected}</strong></div>
      </div>
      {culling.some((group) => (group.exposure_brackets?.length ?? 0) > 0) && (
        <div className="group-refine-note">
          Exposure-bracket RAWs are excluded from ordinary Adaptive Recipe review so their intentional EV differences remain intact for HDR merge in Lightroom/Camera Raw.
        </div>
      )}
      <div className="cull-toolbar">
        <span>
          {reviewViewMode === "TRIAGE"
            ? "Triage reuses Cull decisions and saved per-photo exceptions so attention goes to uncertain photos first."
            : "All shows every adaptive Recipe, including photos already considered straightforward."}
        </span>
        <div className="cull-toolbar-actions">
          <button
            className="cull-batch-action"
            disabled={
              reviewBatchUpdating ||
              reviewSyncSource != null ||
              reviewUpdating != null ||
              !bridge?.confirmRecipeReviewGroups ||
              clearRecipeReviewGroups.length === 0
            }
            onClick={() =>
              void confirmClearRecipeGroups(
                clearRecipeReviewGroups.map((group) => group.group_id),
              )
            }
          >
            {reviewBatchUpdating
              ? "Confirming…"
              : `Confirm clear groups (${clearRecipeReviewGroups.length} · ${clearRecipeReviewAssetCount} photos)`}
          </button>
          <button
            className="cull-batch-action"
            disabled={
              reviewBatchUpdating ||
              reviewSyncSource != null ||
              reviewUpdating != null ||
              !bridge?.confirmRecipeReviews ||
              visibleRecipeReviewItems.length === 0
            }
            onClick={() => void confirmVisibleRecipeReviews()}
          >
            {reviewBatchUpdating
              ? "Confirming…"
              : `Confirm visible (${visibleRecipeReviewItems.length})`}
          </button>
          <div className="cull-view-switch" role="group" aria-label="Recipe review view mode">
            {(["TRIAGE", "ALL"] as ReviewViewMode[]).map((viewMode) => (
              <button
                className={reviewViewMode === viewMode ? "active" : ""}
                key={viewMode}
                disabled={reviewBatchUpdating}
                onClick={() => setReviewViewMode(viewMode)}
              >
                {viewMode === "TRIAGE" ? "Triage" : "All"}
              </button>
            ))}
          </div>
        </div>
      </div>
      {reviewBatchNote && <div className="review-batch-note">{reviewBatchNote}</div>}
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

          const recipesWithTargets = preview.recipes.filter(
            (recipe) => recipe.target_asset_id != null,
          );
          const syncingThisGroup = reviewSyncSource?.groupId === group.id;
          const reviewRecipes =
            reviewViewMode === "ALL" || syncingThisGroup
              ? recipesWithTargets
              : recipesWithTargets
                  .filter((recipe) => recipeReviewPriority(recipe.target_asset_id!) < 10)
                  .slice()
                  .sort((left, right) => {
                    const leftId = left.target_asset_id!;
                    const rightId = right.target_asset_id!;
                    const byPriority =
                      recipeReviewPriority(leftId) - recipeReviewPriority(rightId);
                    if (byPriority !== 0) return byPriority;

                    const byQuality =
                      (cullingRecommendations.get(leftId)?.quality_score ?? 1) -
                      (cullingRecommendations.get(rightId)?.quality_score ?? 1);
                    if (Math.abs(byQuality) > 0.0001) return byQuality;

                    return (
                      (cullingRecommendations.get(leftId)?.group_rank ??
                        Number.MAX_SAFE_INTEGER) -
                      (cullingRecommendations.get(rightId)?.group_rank ??
                        Number.MAX_SAFE_INTEGER)
                    );
                  });
          const groupAttentionCount = recipesWithTargets.filter(
            (recipe) =>
              recipe.target_asset_id != null &&
              recipeReviewPriority(recipe.target_asset_id) < 10,
          ).length;
          const groupConfirmedCount = recipesWithTargets.filter(
            (recipe) =>
              recipe.target_asset_id != null &&
              reviewedRecipeAssetIds.has(recipe.target_asset_id),
          ).length;
          const groupExceptionCount = recipesWithTargets.filter(
            (recipe) =>
              recipe.target_asset_id != null &&
              recipeReviews[recipe.target_asset_id] != null,
          ).length;
          const clearGroup = clearRecipeReviewGroups.find(
            (value) => value.group_id === group.id,
          );

          return (
            <div className="recipe-review-group" key={group.id}>
              <div className="recipe-review-head">
                <strong>Group {groupIndex + 1}</strong>
                <div className="recipe-review-head-actions">
                  <span>
                    {`${groupAttentionCount} attention · ${groupConfirmedCount} confirmed · ${groupExceptionCount} exceptions · ${recipesWithTargets.length} total`}
                  </span>
                  {clearGroup && (
                    <button
                      className="cull-batch-action"
                      disabled={
                        reviewBatchUpdating ||
                        reviewUpdating != null ||
                        !bridge?.confirmRecipeReviewGroups
                      }
                      onClick={() => void confirmClearRecipeGroups([group.id])}
                    >
                      Confirm clear group ({clearGroup.asset_ids.length})
                    </button>
                  )}
                </div>
              </div>
              {syncingThisGroup && reviewSyncSource && (
                <div className="recipe-sync-panel">
                  <div>
                    <strong>
                      Exception source: {assetNames.get(reviewSyncSource.assetId) ?? reviewSyncSource.assetId.slice(0, 8)}
                    </strong>
                    <small>
                      Copy only the selected per-photo delta fields. Adaptive Recipe baselines and group style stay untouched.
                    </small>
                  </div>
                  <div className="recipe-sync-fields" role="group" aria-label="Exception fields to sync">
                    {([
                      ["exposure", "Exposure"],
                      ["contrast", "Contrast"],
                      ["saturation", "Saturation"],
                    ] as const).map(([field, label]) => (
                      <label key={field}>
                        <input
                          type="checkbox"
                          checked={reviewSyncFields[field]}
                          disabled={reviewSyncUpdating}
                          onChange={() =>
                            setReviewSyncFields((current) => ({
                              ...current,
                              [field]: !current[field],
                            }))
                          }
                        />
                        <span>{label}</span>
                      </label>
                    ))}
                  </div>
                  <div className="recipe-sync-actions">
                    <button
                      className="cull-batch-action"
                      disabled={reviewSyncUpdating || recipesWithTargets.length <= 1}
                      onClick={() =>
                        setReviewSyncTargets(
                          recipesWithTargets
                            .map((recipe) => recipe.target_asset_id!)
                            .filter((assetId) => assetId !== reviewSyncSource.assetId),
                        )
                      }
                    >
                      Select group peers ({Math.max(0, recipesWithTargets.length - 1)})
                    </button>
                    <button
                      className="cull-batch-action"
                      disabled={reviewSyncUpdating || reviewSyncTargets.length === 0}
                      onClick={() => setReviewSyncTargets([])}
                    >
                      Clear targets
                    </button>
                    <button
                      className="button primary"
                      disabled={
                        reviewSyncUpdating ||
                        reviewSyncTargets.length === 0 ||
                        !Object.values(reviewSyncFields).some(Boolean) ||
                        !bridge?.syncRecipeReviewException
                      }
                      onClick={() => void syncRecipeReviewException()}
                    >
                      {reviewSyncUpdating
                        ? "Syncing exception…"
                        : `Apply to selected (${reviewSyncTargets.length})`}
                    </button>
                    <button
                      className="cull-batch-action"
                      disabled={reviewSyncUpdating}
                      onClick={() => {
                        setReviewSyncSource(null);
                        setReviewSyncTargets([]);
                        setReviewSyncNote(null);
                      }}
                    >
                      Done
                    </button>
                  </div>
                  {reviewSyncNote && <small className="success-text">{reviewSyncNote}</small>}
                </div>
              )}
              {reviewViewMode === "TRIAGE" && !syncingThisGroup && reviewRecipes.length === 0 && (
                <div className="panel-note">
                  Recipe triage is clear for this group. Switch to All for a full visual pass.
                </div>
              )}
              <div className="recipe-review-grid">
                {reviewRecipes.map((recipe) => {
                  const assetId = recipe.target_asset_id;
                  if (!assetId) return null;
                  const review = recipeReviews[assetId];
                  const reviewed = reviewedRecipeAssetIds.has(assetId);
                  const updating = reviewUpdating === assetId;
                  const isSyncSource =
                    syncingThisGroup && reviewSyncSource?.assetId === assetId;
                  const isSyncTarget =
                    syncingThisGroup && reviewSyncTargets.includes(assetId);
                  return (
                    <article
                      className={`recipe-review-card ${isSyncSource ? "sync-source" : ""} ${isSyncTarget ? "sync-target" : ""}`}
                      key={recipe.id}
                    >
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
                              disabled={updating || reviewBatchUpdating || reviewSyncUpdating || !bridge?.renderRecipePreview}
                              onClick={() => void renderEditedPreview(group.id, assetId)}
                            >
                              {updating ? "Rendering…" : "Render edited preview"}
                            </button>
                          )}
                        </div>
                      </div>
                      <div className="recipe-review-info">
                        <strong>{assetNames.get(assetId) ?? assetId.slice(0, 8)}</strong>
                        <small>{recipeReviewLabel(assetId)}</small>
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
                          <button disabled={updating || reviewBatchUpdating || reviewSyncUpdating} onClick={() => adjustRecipeReview(assetId, "exposure", -0.1)}>−</button>
                          <strong>{signed(review?.exposure_delta_ev ?? 0)} EV</strong>
                          <button disabled={updating || reviewBatchUpdating || reviewSyncUpdating} onClick={() => adjustRecipeReview(assetId, "exposure", 0.1)}>+</button>
                        </div>
                        <div className="mini-adjust">
                          <span>Contrast</span>
                          <button disabled={updating || reviewBatchUpdating || reviewSyncUpdating} onClick={() => adjustRecipeReview(assetId, "contrast", -5)}>−</button>
                          <strong>{signed(review?.contrast_delta ?? 0, 0)}</strong>
                          <button disabled={updating || reviewBatchUpdating || reviewSyncUpdating} onClick={() => adjustRecipeReview(assetId, "contrast", 5)}>+</button>
                        </div>
                        <div className="mini-adjust">
                          <span>Saturation</span>
                          <button disabled={updating || reviewBatchUpdating || reviewSyncUpdating} onClick={() => adjustRecipeReview(assetId, "saturation", -5)}>−</button>
                          <strong>{signed(review?.saturation_delta ?? 0, 0)}</strong>
                          <button disabled={updating || reviewBatchUpdating || reviewSyncUpdating} onClick={() => adjustRecipeReview(assetId, "saturation", 5)}>+</button>
                        </div>
                      </div>
                      <div className="recipe-review-actions">
                        {syncingThisGroup && !isSyncSource && (
                          <label className="recipe-sync-target">
                            <input
                              type="checkbox"
                              checked={isSyncTarget}
                              disabled={reviewSyncUpdating}
                              onChange={() =>
                                setReviewSyncTargets((current) =>
                                  current.includes(assetId)
                                    ? current.filter((value) => value !== assetId)
                                    : [...current, assetId],
                                )
                              }
                            />
                            <span>Sync target</span>
                          </label>
                        )}
                        <button
                          className={`review-choice ${reviewed ? "clear" : "keep"}`}
                          disabled={updating || reviewBatchUpdating || reviewSyncUpdating}
                          onClick={() => void setRecipeReviewed(group.id, assetId, !reviewed)}
                        >
                          {reviewed ? "Reopen review" : "Looks good"}
                        </button>
                        {review && (
                          <>
                            <button
                              className={`review-choice clear ${isSyncSource ? "active-sync-source" : ""}`}
                              disabled={updating || reviewBatchUpdating || reviewSyncUpdating}
                              onClick={() => {
                                setReviewSyncSource({ groupId: group.id, assetId });
                                setReviewSyncTargets([]);
                                setReviewSyncFields({ exposure: true, contrast: true, saturation: true });
                                setReviewSyncNote(null);
                              }}
                            >
                              {isSyncSource ? "Exception source" : "Sync this exception"}
                            </button>
                            <button
                              className="review-choice clear recipe-reset"
                              disabled={updating || reviewBatchUpdating || reviewSyncUpdating}
                              onClick={() => void resetRecipeReview(assetId)}
                            >
                              Clear exception
                            </button>
                          </>
                        )}
                      </div>
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

  const renderLightroom = () => {
    const allHandoffGroups = (photoContext?.groups ?? []).map((group, index) => {
      const binding = referenceBindings[group.id];
      const preview = referencePreviews[group.id];
      const preflight = handoffPreflights[group.id];
      const preflightError = handoffPreflightErrors[group.id];
      const result = handoffResults[group.id];
      const bracketSets = bracketSetsForGroup(group.id);
      const detectedHdrSourceAssetIds = Array.from(
        new Set(bracketSets.flatMap((set) => set.members.map((member) => member.asset_id))),
      );
      const hdrSourceAssetIds =
        preflight?.hdr_source_asset_ids ?? detectedHdrSourceAssetIds;
      const hdrMergeCompleted = preflight?.hdr_merge_completed ?? false;
      const hdrMergeRequired =
        preflight?.hdr_merge_required ?? (hdrSourceAssetIds.length > 0 && !hdrMergeCompleted);
      const hdrSourceIdSet = new Set(hdrSourceAssetIds);
      const standardSourceCount = group.asset_ids.filter(
        (assetId) => !hdrSourceIdSet.has(assetId),
      ).length;
      const deliverableRecipes =
        preview?.recipes.filter((recipe) => recipe.target_asset_id != null) ?? [];
      const rejectedCount = group.asset_ids.filter(
        (assetId) => cullingReviews[assetId] === "REJECT",
      ).length;
      const exceptionCount = deliverableRecipes.filter(
        (recipe) =>
          recipe.target_asset_id != null && recipeReviews[recipe.target_asset_id] != null,
      ).length;
      const reviewAttentionCount = deliverableRecipes.filter(
        (recipe) =>
          recipe.target_asset_id != null &&
          recipeReviewPriority(recipe.target_asset_id) < 10,
      ).length;
      const conflictCount = preflight?.conflicting_sidecars.length ?? 0;
      const currentCount = preflight?.current_sidecars.length ?? 0;
      const preflightReady = !bridge?.preflightGroupXmp || preflight != null;
      const missingCount = preflight?.missing_sidecars.length ?? deliverableRecipes.length;
      const standardDeliveryExpected = standardSourceCount > 0;
      const unresolved =
        (standardDeliveryExpected &&
          (binding == null ||
            preview == null ||
            preview.pending_asset_id != null ||
            deliverableRecipes.length === 0)) ||
        preflightError != null ||
        !preflightReady;
      const ready =
        standardDeliveryExpected &&
        !unresolved &&
        conflictCount === 0 &&
        missingCount > 0;
      const safeBatchReady =
        ready && reviewAttentionCount === 0 && !hdrMergeRequired;
      const writesWhiteBalance = deliverableRecipes.some(
        (recipe) =>
          recipe.adjustments.temperature != null || recipe.adjustments.tint != null,
      );
      const priority =
        hdrMergeRequired || conflictCount > 0 || preflightError != null
          ? 0
          : unresolved
            ? 1
            : reviewAttentionCount > 0
              ? 2
              : missingCount > 0
                ? 3
                : 4;

      return {
        group,
        index,
        binding,
        preview,
        preflight,
        preflightError,
        result,
        deliverableRecipes,
        rejectedCount,
        exceptionCount,
        reviewAttentionCount,
        conflictCount,
        currentCount,
        preflightReady,
        missingCount,
        unresolved,
        ready,
        safeBatchReady,
        writesWhiteBalance,
        hdrSourceAssetIds,
        hdrMergeRequired,
        hdrMergeCompleted,
        standardSourceCount,
        priority,
      };
    });

    const batchConflictCount = allHandoffGroups.reduce(
      (total, item) => total + item.conflictCount,
      0,
    );
    const preflightErrorGroupCount = allHandoffGroups.filter(
      (item) => item.preflightError != null,
    ).length;
    const safeBatchGroups = allHandoffGroups.filter((item) => item.safeBatchReady);
    const selectedSafeBatchGroups = safeBatchGroups.filter((item) =>
      handoffBatchTargets.includes(item.group.id),
    );
    const selectedBatchGroupIds = selectedSafeBatchGroups.map((item) => item.group.id);
    const selectedBatchMissingCount = selectedSafeBatchGroups.reduce(
      (total, item) => total + item.missingCount,
      0,
    );
    const safeBatchMissingCount = safeBatchGroups.reduce(
      (total, item) => total + item.missingCount,
      0,
    );
    const batchReviewAttentionCount = allHandoffGroups.reduce(
      (total, item) => total + item.reviewAttentionCount,
      0,
    );
    const currentGroupCount = allHandoffGroups.filter(
      (item) => item.priority === 4,
    ).length;
    const conflictGroupCount = allHandoffGroups.filter(
      (item) => item.conflictCount > 0,
    ).length;
    const hdrMergeGroupCount = allHandoffGroups.filter(
      (item) => item.hdrMergeRequired,
    ).length;
    const actionGroupCount = allHandoffGroups.length - currentGroupCount;
    const verifiedTargetCount = Object.values(handoffResults).reduce(
      (total, result) => total + result.verified_sidecar_count,
      0,
    );
    const visibleHandoffGroups = allHandoffGroups
      .filter((item) => lightroomViewMode === "ALL" || item.priority < 4)
      .slice()
      .sort((left, right) => left.priority - right.priority || left.index - right.index);

    return (
      <section className="queue-card">
        <div className="queue-title">
          <strong>Lightroom XMP handoff</strong>
          <span>Explicit create-new writes · original RAW stays untouched · full-group Recipe verification after write</span>
        </div>

        <div className="review-summary">
          <div><span>Needs action</span><strong>{actionGroupCount}</strong></div>
          <div><span>HDR merge</span><strong>{hdrMergeGroupCount}</strong></div>
          <div><span>XMP conflicts</span><strong>{conflictGroupCount}</strong></div>
          <div><span>Verified this session</span><strong>{verifiedTargetCount}</strong></div>
        </div>

        <div className="cull-toolbar">
          <span>
            Needs action puts conflicts, unresolved groups, Recipe attention and missing XMP ahead of already-current delivery groups.
          </span>
          <div className="cull-toolbar-actions">
            <button
              className="cull-batch-action"
              disabled={handoffBatchRunning || handoffRunning != null || handoffPreflightLoading}
              onClick={() => setHandoffPreflightRevision((value) => value + 1)}
            >
              {handoffPreflightLoading ? "Checking XMP…" : "Refresh XMP checks"}
            </button>
            {batchReviewAttentionCount > 0 && (
              <button
                className="cull-batch-action"
                disabled={handoffBatchRunning || handoffRunning != null}
                onClick={() => setActiveView("review")}
              >
                Review attention ({batchReviewAttentionCount})
              </button>
            )}
            <div className="cull-view-switch" role="group" aria-label="Lightroom handoff view mode">
              {(["NEEDS_ACTION", "ALL"] as LightroomViewMode[]).map((viewMode) => (
                <button
                  className={lightroomViewMode === viewMode ? "active" : ""}
                  key={viewMode}
                  disabled={handoffBatchRunning || handoffRunning != null}
                  onClick={() => setLightroomViewMode(viewMode)}
                >
                  {viewMode === "NEEDS_ACTION" ? "Needs action" : "All"}
                </button>
              ))}
            </div>
          </div>
        </div>

        <div className="handoff-card">
          <div className="handoff-card-main">
            <span>Safe batch handoff</span>
            <strong>{safeBatchMissingCount} missing XMP across {safeBatchGroups.length} review-clear groups</strong>
            <small>
              Safe batch only includes groups whose Recipe attention is clear, whose XMP preflight succeeded without conflicts, and which contain no pending HDR bracket sources.
              HDR groups stay in Needs action so capture EV is preserved until merge.
            </small>
            <div className="handoff-batch-controls">
              <button
                className="cull-batch-action"
                disabled={handoffBatchRunning || handoffRunning != null || safeBatchGroups.length === 0}
                onClick={() => setHandoffBatchTargets(safeBatchGroups.map((item) => item.group.id))}
              >
                Select ready ({safeBatchGroups.length})
              </button>
              <button
                className="cull-batch-action"
                disabled={handoffBatchRunning || handoffRunning != null || handoffBatchTargets.length === 0}
                onClick={() => setHandoffBatchTargets([])}
              >
                Clear
              </button>
            </div>
            <small className={batchReviewAttentionCount > 0 ? "attention-text" : "success-text"}>
              {batchReviewAttentionCount > 0
                ? `${batchReviewAttentionCount} Recipe attention items are isolated from safe batch delivery.`
                : "Recipe review attention is clear for delivery groups."}
            </small>
            {batchConflictCount > 0 && (
              <small className="error-text">
                {batchConflictCount} conflicting XMP are isolated; safe groups can still be delivered.
              </small>
            )}
            {preflightErrorGroupCount > 0 && (
              <small className="error-text">
                {preflightErrorGroupCount} groups failed XMP preflight and remain isolated in Needs action.
              </small>
            )}
            {handoffBatchNote && <small className="success-text">{handoffBatchNote}</small>}
          </div>
          <div className="handoff-actions">
            <button
              className="button primary"
              disabled={
                !bridge?.writeBatchXmp ||
                handoffBatchRunning ||
                handoffRunning != null ||
                handoffPreflightLoading ||
                selectedBatchGroupIds.length === 0
              }
              onClick={() => void writeBatchXmp(selectedBatchGroupIds)}
            >
              {handoffBatchRunning
                ? "Writing + verifying selected XMP…"
                : selectedBatchGroupIds.length > 0
                  ? `Write + verify ${selectedBatchMissingCount} XMP in ${selectedBatchGroupIds.length} groups`
                  : "Select review-clear groups"}
            </button>
            <small>
              The backend re-preflights every selected group before the first write and rolls back new XMP if a later selected group fails.
            </small>
          </div>
        </div>

        <div className="handoff-groups">
          {visibleHandoffGroups.map((item) => {
            const {
              group,
              index,
              binding,
              preview,
              preflight,
              preflightError,
              result,
              deliverableRecipes,
              rejectedCount,
              exceptionCount,
              reviewAttentionCount,
              conflictCount,
              currentCount,
              preflightReady,
              missingCount,
              ready,
              safeBatchReady,
              writesWhiteBalance,
              hdrSourceAssetIds,
              hdrMergeRequired,
              hdrMergeCompleted,
              standardSourceCount,
            } = item;

            return (
              <div className="handoff-card" key={group.id}>
                <div className="handoff-card-main">
                  <span>Group {index + 1}</span>
                  <strong>{group.asset_ids.length} source photos</strong>
                  <small>
                    {binding
                      ? `Reference: ${assetNames.get(binding.selected_reference_asset_id) ?? binding.selected_reference_asset_id.slice(0, 8)}`
                      : hdrSourceAssetIds.length > 0 && standardSourceCount === 0
                        ? hdrMergeCompleted
                          ? "HDR merge complete · no Reference needed"
                          : "No Reference required before HDR merge"
                        : "Choose a reference first"}
                  </small>
                  <small>
                    {preview?.pending_asset_id
                      ? "Waiting for exposure evidence"
                      : preview
                        ? [
                            `${deliverableRecipes.length} XMP targets`,
                            rejectedCount ? `${rejectedCount} confirmed Reject skipped` : null,
                            reviewAttentionCount ? `${reviewAttentionCount} review attention` : "review clear",
                            currentCount ? `${currentCount} XMP already current` : null,
                            exceptionCount ? `${exceptionCount} photo exceptions` : null,
                            `WB ${writesWhiteBalance ? "measured" : "untouched"}`,
                          ]
                            .filter(Boolean)
                            .join(" · ")
                        : hdrSourceAssetIds.length > 0 && standardSourceCount === 0
                          ? hdrMergeCompleted
                            ? "HDR source set marked merged externally"
                            : "HDR source set waiting for external merge"
                          : "Resolve adaptive Recipes first"}
                  </small>
                  {hdrMergeRequired && (
                    <small className="attention-text">
                      HDR source set: {hdrSourceAssetIds
                        .map((assetId) => assetNames.get(assetId) ?? assetId.slice(0, 8))
                        .join(", ")}. Merge these RAWs in Lightroom/Camera Raw first; Photo-Cake leaves their normalization XMP untouched.
                    </small>
                  )}
                  {hdrMergeCompleted && hdrSourceAssetIds.length > 0 && (
                    <small className="success-text">
                      HDR merge marked complete for the current bracket membership. A changed bracket set will automatically reopen this action.
                    </small>
                  )}
                  {hdrSourceAssetIds.length > 0 && bridge?.setHdrMerged && (
                    <button
                      className="review-choice clear"
                      disabled={
                        hdrMergeUpdating === group.id ||
                        handoffRunning != null ||
                        handoffBatchRunning
                      }
                      onClick={() =>
                        void setHdrMergeCompleted(group.id, !hdrMergeCompleted)
                      }
                    >
                      {hdrMergeUpdating === group.id
                        ? "Updating HDR state…"
                        : hdrMergeCompleted
                          ? "Reopen HDR merge"
                          : "Mark HDR merged"}
                    </button>
                  )}
                  {safeBatchReady && (
                    <label className="handoff-batch-check">
                      <input
                        type="checkbox"
                        checked={handoffBatchTargets.includes(group.id)}
                        disabled={handoffBatchRunning || handoffRunning != null}
                        onChange={() =>
                          setHandoffBatchTargets((current) =>
                            current.includes(group.id)
                              ? current.filter((value) => value !== group.id)
                              : [...current, group.id],
                          )
                        }
                      />
                      <span>Include in safe batch</span>
                    </label>
                  )}
                  {preflightError && (
                    <small className="error-text">XMP preflight failed: {preflightError}</small>
                  )}
                  {conflictCount > 0 && (
                    <small className="error-text">
                      Conflicting XMP: {preflight!.conflicting_sidecars
                        .slice(0, 3)
                        .map(filenameFromPath)
                        .join(", ")}
                      {conflictCount > 3 ? ` +${conflictCount - 3} more` : ""}
                    </small>
                  )}
                </div>

                <div className="handoff-actions">
                  {result ? (
                    <>
                      <strong>{result.verified_sidecar_count} XMP verified</strong>
                      <small>
                        {result.written_sidecars.length} newly written · every target re-read against the current Recipe
                      </small>
                    </>
                  ) : (
                    <>
                      <button
                        className="button primary"
                        disabled={!ready || !bridge?.writeGroupXmp || handoffRunning != null || handoffBatchRunning}
                        onClick={() => void writeGroupXmp(group.id)}
                      >
                        {handoffRunning === group.id
                          ? "Writing + verifying XMP…"
                          : hdrSourceAssetIds.length > 0 && standardSourceCount === 0
                            ? hdrMergeCompleted
                              ? "HDR merge complete"
                              : "HDR merge first"
                            : conflictCount > 0
                              ? "Existing XMP conflict"
                              : !preflightReady && handoffPreflightLoading
                                ? "Checking XMP…"
                                : preflightReady && missingCount === 0 && deliverableRecipes.length > 0
                                  ? "XMP already current"
                                  : ready
                                    ? `Write + verify ${missingCount} standard XMP`
                                    : "Not ready"}
                      </button>
                      <small>
                        No overwrite: existing conflicts stop the group. For mixed groups, individual handoff may write standard peers while bracket RAWs remain untouched for HDR merge.
                      </small>
                    </>
                  )}
                </div>
              </div>
            );
          })}
          {visibleHandoffGroups.length === 0 && photoContext?.groups.length ? (
            <div className="panel-note">
              Delivery is current for every group. Switch to All to inspect completed groups.
            </div>
          ) : null}
          {!photoContext?.groups.length && (
            <div className="panel-note">Import, analyze and choose a reference before Lightroom handoff.</div>
          )}
        </div>
      </section>
    );
  };

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
            {mode === "workstation" && workflowStatus && (
              <p className={workflowStatus.next_focus === "COMPLETE" ? "success-text" : "workflow-next-summary"}>
                {workflowStatus.next_focus === "COMPLETE"
                  ? workflowFocusDetail(workflowStatus)
                  : `Next: ${workflowFocusLabel(workflowStatus.next_focus)} · ${workflowFocusDetail(workflowStatus)}`}
              </p>
            )}
            {importNote && <p className="success-text">{importNote}</p>}
            {backendError && <p className="error-text">{backendError}</p>}
          </div>
          <div className="batch-controls">
            {mode === "workstation" && workflowStatus?.next_focus === "COMPLETE" && (
              <span className="workflow-complete-pill">Delivery current</span>
            )}
            {mode === "workstation" &&
              workflowStatus &&
              workflowNextView &&
              workflowNextView !== activeView && (
              <button
                className="button primary"
                onClick={() => setActiveView(workflowNextView)}
              >
                Continue: {workflowFocusLabel(workflowStatus.next_focus)}
              </button>
            )}
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

        {mode === "workstation" && workflowStatus && (
          <div className={`workflow-focus-card ${workflowStatus.next_focus === "COMPLETE" ? "complete" : ""}`}>
            <span>{workflowStatus.next_focus === "COMPLETE" ? "Current state" : "Next photographer action"}</span>
            <strong>{workflowFocusLabel(workflowStatus.next_focus)}</strong>
            <small>{workflowFocusDetail(workflowStatus)}</small>
            {workflowNextView && (
              <button className="review-choice keep" onClick={() => setActiveView(workflowNextView)}>
                Open {workflowFocusLabel(workflowStatus.next_focus)}
              </button>
            )}
          </div>
        )}

        <div className="workflow-list">
          {workflowPanelSteps.map((step) => {
            const active =
              workflowStatus != null &&
              step.focus != null &&
              workflowStatus.next_focus === step.focus;
            const complete =
              workflowStatus != null &&
              (workflowStatus.next_focus === "COMPLETE" ||
                workflowNextOrder > step.order);
            return (
              <div
                className={`workflow-step ${active ? "active" : ""} ${complete ? "complete" : ""}`}
                key={step.number}
              >
                <span className="workflow-number">{step.number}</span>
                <div>
                  <strong>{step.title}</strong>
                  <small>{step.detail}</small>
                </div>
              </div>
            );
          })}
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
