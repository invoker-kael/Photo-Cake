# Photo-Cake Architecture

## Scope

Photo-Cake is a local-first AI photography workflow application.

The architecture follows the real photographer workflow instead of a generic image processing pipeline.

```text
RAW Collection
      |
      v
Catalog
      |
      v
Metadata + Preview
      |
      v
Analysis
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
      +------------+
      |            |
      v            v
     XMP        Export
```

---

# Existing Code Reuse

The current repository is the foundation.

```text
apps/
 ├── windows
 └── android

crates/
 ├── photo-core
 └── photo-inference
```

Do not create parallel implementations when existing modules can be extended.

---

# Core Layer

## photo-core

Shared photography workflow engine.

Responsibilities:

- catalog management
- RAW asset model
- metadata
- photo groups
- preview system
- Recipe model
- Edit Graph
- batch jobs
- XMP/export interfaces

This layer contains workflow rules shared by platforms.

---

## photo-inference

AI capability layer.

Current capabilities:

- image analysis
- similarity detection
- segmentation
- local model execution

Future extensions:

- photo quality scoring
- culling assistance
- style extraction
- editing suggestions

Models must remain replaceable.

---

# Platform Architecture

## Windows

Primary workstation.

Responsibilities:

- large RAW collections
- batch processing
- GPU acceleration
- Lightroom workflow
- XMP generation
- direct export

## Android

Mobile companion.

Responsibilities:

- photo selection
- reference photo selection
- preview
- lightweight analysis

Business logic stays in shared core modules.

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

Photo Group is a first-class object because real photography sessions contain different scenes, lighting, and editing requirements.

---

# Non-Destructive Editing

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

- RAW remains immutable.
- XMP is the Lightroom bridge.
- Export always uses the same Recipe model.

---

# Development Priority

Development should maximize photographer value:

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

Do not block early workflow delivery on:

- full RAW replacement engine
- Lightroom plugin
- cloud service

---

# Long Term Expansion

Later phases may add:

- advanced RAW processing
- personal style profile
- AI retouch planning
- advanced GPU acceleration

Foundation remains:

```text
RAW -> Recipe -> XMP -> Lightroom
```
