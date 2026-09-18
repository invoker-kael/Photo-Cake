# Photo-Cake Luna Execution Plan

## Purpose

This document is the execution contract for autonomous implementation agents.
Read this file before changing code. Keep changes incremental, testable, and committed to `main` unless a short-lived development branch is explicitly required.

## Current Architecture Goal

Photo-Cake is a local-first batch photo processing application.

Required pipeline:

```
Import
  -> Analyze
  -> Classification / Embedding
  -> Grouping
  -> Preset / Retouch
  -> QA
  -> Export
```

Requirements:

- UI must remain responsive while batches execute.
- RAW originals are immutable.
- Every expensive stage must have reusable checkpoints.
- Export changes must not invalidate AI analysis or editing stages.
- Windows is the primary platform; shared core must remain portable for Android.

---

# Phase 1 - Repair Current Export Integration

Priority: P0

## Problem

`ExportStore`, `ExportWorker`, and `BatchExportExecutor` exist but are not fully connected.

Current missing flow:

```
BatchStage::Export
      |
      v
ExportStore checkpoint lifecycle
      |
      v
ExportWorker
```

## Required implementation

Modify export execution so every export follows:

```
reserve(batch_id, item_id)
        |
mark_running()
        |
render temporary file
        |
verify output
        |
atomic rename
        |
mark_done()
```

Failure:

```
render error
      |
mark_failed(error)
      |
retry only export stage
```

Do not rerun:

- Import
- Analyze
- Embedding
- Classification
- Grouping
- Retouch

## Tests Required

- successful export
- renderer failure
- restart recovery
- retry keeps output reservation
- RAW hash unchanged
- partial file cleanup

---

# Phase 2 - Automation Runner Integration

Priority: P0

Connect BatchStage::Export into the existing runner.

Expected:

```
BatchItem(stage=Export)
          |
          v
ExportStageExecutor
          |
          v
BatchExportExecutor
```

Acceptance:

- Full pipeline can reach DONE.
- Failed export keeps previous stages completed.
- Multiple photos have independent checkpoints.

---

# Phase 3 - Background Worker

Priority: P0

Implement:

```
UI
 |
Job Queue
 |
Background Worker
 |
AutomationRunner
 |
Checkpoint Store
```

Required features:

- UI never blocks during batch execution.
- Pause/resume.
- Cancel pending work.
- Crash recovery.
- Progress reporting.

---

# Phase 4 - PixCake-like Workflow UX

Priority: P1

Implement project-oriented workflow:

```
Create Project
      |
Import Folder
      |
Analyze Photos
      |
Review Groups
      |
Apply Preset
      |
QA
      |
Export
```

Do not copy proprietary implementation.

---

# Validation Rules

Before closing any issue:

1. Acceptance criteria must be satisfied.
2. Relevant tests must pass.
3. Windows CI must be green.
4. No fake implementation or skipped validation.
5. Reuse existing modules before creating new ones.

---

# Commit Style

Use small meaningful commits:

```
feat: ...
fix: ...
test: ...
docs: ...
```

Never create replacement long-lived platform branches.
