import { useMemo, useState } from "react";
import { defaultRecipe, demoJobs, type AutomationRecipe, type BatchJob } from "./batch";

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
          <span>{job.stage}</span>
          {job.qa && <span>QA {job.qa}</span>}
          {job.error && <span className="error-text">{job.error}</span>}
        </div>
        <div className="progress"><span style={{ width: `${job.progress}%` }} /></div>
      </div>
      <div className="job-percent">{job.progress}%</div>
    </div>
  );
}

export default function App() {
  const [jobs, setJobs] = useState(demoJobs);
  const [recipe, setRecipe] = useState<AutomationRecipe>(defaultRecipe);
  const [paused, setPaused] = useState(false);

  const summary = useMemo(() => {
    const done = jobs.filter((job) => job.status === "DONE").length;
    const failed = jobs.filter((job) => job.status === "FAILED").length;
    const running = jobs.filter((job) => job.status === "RUNNING").length;
    return { done, failed, running, total: jobs.length };
  }, [jobs]);

  const updateRecipe = <K extends keyof AutomationRecipe>(key: K, value: AutomationRecipe[K]) => {
    setRecipe((current) => ({ ...current, [key]: value }));
  };

  const retryFailed = () => {
    setJobs((current) =>
      current.map((job) =>
        job.status === "FAILED"
          ? { ...job, status: "PENDING", error: undefined }
          : job,
      ),
    );
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
            <h1>Portrait Session</h1>
            <p>{summary.done}/{summary.total} complete · {summary.running} running · {summary.failed} failed</p>
          </div>
          <div className="batch-controls">
            {summary.failed > 0 && <button className="button secondary" onClick={retryFailed}>Retry failed</button>}
            <button className="button secondary" onClick={() => setPaused((value) => !value)}>
              {paused ? "Resume" : "Pause"}
            </button>
          </div>
        </section>

        <section className="queue-card">
          <div className="queue-title">
            <strong>Processing queue</strong>
            <span>Checkpoint after every stage</span>
          </div>
          <div className="job-list">
            {jobs.map((job) => <JobRow job={job} key={job.id} />)}
          </div>
        </section>
      </main>

      <aside className="automation-panel">
        <p className="eyebrow">AUTOMATION</p>
        <h2>{recipe.name}</h2>
        <p className="panel-note">Stages run automatically for every imported photo. Failed photos do not block the batch.</p>

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
          <span>IMPORT</span><i>→</i><span>ANALYZE</span><i>→</i><span>RETOUCH</span><i>→</i><span>QA</span><i>→</i><span>EXPORT</span>
        </div>
      </aside>
    </div>
  );
}
