# Batch automation

Photo-Cake treats every imported photo as an independent, persistent job. The source RAW is immutable; only cache artifacts, analysis, edit state, QA state, and export state are written by the application.

## Default local workflow

\`\`\`text
IMPORT
  -> ANALYZE
     -> portrait / non-portrait classification
     -> local image embedding
     -> adaptive grouping
  -> APPLY_PRESET
  -> PORTRAIT_RETOUCH (portrait only)
  -> QA
  -> EXPORT
  -> DONE
\`\`\`

The product is designed around a project/batch workflow rather than a single blocking edit command. Importing RAW files creates a persistent batch and starts background analysis automatically. The UI remains available while the worker processes the queue.

## Background worker behavior

- One active worker is allowed per batch.
- Work is checkpointed before and after every stage.
- The worker executes one persisted stage at a time and emits batch updates to the UI after each transition.
- Failed photos remain isolated. Other photos continue unless \`stop_on_error\` is enabled.
- Pause and cancel are cooperative: the current stage is allowed to finish and persist safely, then the control request takes effect at the next stage boundary.
- Resume wakes the same batch worker and continues from the stored stage rather than restarting the photo.
- Retry returns failed photos to \`PENDING\` at the failed stage only.
- An abnormal process exit changes persisted \`RUNNING\` items back to \`PENDING\` on next startup, preserving the stage. The app does not silently assume that every pending batch should auto-run after restart; persistent active/paused project intent will be added before automatic restart continuation is enabled.

## Export checkpoint behavior

- \`BatchExportExecutor\` reserves one stable output path per \`batch_id + item_id\`.
- The reservation is persisted before rendering, so a retry never selects a different collision-safe path.
- The renderer writes only a \`.partial\` file. A non-empty file is atomically renamed to the reserved final path before the checkpoint becomes \`DONE\`.
- Renderer, verification, rename, and store failures mark only that export checkpoint \`FAILED\`; upstream stages remain reusable.
- Startup recovery removes stale partial files and returns incomplete exports to \`PENDING\`. If the final file was already atomically renamed and is non-empty, recovery finishes the checkpoint as \`DONE\`.

## Batch-edit ergonomics

Photo-Cake follows a standard-photo / group / review pattern for large shoots:

1. Import into an explicit project/batch without changing the source folder.
2. Analyze previews locally and split photos by useful semantic/visual context.
3. Establish or select a reference look for a group.
4. Apply the intent across the group, resolving per-photo parameters instead of blindly copying raw numeric values.
5. Run portrait-specific processing only where classification says it is useful.
6. Perform automatic QA and surface exceptions for review.
7. Export approved photos directly from Photo-Cake; Lightroom/Photoshop handoff remains secondary.

This keeps repetitive work automatic while reserving manual attention for reference images, ambiguous classifications, QA exceptions, and high-value final refinements.

## Persistence

SQLite uses WAL mode. Batch records store the current stage/status, attempts, errors, and stable catalog asset identity. Preview and analysis caches include source/model/config revisions so unchanged evidence can be reused without rerunning earlier stages.

Future invalidation metadata will continue to be scoped: changing an output recipe must not rerun analysis or retouch; changing a model or semantic classification should invalidate only the dependent semantic/grouping/retouch work.
