# Photo-Cake Luna Execution Specification

## Purpose

This document is the execution authority for Luna.

Photo-Cake is a local-first semi-automatic photography workflow assistant.

The target workflow is:

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
Reference Style
    |
Recipe
    |
XMP / Export
```

The goal is not to replace Lightroom. The goal is to reduce repetitive photographer work while preserving RAW files, Lightroom compatibility, and user control.

---

## Existing Code First

Reuse current implementation before creating new systems.

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
 +-- recipe/edit model
 +-- export

photo-inference
 |
 +-- analysis
 +-- similarity
 +-- segmentation
 +-- future style/culling models
```

Rules:

- Extend existing modules.
- Keep workflow logic platform independent.
- Do not rebuild working capabilities.
- Do not make export the source of truth.

---

## Platform Strategy

Windows:

- Main RAW workstation
- Large photo libraries
- Batch processing
- Lightroom XMP workflow
- GPU acceleration

Android:

- Companion workflow
- Photo selection
- Reference photo selection
- Preview

Shared models and workflow logic must remain in core layers.

---

## Development Priority

```text
1. Catalog and asset foundation
2. Preview and metadata
3. Photo Group model
4. Smart Culling foundation
5. Reference Style model
6. Recipe/Edit Graph
7. XMP generation
8. Direct export
```

---

## Non Destructive Rules

- RAW files are immutable.
- XMP is the primary Lightroom bridge.
- Recipe is the unified editing decision format.
- Export and Lightroom use the same Recipe.
- Never delete photos automatically.
- AI only suggests decisions unless explicitly approved.

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
Grouping foundation
Recipe
XMP
```

The user can continue editing in Lightroom.

---

## Completion Criteria

A task is complete when:

- It improves photographer workflow.
- Existing architecture is reused.
- Implementation exists.
- Tests pass.
- Build succeeds.
