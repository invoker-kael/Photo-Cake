# Batch Automation

Batch automation is a first-class product requirement, not a later add-on.

## Implemented foundation

The current core now persists batches and per-photo jobs in SQLite. The Windows host opens `photo-cake.sqlite3` in the application data directory and recovers interrupted work on startup.

Implemented behavior:

- one independent persisted job per photo
- checkpoint before stage execution (`RUNNING`)
- checkpoint after success, failure, pause, resume, retry, or cancel
- abnormal-exit recovery: `RUNNING` becomes `PENDING` at the same stage
- explicit retry resumes the failed stage instead of restarting import
- one failed photo does not block other pending photos by default
- optional `stop_on_error` policy
- automatic QA stage can be skipped by the batch contract
- SQLite WAL mode for durable local state

The actual image-processing stage implementations are still adapters; the current default executor is a no-op so the automation contract can be tested before RAW/AI engines are attached.

## Default pipeline

```text
IMPORT
  -> ANALYZE
  -> APPLY_PRESET
  -> RETOUCH
  -> QA
  -> EXPORT
  -> DONE
```

## Job states

```text
PENDING
RUNNING
PAUSED
FAILED
CANCELLED
DONE
```

Stage and status are deliberately separate. If ANALYZE fails, retrying the job restarts ANALYZE rather than IMPORT.

## Crash-safe transition

```text
PENDING / stage=ANALYZE
        |
        | persist before work
        v
RUNNING / stage=ANALYZE
        |
        +---- success ----> PENDING / stage=APPLY_PRESET
        |
        +---- failure ----> FAILED  / stage=ANALYZE
        |
        +---- process dies
                         next launch
                            |
                            v
                    PENDING / stage=ANALYZE
```

This prevents a crash from falsely marking an unfinished stage as complete and prevents already completed stages from being repeated.

## SQLite records

`batches` stores batch-level policy. `batch_items` stores the ordered photo jobs with:

- stable UUID
- source reference/path (portable asset IDs come later)
- current stage
- current status
- attempt count
- last error

Future migrations will extend this with edit revision/hash, model versions, input fingerprint, QA result and export records.

## Runner API

The shared Rust core exposes `AutomationRunner<E>` where `E: StageExecutor`. Windows, Android and future inference backends plug into the same runner contract.

The runner supports:

- create/list/load batches
- run one checkpointed stage
- run until no pending work remains
- retry failed jobs
- pause/resume a batch
- cancel a batch
- recover interrupted jobs

## Automation recipes

A future recipe will look conceptually like:

```json
{
  "name": "Portrait Natural",
  "watch": false,
  "preset": "natural-v1",
  "autoRetouch": true,
  "autoQa": true,
  "export": {
    "format": "jpeg",
    "quality": 95,
    "colorSpace": "sRGB"
  },
  "failurePolicy": {
    "retries": 2,
    "continueBatch": true
  }
}
```

## Planned automation triggers

- Manual selection
- Folder import
- Drag and drop
- Optional watched folder
- Re-run failed only
- Re-run QA only
- Export approved only

Watched folders are intentionally optional because Photo-Cake is a personal editor, not a server daemon.

## QA behavior

QA does not silently delete or hide images. It will produce PASS / REVIEW / FAIL with reasons such as blur, closed eyes, overexposure, mask anomaly, excessive smoothing, or export failure.

The safe initial export policy remains: export PASS images automatically and leave REVIEW/FAIL visible for manual confirmation.
