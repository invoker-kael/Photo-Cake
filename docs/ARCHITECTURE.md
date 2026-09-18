# Architecture

## Scope

Photo-Cake is intentionally a single-user local application. There is no account system, organization model, multi-tenant service, or mandatory cloud scheduler.

The product goal is a local-first semi-automatic AI photo workflow: reduce repetitive Lightroom-style manual editing by automating analysis, grouping, retouch planning, batch application, QA, and export.

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

Windows and Android are separate deliverables, not separate codebases. Shared behavior belongs in common packages/crates:

- project schema
- catalog and asset identity
- batch jobs and checkpoints
- edit graph semantics
- QA rules
- responsive UI contracts

Platform shells only own platform behavior:

- file picking
- drag/drop
- touch/stylus
- packaging
- thermal/resource policy
- hardware acceleration bindings

## Product workflow architecture

The main workflow is:

```text
IMPORT
  -> ANALYZE
  -> CULL / GROUP
  -> SELECT REFERENCE LOOK
  -> AI RETOUCH PLAN
  -> APPLY EDIT GRAPH
  -> QA REVIEW
  -> EXPORT
```

The system optimizes for groups rather than isolated images. A user should be able to adjust one reference image or recipe and apply the intent across similar photos.

## Non-destructive editing

Original files are immutable.

A project stores:

- original references
- asset identity/hash
- metadata
- previews
- AI analysis cache
- grouping results
- edit graph
- masks
- job/checkpoint state
- export recipes

Pixel output is generated only during rendering/export.

## AI retouch direction

AI features should produce editable intent, not destructive replacements:

- exposure and white balance suggestions
- portrait enhancement masks
- skin refinement parameters
- lighting adjustments
- background adjustments
- group-based style recipes

## First milestone

The first milestone remains a reliable automation substrate:

- background execution
- persistent jobs
- crash recovery
- resumable stages
- safe export

After that, catalog, preview, grouping, and semi-automatic AI retouch are built on top of the stable execution model.
