import { useEffect, useMemo, useState } from "react";
import {
  demoJobs,
  type BackendBatch,
  type BackendBatchItem,
  type BatchJob,
  type BatchStage,
  type PhotoCakeBridge,
} from "./batch";

export type {
  BackendBatch,
  BackendRawImportResult,
  BatchWorkerEvent,
  PhotoCakeBridge,
} from "./batch";

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
  const [demoPaused, setDemoPaused] = useState(false);
  const [backendError, setBackendError] = useState<string | null>(null);
  const [importNote, setImportNote] = useState<string | null>(null);
  const [importing, setImporting] = useState(false);

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
          <button className="nav-item active">Library</button>
          <button className="nav-item">Cull</button>
          <button className="nav-item">Groups</button>
          <button className="nav-item">Reference</button>
          <button className="nav-item">Lightroom</button>
        </nav>
        <div className="sidebar-foot">
          <span>{mode === "workstation" ? "Workstation" : "Companion"}</span>
          <strong>Local</strong>
        </div>
      </aside>

      <main className="workspace">
        <section className="batch-head">
          <div>
            <p className="eyebrow">RAW PREPARATION</p>
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
            ["2", "Cull", "Keep / Review / Reject suggestion"],
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
