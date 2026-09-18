# Architecture

## Scope

Photo-Cake is a single-user local-first semi-automatic AI photo workflow application.

The goal is to reduce repetitive Lightroom-style manual editing while keeping RAW files immutable and maintaining professional workflow compatibility.

## Layers

```text
              packages/ui
          Responsive React UI
                   |
          +--------+--------+
          |                 |
   apps/windows        apps/android
    Tauri host          Tauri host
          |                 |
          +--------+--------+
                   |
            crates/photo-core
 project / catalog / jobs / edits / QA
                   |
        +----------+----------+
        |                     |
 platform storage      inference adapter
        |                     |
 Windows filesystem     CUDA/DirectML/CPU
 Android SAF            NNAPI/QNN/Vulkan
```

## Platform contract

Windows and Android are separate deliverables, not separate application logic.

Shared behavior:

- project schema
- catalog and asset identity
- batch jobs and checkpoints
- edit graph semantics
- QA rules
- responsive UI contracts

Platform-specific:

- file picking
- drag/drop
- touch/stylus
- packaging
- hardware acceleration bindings
- thermal/resource policy

## Product workflow architecture

```text
IMPORT RAW
  -> ANALYZE
  -> AI CULLING
  -> GROUPING
  -> REFERENCE STYLE SELECTION
  -> RECIPE GENERATION
  -> AI RETOUCH PLAN
  -> EDIT GRAPH
  -> QA REVIEW
  -> XMP / EXPORT
```

The system optimizes for photo groups instead of isolated images.

## Non-destructive editing

Original files are immutable.

Stored project data:

- original references
- asset identity/hash
- metadata
- previews
- AI analysis cache
- grouping results
- reference style data
- edit graph
- masks
- job/checkpoint state
- export recipes

RAW files are never modified. Lightroom-compatible XMP output remains a primary workflow.

## Batch execution model

Each imported photo belongs to a persistent batch job.

Rules:

- one active worker per batch
- checkpoint before and after stages
- failed photos are isolated
- retry resumes from failed stage
- pause/cancel take effect at safe stage boundaries
- abnormal termination restores incomplete work safely

Export uses atomic writes:

```text
render
 -> partial file
 -> verification
 -> atomic rename
 -> DONE
```

## Local inference architecture

Photo-Cake is offline-first.

Normal operations must work without cloud AI:

- ingest
- classification
- grouping
- color analysis
- portrait analysis
- masks
- QA
- export

Principles:

- online inference disabled by default
- deterministic processing preferred when models are unnecessary
- lightweight reusable local vision models preferred
- inference results are versioned and cached
- only affected tasks are recomputed

Shared artifacts:

- person/face detection
- image embeddings
- duplicate detection
- face embeddings
- segmentation masks

Cache identity:

- asset ID
- source fingerprint
- preview revision
- inference task
- model ID/version
- configuration hash

Backend performance may vary, but semantic results must remain consistent.

## AI retouch direction

AI produces editable intent, not destructive replacement:

- exposure suggestions
- white balance
- portrait enhancement masks
- skin refinement
- lighting adjustment
- background adjustment
- group style recipes

## First milestone

Build the stable automation foundation:

- background execution
- persistent jobs
- crash recovery
- resumable stages
- safe export
- RAW + XMP workflow

Advanced catalog, preview, grouping and AI retouch build on this foundation.
