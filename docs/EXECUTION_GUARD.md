# Photo-Cake Execution Guardrails

Before changing code:

1. Inspect current main branch implementation.
2. Check CI status after every implementation batch.
3. Do not claim integration unless code path is actually connected.
4. Do not close issues until acceptance criteria and CI are green.

Current priority:

## Export Pipeline Integration

Verify and complete:

BatchStage::Export
 -> BatchExportExecutor
 -> ExportStore checkpoint lifecycle
 -> ExportWorker
 -> renderer
 -> DONE/FAILED state

Required checks:

- compile
- tests
- Windows CI
- no regression in model smoke tests

After this:

## Background Worker

Implement only after export lifecycle is verified.

UI
 -> Job Queue
 -> Background Worker
 -> AutomationRunner
 -> Checkpoint Store

Rules:

- Preserve RAW originals
- Reuse completed stages
- Prefer small verified commits
- Never skip CI verification
