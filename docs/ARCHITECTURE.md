# Photo-Cake Architecture

## Scope

Photo-Cake is a local-first AI photography workflow application.

The architecture follows a photographer workflow:

```text
RAW collection
    |
    v
Catalog
    |
    v
Analysis
    |
    v
Selection
    |
    v
Grouping
    |
    v
Style Learning
    |
    v
Recipe
    |
    v
XMP / Export
```

It assists editing decisions while preserving professional RAW workflow.

---

# Reuse Existing Implementation

The existing repository structure is the foundation.

```text
apps/
 ├── windows
 └── android

crates/
 ├── photo-core
 └── photo-inference
```

Do not rewrite working foundations without technical reason.

---

# Core Pipeline

```text
RAW Catalog
    |
    v
Metadata Extraction
    |
    v
Photo Analysis
    |
 +----------------+
 |                |
 v                v
Quality        Similarity
Score          Detection
 |
 v
Smart Culling
 |
 v
Photo Group
 |
 v
Reference Style
 |
 v
Recipe / Edit Graph
 |
 +-------------+
 |             |
 v             v
XMP          Export
```

---

# Shared Core Layer

## photo-core

Platform-independent workflow logic.

Responsibilities:

- catalog
- asset model
- RAW metadata
- photo groups
- preview foundation
- recipe model
- edit graph
- batch jobs
- export interfaces

This layer should contain photography workflow rules.

---

## photo-inference

AI capability layer.

Responsibilities:

Current:

- image analysis
- similarity
- segmentation
- local model execution

Future:

- culling models
- quality scoring
- style extraction
- editing suggestions

Models remain replaceable.

---

# Platform Design

## Windows

Primary editing workstation.

Responsibilities:

- large RAW collections
- batch processing
- Lightroom workflow
- XMP generation
- GPU acceleration

## Android

Mobile assistant.

Responsibilities:

- photo selection
- reference photo selection
- preview
- lightweight analysis

Core workflow logic stays shared.

---

# Data Model

```text
Project
 |
Catalog
 |
Photo Asset
 |
Photo Group
 |
Reference Set
 |
Recipe
 |
Edit Graph
 |
Output Job
```

Photo Group is a first-class object because real photography sessions contain different scenes and lighting conditions.

---

# Non-destructive Editing

```text
RAW
 |
Edit Graph
 |
Recipe
 |
+----------+
|          |
XMP      Export
```

Rules:

- RAW files are immutable.
- XMP is the primary Lightroom bridge.
- Export uses the same Recipe model.

---

# Development Priority

Priority is user photography value:

```text
Catalog
 -> Preview
 -> Grouping
 -> Culling
 -> Recipe
 -> XMP
 -> Export
 -> Advanced AI
```

Do not block the workflow on:

- full RAW engine replacement
- Lightroom plugin
- cloud service

---

# Future Expansion

Later:

- advanced RAW processing
- personal style profile
- AI retouch planning
- advanced GPU acceleration

The foundation remains:

```text
RAW -> Recipe -> XMP -> Lightroom
```
