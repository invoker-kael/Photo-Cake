# Photo-Cake Luna Execution Guide

## Purpose

This document is the execution entry point for Luna.

Luna should read this folder before making implementation decisions.

Goal:

Build a usable personal semi-automatic photography workflow product, not a generic image converter.

---

# Execution Principle

Implement directly from requirements.

Do not expand scope without a blocking reason.

Priority:

1. Working product pipeline
2. Stable data model
3. Lightroom compatible workflow
4. Direct export capability
5. Advanced AI features

---

# Product Workflow

```
RAW Import
    |
    v
Metadata Extraction
    |
    v
Photo Analysis
    |
    v
Quality Score / Grouping
    |
    v
Recipe Generation
    |
    +-------------------+
    |                   |
    v                   v
XMP Sidecar         Direct Export
    |                   |
    v                   v
Lightroom          JPEG/TIFF
```

---

# Phase 1 Deliverable

Complete a working foundation.

Required:

- RAW file indexing
- Metadata database
- Recipe schema
- XMP sidecar generation
- Direct export framework
- Basic validation tests

Output must allow:

```
IMG.CR3
IMG.XMP
```

to be opened by Lightroom with adjustments available.

---

# Phase 2 Deliverable

Smart photo processing.

Implement:

- Photo quality analysis
- Blur detection
- Exposure analysis
- Face quality detection
- Similar photo grouping
- Best photo candidate selection

---

# Phase 3 Deliverable

Automated post-processing.

Implement:

- Reference look analysis
- AI Retouch Plan
- Batch recipe application

---

# Phase 4 Deliverable

Advanced RAW pipeline.

Implement only after previous phases are stable:

- RAW decoder
- GPU acceleration
- Full render engine

---

# Storage Rules

Default:

```
RAW + XMP
```

Do not create TIFF/JPEG intermediates automatically.

Large exports require explicit user request.

---

# Engineering Rules

- Keep original RAW untouched.
- Keep Recipe independent from output format.
- XMP and export must share the same Recipe.
- Avoid unnecessary architecture.
- Add tests for completed features.
- Fix implementation problems before adding new features.

---

# Completion Criteria

A phase is complete only when:

- Code implemented
- Tests added or updated
- Build passes
- User workflow is usable

The final product should support:

Import hundreds of RAW photos → automatic analysis → generate Lightroom XMP or export final images.
