# Photo-Cake Luna Execution Specification

## Purpose

This is the execution authority for Luna.

Goal:

Build a local-first semi-automatic photography workflow product.

Photo-Cake assists photographers by reducing repetitive editing while keeping RAW files, Lightroom compatibility, and user control.

Existing code must be reused whenever it matches the workflow. Do not rebuild working modules without technical necessity.

## Execution Order

1. Read this document
2. Read product requirements
3. Read architecture
4. Inspect current code and reusable modules
5. Implement the smallest complete increment
6. Test
7. Build verification
8. Continue current phase

## Existing Code Reuse Strategy

The current architecture already provides reusable foundations.

Reuse:

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

Extend existing modules instead of creating parallel implementations.

## Platform Strategy

Photo-Cake supports two platforms.

Windows:

- Primary workstation
- Large RAW collections
- Batch processing
- Lightroom workflow
- XMP generation
- GPU acceleration

Android:

- Mobile workflow companion
- Photo selection
- Reference image selection
- Lightweight analysis
- Shared core logic

Business logic must remain reusable through shared core modules.

## Product Workflow

```text
RAW Import
 |
Metadata
 |
Smart Culling
 |
Photo Grouping
 |
Reference Style Analysis
 |
Recipe Generation
 |
Preview Review
 |
+-------------+
|             |
XMP           Export
|
Lightroom     JPEG/TIFF
```

## Execution Rules

- Preserve original RAW files.
- Use RAW + XMP as the default workflow.
- Do not create unnecessary large files.
- Keep direct export available.
- Use one Recipe model for XMP and export.
- Prefer working user features over architecture expansion.
- Keep local-first operation.
- Do not add cloud/account/plugin features unless required.
- Reuse existing working code before introducing new frameworks.

## Current Product Model

Priority order:

```text
Culling
  -> Grouping
  -> Reference Style
  -> Recipe
  -> Review
  -> Output
```

## Phase 1 Completion Target

Build the foundation required for a real photography workflow:

- RAW indexing
- Metadata extraction
- Asset management
- Preview foundation
- Photo Group foundation
- Recipe schema
- XMP generation
- Export framework

Acceptance:

Input:

```text
IMG.CR3
```

Output:

```text
IMG.XMP
```

Lightroom must read the RAW and show adjustments.

## Development Restrictions

Do not prioritize:

- Full RAW replacement engine
- Lightroom plugin
- Cloud AI service
- Complex social/account features
- Rewriting existing reusable modules

These belong to later roadmap phases.

## Completion Criteria

A feature is complete when:

- Implementation exists
- Existing architecture is respected
- Tests pass
- Build succeeds
- User photography workflow is improved
