# Photo-Cake Luna Execution Specification

## Purpose

This document is the execution authority for Luna.

Photo-Cake is a local-first semi-automatic photography workflow assistant.

The target workflow:

```text
RAW Collection
    |
Catalog
    |
Preview + Metadata
    |
Smart Culling
    |
Photo Group
    |
Reference Set
    |
Style Profile
    |
Recipe / Edit Graph
    |
XMP / Direct Export
```

The goal is not replacing Lightroom. The goal is reducing repetitive photographer work while keeping RAW files, Lightroom compatibility and photographer decisions.

---

## Existing Code First

Reuse existing implementation before creating new systems.

```text
photo-core
 |
 +-- catalog
 +-- importer
 +-- raw
 +-- metadata
 +-- preview
 +-- grouping
 +-- culling
 +-- reference
 +-- recipe
 +-- export

photo-inference
 |
 +-- analysis
 +-- similarity
 +-- segmentation
 +-- culling models
 +-- style extraction
```

Rules:

- Extend existing modules.
- Do not create parallel workflow engines.
- Keep workflow logic platform independent.
- Do not make renderer/export the source of truth.

---

## Photography Workflow Rules

The workflow follows real photographer behavior:

```text
Many RAW files
      |
      v
AI assisted selection
      |
      v
Photo groups
      |
      v
Choose preferred references
      |
      v
Generate style recipe
      |
      v
Apply batch adjustments
```

Important:

- RAW remains unchanged.
- XMP is the preferred Lightroom delivery format and should remain tiny metadata beside the RAW.
- Direct export remains available.
- AI recommends; user controls final selection.

---

## Platform Strategy

Windows:

- Primary RAW workstation
- Large photo libraries
- Batch processing
- Lightroom workflow
- GPU acceleration

Android:

- Companion application
- Mobile photo selection
- Reference selection
- Preview

Core models and business logic must be shared.

---

## Development Priority

```text
1. Catalog and asset foundation
2. Preview and metadata
3. Photo Group model
4. Smart Culling
5. Reference Set and Style Profile
6. Reuse color_sync to resolve group intent per photo
7. Materialize per-photo Recipe/Edit Graph
8. Lightroom XMP mapping and sidecar writing
9. Direct export
9. Advanced AI assistance
```

---

## Non Destructive Rules

- Never modify RAW source files.
- Never generate unnecessary duplicate full-size files.
- Recipe is the editing decision source.
- XMP and export are outputs of Recipe.
- Never delete photos automatically.

---

## Phase 1 Acceptance

Input:

```text
RAW photo collection
```

Output:

```text
Catalog
Preview
Photo Groups
Reference foundation
Recipe
XMP
```

The result must continue working in Lightroom.

---

## Completion Criteria

A task is complete when:

- It improves photographer workflow.
- It reuses current architecture.
- It has implementation, tests and build validation.
- It moves Photo-Cake closer to semi-automatic personal editing workflow.


## Adaptive Batch Rule

For a Photo Group, preserve one shared style intent but resolve photo-specific adjustments from existing analysis. Reuse `color_sync`; do not introduce a second batch-style engine. Materialize one Recipe per target asset before XMP generation so differently exposed photos do not receive identical numeric corrections.


## Reference Style Execution Rule

Use the existing `ReferenceSet::color_intent_from_reference` path to turn a selected reference plus `StyleProfile` into `GroupColorIntent`. Then reuse `color_sync` to resolve individual photos and `Recipe::materialize_group` to create target-bound Recipes. Do not bypass this chain with blind preset copying.


## Grouping Execution Rule

Preserve the existing two-stage design: use `initial_group_raw_assets` for fast moment grouping, then `refine_group_by_similarity` for portrait/scene refinement. Promote semantic groups through `SemanticPhotoGroup::to_photo_group` before reference/style/Recipe work. Do not add a parallel scene-grouping subsystem.
