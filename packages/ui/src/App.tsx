import { useEffect, useMemo, useState } from "react";
import {
  demoJobs,
  type BackendBatch,
  type BackendBatchItem,
  type BackendCullingReview,
  type BackendGroupCullingResult,
  type BackendPhotoContext,
  type BackendReferenceBinding,
  type BatchJob,
  type BatchStage,
  type CullingDecision,
  type CullingUserDecision,
  type PhotoCakeBridge,
} from "./batch";

export type {
  BackendBatch,
  BackendCullingReview,
  BackendGroupCullingResult,
  BackendPhotoContext,
  BackendRawImportResult,
  BackendReferenceBinding,
  BatchWorkerEvent,
  PhotoCakeBridge,
} from "./batch";

type WorkspaceView = "library" | "cull" | "groups" | "reference" | "lightroom";

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
  const [referenceBindings, setReferenceBindings] = useState<Record<string, BackendReferenceBinding>>({});

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
  }, [activeBatchId, bridge]);

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
  }, [activeBatchId, analysisRevision, bridge]);

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
  }, [activeBatchId, bridge]);

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
  }, [activeBatchId, bridge]);

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
    const effective = recommendations.map((item) => {
      const user = cullingReviews[item.asset_id];
      if (user) return user;
      return item.decision === "REJECT_SUGGESTION" ? "REJECT" : item.decision;
    });
    return {
      keep: effective.filter((decision) => decision === "KEEP").length,
      review: effective.filter((decision) => decision === "REVIEW").length,
      reject: effective.filter((decision) => decision === "REJECT").length,
      pending: culling.reduce((total, group) => total + group.pending_asset_ids.length, 0),
      confirmed: Object.keys(cullingReviews).length,
    };
  }, [culling, cullingReviews]);

  const assetNames = useMemo(
    () => new Map(photoContext?.assets.map((asset) => [asset.id, asset.filename]) ?? []),
    [photoContext],
  );

  const cullingRecommendations = useMemo(() => {
    const values = culling.flatMap((group) => group.recommendations);
    return new Map(values.map((item) => [item.asset_id, item]));
  }, [culling]);

  const referenceCandidatesForGroup = (assetIds: string[]) =>
    assetIds
      .filter((assetId) => {
        const user = cullingReviews[assetId];
        if (user === "REJECT") return false;
        const recommendation = cullingRecommendations.get(assetId);
        return user != null || recommendation?.decision !== "REJECT_SUGGESTION";
      })
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
    if (bridge && activeBatch) {
      void runBackendAction(() => bridge.retryFailed(activeBatch.id));
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
    if (bridge && activeBatch) {
      void runBackendAction(() =>
        isPaused ? bridge.resumeBatch(activeBatch.id) : bridge.pauseBatch(activeBatch.id),
      );
      return;
    }
    setDemoPaused((value) => !value);
  };

  const startOrContinue = () => {
    if (bridge && activeBatch) {
      void runBackendAction(() => bridge.runBatch(activeBatch.id));
    }
  };

  const cancelBatch = () => {
    if (bridge && activeBatch) {
      void runBackendAction(() => bridge.cancelBatch(activeBatch.id));
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
      setBackendError(null);
    } catch (error) {
      setBackendError(String(error));
    }
  };

  const setReferencePhoto = async (groupId: string, assetId: string) => {
    if (!bridge?.setGroupReference) return;
    try {
      const binding = await bridge.setGroupReference(groupId, assetId);
      setReferenceBindings((current) => ({ ...current, [groupId]: binding }));
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
      setBackendError(null);
    } catch (error) {
      setBackendError(String(error));
    }
  };

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
            <strong>Cull → Groups → Reference → XMP</strong>
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

      <section className="queue-card">
        <div className="queue-title">
          <strong>RAW preparation queue</strong>
          <span>Import and local analysis only · editing starts after photos are ready</span>
        </div>
        <div className="job-list">
          {jobs.length > 0
            ? jobs.map((job) => <JobRow job={job} key={job.id} />)
            : <div className="panel-note">Import an existing RAW folder. Source RAW files stay in place.</div>}
        </div>
      </section>
    </>
  );

  const renderCull = () => (
    <>
      <section className="photo-overview">
        <div className="overview-card"><span>Keep</span><strong>{cullingSummary.keep}</strong></div>
        <div className="overview-card"><span>Review</span><strong>{cullingSummary.review}</strong></div>
        <div className="overview-card"><span>Reject</span><strong>{cullingSummary.reject}</strong></div>
        <div className="overview-card"><span>Confirmed / pending</span><strong>{cullingSummary.confirmed} / {cullingSummary.pending}</strong></div>
      </section>

      <section className="queue-card">
        <div className="queue-title">
          <strong>Smart culling</strong>
          <span>Group-relative quality + near-duplicate evidence · never deletes originals</span>
        </div>
        {cullingLoading && <div className="panel-note">Refreshing cached culling evidence…</div>}
        {!cullingLoading && culling.length === 0 && (
          <div className="panel-note">Import and analyze RAW photos before culling.</div>
        )}
        <div className="cull-groups">
          {culling.map((group, groupIndex) => (
            <div className="cull-group" key={group.group_id}>
              <div className="cull-group-head">
                <strong>Group {groupIndex + 1}</strong>
                <span>
                  {group.recommendations.length} scored · {group.pending_asset_ids.length} pending
                </span>
              </div>
              <div className="cull-list">
                {group.recommendations.map((item) => {
                  const userDecision = cullingReviews[item.asset_id];
                  return (
                    <div className="cull-row" key={item.asset_id}>
                      <span className="cull-rank">#{item.group_rank}</span>
                      <div className="cull-name">
                        <strong>{assetNames.get(item.asset_id) ?? item.asset_id.slice(0, 8)}</strong>
                        <small>
                          Quality {Math.round(item.quality_score * 100)}% · Suggested {cullingLabel(item.decision)}
                        </small>
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
                      <span className="cull-rank">—</span>
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
        <span>Moment groups are conservative; semantic refinement stays inside the parent group</span>
      </div>
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
          const candidates = referenceCandidatesForGroup(group.asset_ids);
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
                </div>
              </div>

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

  const renderFutureView = (title: string, body: string) => (
    <section className="queue-card">
      <div className="queue-title"><strong>{title}</strong></div>
      <div className="panel-note">{body}</div>
    </section>
  );

  return (
    <div className="app-shell">
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
          {([
            ["library", "Library"],
            ["cull", "Cull"],
            ["groups", "Groups"],
            ["reference", "Reference"],
            ["lightroom", "Lightroom"],
          ] as const).map(([view, label]) => (
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
            <h1>{activeBatch?.name ?? (bridge ? "Import a RAW folder" : "Photography workflow")}</h1>
            <p>{summary.done}/{summary.total} ready · {summary.running} analyzing · {summary.failed} failed</p>
            {importNote && <p className="success-text">{importNote}</p>}
            {backendError && <p className="error-text">{backendError}</p>}
          </div>
          <div className="batch-controls">
            {bridge && activeBatch && summary.pending > 0 && summary.running === 0 && !isPaused && (
              <button className="button primary" onClick={startOrContinue}>Analyze / Continue</button>
            )}
            {summary.failed > 0 && <button className="button secondary" onClick={retryFailed}>Retry failed</button>}
            {jobs.length > 0 && (
              <button className="button secondary" onClick={togglePause}>
                {isPaused ? "Resume" : "Pause"}
              </button>
            )}
            {bridge && activeBatch && jobs.some((job) => !["DONE", "CANCELLED"].includes(job.status)) && (
              <button className="button secondary" onClick={cancelBatch}>Cancel</button>
            )}
          </div>
        </section>

        {activeView === "library" && renderLibrary()}
        {activeView === "cull" && renderCull()}
        {activeView === "groups" && renderGroups()}
        {activeView === "reference" && renderReference()}
        {activeView === "lightroom" && renderFutureView(
          "Lightroom handoff",
          "Core same-basename XMP generation is implemented and protects existing sidecars. The workstation UI will expose explicit apply/handoff only after a reference-driven Recipe set exists.",
        )}
      </main>

      <aside className="automation-panel">
        <p className="eyebrow">PHOTOGRAPHY WORKFLOW</p>
        <h2>Semi-automatic, reference driven</h2>
        <p className="panel-note">
          Background analysis prepares reusable evidence. Photo-Cake then helps you review groups,
          choose a reference look, adapt it per photo and hand tiny XMP sidecars to Lightroom.
        </p>

        <div className="workflow-list">
          {[
            ["1", "Import & analyze", "Keep RAW untouched"],
            ["2", "Cull", "Quality + near-duplicate evidence"],
            ["3", "Group", "Moment → semantic similarity"],
            ["4", "Reference look", "Your preferred photo/style"],
            ["5", "Adaptive recipe", "Different correction per photo"],
            ["6", "Lightroom XMP", "Or direct export on demand"],
          ].map(([number, title, detail]) => (
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
          <span>Original RAW + small XMP. No automatic TIFF/JPEG working copies.</span>
        </div>
      </aside>
    </div>
  );
}
