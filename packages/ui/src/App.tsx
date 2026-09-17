import { useEffect, useMemo, useState } from "react";
import {
  defaultRecipe,
  demoJobs,
  type AutomationRecipe,
  type BackendBatch,
  type BackendBatchItem,
  type BatchJob,
  type BatchStage,
  type PhotoCakeBridge,
} from "./batch";

export type { BackendBatch, BatchWorkerEvent, PhotoCakeBridge } from "./batch";

const stageProgress: Record<BatchStage, number> = {
  IMPORT: 5,
  ANALYZE: 25,
  APPLY_PRESET: 45,
  PORTRAIT_RETOUCH: 65,
  QA: 82,
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
  return stage.replaceAll("_", " ");
}

function Toggle({ value, onChange }: { value: boolean; onChange: (next: boolean) => void }) {
  return (
    <button
      className={`toggle ${value ? "toggle-on" : ""}`}
      type="button"
      role="switch"
      aria-checked={value}
      onClick={() => onChange(!value)}
    >
      <span />
    </button>
  );
}

function JobRow({ job }: { job: BatchJob }) {
  return (
    <div className="job-row">
      <div className="thumb">RAW</div>
      <div className="job-main">
        <div className="job-title-line">
          <strong>{job.filename}</strong>
          <span className={`status status-${job.status.toLowerCase()}`}>{job.status}</span>
        </div>
        <div className="job-meta">
          <span>{stageLabel(job.stage)}</span>
          {job.qa && <span>QA {job.qa}</span>}
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
}

export default function App({ bridge }: AppProps) {
  const [demoState, setDemoState] = useState(demoJobs);
  const [batches, setBatches] = useState<BackendBatch[]>([]);
  const [activeBatchId, setActiveBatchId] = useState<string | null>(null);
  const [recipe, setRecipe] = useState<AutomationRecipe>(defaultRecipe);
  const [demoPaused, setDemoPaused] = useState(false);
  const [backendError, setBackendError] = useState<string | null>(null);

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

  const updateRecipe = <K extends keyof AutomationRecipe>(key: K, value: AutomationRecipe[K]) => {
    setRecipe((current) => ({ ...current, [key]: value }));
  };

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
          <div className="subtitle">Local AI photo workflow</div>
        </div>
        <div className="top-actions">
          <button className="button secondary">Import Photos</button>
          <button className="button primary">New Batch</button>
        </div>
      </header>

      <aside className="sidebar">
        <nav>
          <button className="nav-item active">Batch</button>
          <button className="nav-item">Library</button>
          <button className="nav-item">Edit</button>
          <button className="nav-item">Review</button>
          <button className="nav-item">Export</button>
        </nav>
        <div className="sidebar-foot">
          <span>Device</span>
          <strong>Local</strong>
        </div>
      </aside>

      <main className="workspace">
        <section className="batch-head">
          <div>
            <p className="eyebrow">ACTIVE BATCH</p>
            <h1>{activeBatch?.name ?? (bridge ? "No batch loaded" : "Portrait Session")}</h1>
            <p>{summary.done}/{summary.total} complete · {summary.running} running · {summary.failed} failed</p>
            {backendError && <p className="error-text">{backendError}</p>}
          </div>
          <div className="batch-controls">
            {bridge && activeBatch && summary.pending > 0 && summary.running === 0 && !isPaused && (
              <button className="button primary" onClick={startOrContinue}>Start / Continue</button>
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
            <strong>Background processing queue</strong>
            <span>Checkpoint after every stage · current stage finishes before pause/cancel</span>
          </div>
          <div className="job-list">
            {jobs.length > 0
              ? jobs.map((job) => <JobRow job={job} key={job.id} />)
              : <div className="panel-note">Import RAW photos to create a persistent batch.</div>}
          </div>
        </section>
      </main>

      <aside className="automation-panel">
        <p className="eyebrow">AUTOMATION</p>
        <h2>{recipe.name}</h2>
        <p className="panel-note">Stages keep running in the background. Failed photos remain isolated while the rest of the batch can continue.</p>

        {([
          ["autoAnalyze", "Analyze"],
          ["autoPreset", "Apply preset"],
          ["autoRetouch", "AI retouch"],
          ["autoQa", "Automatic QA"],
          ["autoExportPass", "Export QA PASS"],
        ] as const).map(([key, label]) => (
          <div className="setting-row" key={key}>
            <span>{label}</span>
            <Toggle value={recipe[key]} onChange={(value) => updateRecipe(key, value)} />
          </div>
        ))}

        <div className="setting-block">
          <label htmlFor="retries">Retries per failed stage</label>
          <input
            id="retries"
            type="number"
            min="0"
            max="10"
            value={recipe.retries}
            onChange={(event) => updateRecipe("retries", Number(event.target.value))}
          />
        </div>

        <div className="setting-row">
          <span>Continue on error</span>
          <Toggle value={recipe.continueOnError} onChange={(value) => updateRecipe("continueOnError", value)} />
        </div>

        <div className="automation-flow">
          <span>IMPORT</span><i>→</i><span>ANALYZE</span><i>→</i><span>PRESET</span><i>→</i><span>RETOUCH</span><i>→</i><span>QA</span><i>→</i><span>EXPORT</span>
        </div>
      </aside>
    </div>
  );
}
