# Photo-Cake Luna Execution Specification

## Purpose

This document is the execution authority for Luna.

Photo-Cake is a local-first semi-automatic photography workflow assistant.

The goal is not to replace Lightroom or create a generic image editor. The goal is to reduce repetitive photographer work while preserving RAW, Lightroom compatibility, and user control.

Existing code must be reused whenever it matches the workflow. Do not rebuild working modules without technical necessity.

---

## Execution Order

1. Read this document
2. Read product requirements
3. Read architecture
4. Inspect existing code and reusable modules
5. Implement the smallest complete photography workflow increment
6. Test
7. Build verification
8. Continue current phase

---

## Existing Code First

The current repository already contains reusable foundations.

Primary reuse targets:

```text
photo-core
 |
 +-- catalog
 +-- importer
 +-- raw
 +-- metadata
 +-- grouping
 +-- analysis
 +-- preview
 +-- recipe/edit model
 +-- export

photo-inference
 |
 +-- local models
 +-- embedding
 +-- segmentation
 +-- image analysis
```

Rules:

- Extend existing modules first.
- Avoid parallel implementations.
- Keep shared logic platform independent.

---

## Platform Strategy

### Windows

Primary photography workstation:

- Large RAW collections
- Batch processing
- Lightroom workflow
- XMP generation
- GPU acceleration

### Android

Photography companion:

- Photo selection
- Reference photo selection
- Preview
- Lightweight analysis

Android must reuse shared workflow models instead of duplicating processing logic.

---

## Photography Workflow Priority

The product follows this order:

```text
RAW Import
 |
Catalog
 |
Metadata / Preview
 |
Smart Culling
 |
Photo Grouping
 |
Reference Style Analysis
 |
Recipe Generation
 |
Review
 |
+-------------+
|             |
XMP           Export
|
Lightroom     JPEG/TIFF
```

---

## Core Rules

- RAW files are immutable.
- RAW + XMP is the primary workflow.
- Recipe is the unified editing decision model.
- XMP and export must use the same Recipe.
- Do not create unnecessary large intermediate files.
- Local-first operation is preferred.
- AI provides editing decisions, not destructive replacement.

---

## Current Development Focus

Priority order:

```text
1. Catalog foundation
2. RAW asset management
3. Preview foundation
4. Photo Group model
5. Smart Culling foundation
6. Recipe model
7. XMP output
8. Direct export
```

---

## Phase 1 Acceptance

A complete minimal photographer workflow must exist:

Input:

```text
IMG.CR3
```

Process:

```text
Import
Metadata
Preview
Group
Recipe
```

Output:

```text
IMG.XMP
```

Lightroom can open the RAW file and apply the generated adjustments.

---

## Do Not Prioritize Early

- Full RAW replacement engine
- Lightroom database modification
- Lightroom plugin
- Cloud AI service
- Social/account features
- Rewriting reusable code

---

## Completion Criteria

A task is complete when:

- It improves the photographer workflow.
- Existing architecture is respected.
- Implementation exists.
- Tests pass.
- Build succeeds.
