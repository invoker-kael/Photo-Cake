# Photo-Cake Luna Execution Specification

## Purpose

This is the primary execution document for Luna.

Luna must read this document before implementation.

Goal:

Build a personal semi-automatic photography workflow product.

Photo-Cake is not a Lightroom replacement. It prepares intelligent edits, keeps original RAW files, and provides Lightroom-compatible workflows plus optional direct export.

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
AI Culling
    |
Photo Grouping
    |
Reference Photo Selection
    |
Style Analysis
    |
Recipe Generation
    |
Edit Graph
    |
Preview / Human Approval
    |
+----------------+
|                |
XMP Output       Direct Export
|                |
Lightroom        JPEG/TIFF
```

---

# Core Product Rules

- Original RAW files are immutable.
- RAW + XMP is the default workflow.
- Do not create large intermediate files automatically.
- Direct export remains available on demand.
- XMP and export must use the same Recipe model.
- Prefer working product features over architecture expansion.
- Keep the application local-first.
- Do not add unnecessary cloud, account, multi-user, or plugin features.

---

# Intelligent Culling Workflow

Photo-Cake should reduce manual photo selection work before editing.

Analyze:

- Blur and focus quality
- Exposure problems
- Closed eyes
- Facial expression quality
- Duplicate and burst photos
- Obvious failed compositions

Output:

```
Photo
 |
Quality Score
 |
Keep / Review / Reject suggestion
```

Culling assists the user and must not destroy original files.

---

# Photo Group Model

Groups are first-class objects.

Workflow:

```
Photo
 |
 v
Photo Group
 |
 v
Recipe
```

Examples:

- Travel day
- Landscape
- Portrait
- Indoor lighting
- Night scene

Different groups can receive different Recipes.

---

# Reference Photo Workflow

Photo-Cake must support a reference-based workflow.

Purpose:

Allow the user to select preferred edited photos and apply visual intent to similar photos.

Flow:

```
Reference Photos
      |
      v
Style Analysis
      |
      v
Generate Recipe
      |
      v
Apply to Similar Photo Group
```

Analyze:

- Exposure style
- White balance
- Contrast
- Color characteristics
- Skin tone preference
- Lighting style

The goal is consistent batch editing, not copying pixels.

---

# Non-destructive Editing Model

Editing must be represented as editable data.

Architecture:

```
RAW
 |
 v
Edit Graph
 |
 v
Recipe
 |
 +------------+
 |            |
 v            v
XMP        Render Export
```

Do not directly modify original files.

Future AI features must generate editable intent instead of destructive image replacement.

---

# Human Approval Workflow

Photo-Cake is semi-automatic, not fully automatic.

Required workflow:

```
AI Processing
 |
Preview Comparison
 |
User Approval
 |
Apply XMP / Export
```

The user must be able to review results before final output.

---

# Personal Style Learning

Future versions may learn user preferences from:

- Accepted Recipes
- User modifications
- Common adjustment patterns
- Reference selections

The goal is improving personal workflow efficiency, not replacing user decisions.

---

# Phase 1 - Foundation

Deliver:

- RAW file indexing
- Metadata extraction
- Asset database
- Recipe schema
- XMP sidecar generation
- Direct export framework
- Validation tests

User acceptance:

Given:

```
IMG.CR3
```

Photo-Cake can generate:

```
IMG.XMP
```

and Lightroom can open the RAW with adjustments available.

---

# Phase 2 - Smart Analysis

Deliver:

- Blur detection
- Exposure analysis
- Face quality analysis
- Similar photo grouping
- Best photo selection
- Scene-based grouping
- Intelligent culling

---

# Phase 3 - AI Processing

Deliver:

- Reference look analysis
- AI Retouch Plan
- Batch recipe application
- Portrait enhancement planning
- Lighting adjustment planning

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

Never generate large exports automatically.

---

# Completion Criteria

A feature is complete only when:

- Code implemented
- Tests added or updated
- Build passes
- User workflow works
