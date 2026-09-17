# Batch Automation

Batch automation is a first-class product requirement, not a later add-on.

## Goals

1. Import a directory or selected files.
2. Create one independent job per photo.
3. Run configured stages automatically.
4. Persist a checkpoint after every completed stage.
5. Continue other photos when one fails.
6. Retry only failed stages, not the whole photo.
7. Pause/resume safely when Windows sleeps or a tablet app is backgrounded.
8. Run automatic QA before export.
9. Keep originals untouched.

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

Stages may be disabled by a workflow recipe. For example, a simple resize/export recipe can skip ANALYZE and RETOUCH.

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

## Checkpoint model

Each photo persists:

- current stage
- completed stages
- edit revision/hash
- model/version identifiers
- input file fingerprint
- retry count
- error details
- QA result
- output paths

If an input file or relevant model/edit revision changes, only affected stages should become stale.

## QA behavior

QA does not silently delete or hide images. It produces PASS / REVIEW / FAIL with reasons such as blur, closed eyes, overexposure, mask anomaly, excessive smoothing, or export failure.

The default export policy will be configurable. Safe initial behavior is to export PASS images automatically and leave REVIEW/FAIL visible for manual confirmation.
