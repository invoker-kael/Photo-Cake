# Phase 1 - Export Stage Execution

## Objective

Complete the reliable batch completion path for Photo-Cake before expanding AI retouch features.

## Flow

AutomationRunner
-> BatchPipelineExecutor
-> ExportStageExecutor
-> BatchExportExecutor
-> ExportStore
-> ExportWorker
-> DONE / FAILED

## Current priority

1. Complete persistent export contract
2. Implement full-resolution renderer integration
3. Verify checkpoint recovery
4. Verify retry without rerunning previous stages
5. Verify source RAW immutability

## Required behavior

- Export recipe is persisted
- Per-photo export state is persisted
- Output reservation is collision-safe
- Existing derivatives are never silently overwritten
- Failed exports only retry export stage
- Interrupted RUNNING exports recover safely
- RAW originals remain unchanged

## Verification

Required tests:

- successful export
- renderer failure
- retry
- crash recovery
- collision handling
- RAW unchanged

After completion continue with:

Phase 2 - Catalog + Preview Cache
