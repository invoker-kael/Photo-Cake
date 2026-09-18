# Photo-Cake Product Requirements

## Product Position

Photo-Cake is a local-first semi-automatic photography workflow assistant.

The goal is to reduce repetitive work in large RAW photo processing while preserving a professional Lightroom workflow.

Photo-Cake is not:

- a Lightroom replacement
- a full RAW engine replacement
- a cloud editing service

The product follows a Pixel-Cake style workflow: automate repetitive selection and editing decisions while keeping photographer control.

---

# Main Photography Workflow

```text
RAW Import
    |
    v
Catalog
    |
    v
Preview + Metadata
    |
    v
Smart Culling
    |
    v
Photo Grouping
    |
    v
Reference Style Learning
    |
    v
Adaptive Group Sync
    |
    v
Per-photo Recipe Generation
    |
    v
Review
    |
    +----------------+
    |                |
    v                v
XMP Lightroom    Direct Export
```

---

# Core Principles

- Original RAW files are never modified.
- RAW + XMP is the primary workflow.
- Recipe is the single source of editing decisions.
- Export is optional and generated from Recipe.
- Avoid unnecessary TIFF/JPEG intermediate storage; the normal handoff is the original RAW plus a small XMP sidecar.
- Local-first operation.
- Existing code is extended before creating new systems.

---

# Existing Code Alignment

Photo-Cake should evolve from the current implementation.

```text
photo-core
 |
 +-- catalog
 +-- importer
 +-- raw
 +-- metadata
 +-- grouping
 +-- preview
 +-- recipe/edit model
 +-- export

photo-inference
 |
 +-- image analysis
 +-- similarity
 +-- segmentation
 +-- future culling/style models
```

---

# Photography Features

## Catalog and Import

Purpose: understand the user's photo collection.

Required:

- RAW indexing
- metadata extraction
- asset identity
- preview generation

## Smart Culling

Purpose: reduce the number of photos requiring manual review.

Analyze:

- focus quality
- blur
- closed eyes
- expression quality
- duplicate burst photos
- exposure issues

The system suggests decisions. It never deletes originals automatically.

## Photo Grouping

Photo Group is the main editing unit.

Examples:

- travel scenes
- portraits
- landscapes
- indoor family photos
- night photos

Different groups can have different Recipes.

## Reference Style Workflow

The user provides preferred photos.

```text
Reference Photos
        |
        v
Style Analysis
        |
        v
Recipe
        |
        v
Apply to Group
```

The system learns:

- color preference
- exposure preference
- contrast
- white balance
- skin tone style
- lighting preference

## Output

Primary:

```text
RAW + same-basename XMP -> Lightroom
```

Secondary:

```text
Recipe -> Direct Export
```

---

# User Success Criteria

A photographer can process hundreds of RAW files by:

1. Importing photos
2. Automatically organizing and analyzing them
3. Selecting a preferred style
4. Generating editing decisions
5. Reviewing results
6. Continuing in Lightroom or exporting directly


## Adaptive Batch Editing

A group must not receive a blind copy of one photo's values. The reference defines the desired look; Photo-Cake resolves that intent against each photo's measured exposure/white-balance state, then creates a target-bound Recipe and XMP sidecar for that photo.
