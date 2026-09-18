# Photo-Cake Luna Execution Specification

## Purpose

This is the execution authority for Luna.

Goal:

Build a local-first semi-automatic photography workflow product.

Photo-Cake assists photographers by reducing repetitive editing while keeping RAW files, Lightroom compatibility, and user control.

## Execution Order

1. Read this document
2. Read product requirements
3. Read architecture
4. Inspect current code
5. Implement the smallest complete increment
6. Test
7. Build verification
8. Continue current phase

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

These belong to later roadmap phases.

## Completion Criteria

A feature is complete when:

- Implementation exists
- Tests pass
- Build succeeds
- User photography workflow is improved
