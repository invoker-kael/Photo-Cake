# Photo-Cake Product Requirements

## Product Position

Photo-Cake is a local-first semi-automatic photography workflow assistant.

It is designed for photographers who process large RAW collections and want to reduce repetitive editing while keeping professional control.

It is not:

- a Lightroom replacement
- a full RAW converter replacement
- a cloud AI editing service

Primary workflow:

```text
RAW Photos
    |
    v
Import / Catalog
    |
    v
Metadata + Preview
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
Recipe Generation
    |
    v
Review
    |
    +----------------+
    |                |
    v                v
XMP to Lightroom   Direct Export
```

---

# Core Principles

- Preserve original RAW files.
- RAW + XMP is the primary professional workflow.
- Recipe is the single editing decision format.
- Export is optional, not the source of truth.
- Avoid unnecessary large intermediate files.
- Local-first operation.
- Reuse existing core capabilities before creating new systems.

---

# Existing Code Alignment

Photo-Cake should extend the current architecture instead of rebuilding it.

Reusable foundations:

```text
photo-core
    |
    +-- catalog
    +-- importer
    +-- raw
    +-- grouping
    +-- preview
    +-- recipe/edit model
    +-- export

photo-inference
    |
    +-- analysis
    +-- similarity
    +-- segmentation
    +-- future culling/style models
```

---

# Photography Workflow

## 1. Import and Catalog

The system first understands the photo collection.

Required:

- RAW indexing
- metadata extraction
- asset identity
- preview generation

---

## 2. Smart Culling

Before editing, reduce manual selection work.

Analyze:

- focus quality
- blur
- closed eyes
- expression quality
- duplicate burst photos
- exposure problems

Output:

```text
Keep
Review
Reject suggestion
```

Never delete originals automatically.

---

## 3. Photo Grouping

Photo Group is the main editing unit.

Examples:

- travel day
- landscape
- portrait
- indoor family photos
- night scenes

A recipe applies to a group instead of blindly applying one style to all photos.

---

## 4. Reference Style Workflow

Users select preferred photos as style references.

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
Apply to Similar Group
```

Analyze editing intent:

- exposure
- white balance
- contrast
- color tone
- skin tone preference
- lighting style

---

## 5. Output Workflow

Primary:

```text
RAW + XMP
```

Secondary:

```text
RAW + Recipe + Direct Export
```

Lightroom remains the professional continuation workflow.

---

# User Goal

A user should be able to process hundreds of RAW photos:

1. Import photos
2. Automatically analyze and group photos
3. Select or learn preferred style
4. Generate editing decisions
5. Review results
6. Continue in Lightroom or export
