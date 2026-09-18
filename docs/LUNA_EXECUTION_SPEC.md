# Photo-Cake Luna Execution Specification

## Purpose

This is the primary execution document for Luna.

Luna must read this document before implementation.

Goal:

Build a personal semi-automatic photography workflow product.

Photo-Cake is not a Lightroom replacement. It prepares intelligent edits and keeps Lightroom compatibility.

---

# Execution Order

1. Read this document
2. Read product requirements
3. Read architecture documents
4. Inspect current implementation
5. Implement the smallest complete working increment
6. Add tests
7. Verify build
8. Continue current phase

---

# Product Workflow

```
RAW Import
    |
Metadata Extraction
    |
Photo Analysis
    |
Quality Assessment / Grouping
    |
Recipe Generation
    |
+----------------+
|                |
XMP Output       Direct Export
|                |
Lightroom        JPEG/TIFF
```

---

# Core Rules

- Original RAW files are immutable.
- RAW + XMP is the default workflow.
- Do not create large intermediate files automatically.
- Direct export remains available.
- XMP and export must use the same Recipe model.
- Prefer working product features over architecture expansion.
- Do not implement unnecessary cloud, multi-user, or plugin features.

---

# Phase 1 - Foundation

Deliver:

- RAW indexing
- Metadata extraction
- Asset database
- Recipe schema
- XMP sidecar generation
- Direct export framework
- Validation tests

Completion:

A user can process:

```
IMG.CR3
IMG.XMP
```

and open the result in Lightroom.

---

# Phase 2 - Smart Analysis

Deliver:

- Blur detection
- Exposure analysis
- Face quality analysis
- Similar photo grouping
- Best photo selection

---

# Phase 3 - AI Processing

Deliver:

- Reference look analysis
- AI Retouch Plan
- Batch recipe application

---

# Phase 4 - Advanced RAW Pipeline

Only after previous phases are stable:

- RAW decoder
- GPU acceleration
- Full render engine

---

# Output Requirements

Primary:

```
RAW + XMP
```

Secondary:

```
JPEG/TIFF export on demand
```

---

# Completion Criteria

A feature is complete only when:

- Code implemented
- Tests added or updated
- Build passes
- User workflow works
