# Photo-Cake Architecture

## Scope

Photo-Cake is a single-user local-first AI photography workflow application.

The architecture follows a professional photographer workflow:

RAW collection → selection → grouping → style learning → editing decision → Lightroom/export.

It is not a RAW converter replacement and not a Lightroom replacement.

The system generates editable editing intent while preserving original files.

---

# Core Photography Pipeline

```text
RAW Catalog
    |
    v
Metadata Extraction
    |
    v
Photo Analysis
    |
 +------------------------------+
 |              |               |
 v              v               v
Quality      Similarity       Scene
Score        Detection        Detection
 |
 v
Smart Culling
 |
 v
Photo Group Layer
 |
 v
Reference Style Layer
 |
 v
Recipe / Edit Graph
 |
 +----------------+
 |                |
 v                v
XMP Output     Direct Export
 |
 v
Lightroom
```

---

# Reusable Code Architecture

Existing implementation should be reused where it matches the workflow. Do not rewrite working foundations without reason.

The repository is designed as:

```text
apps/
 ├── windows
 └── android

crates/
 ├── photo-core
 └── photo-inference
```

Responsibilities:

## photo-core

Shared platform-independent logic:

- project model
- catalog
- asset identity
- metadata
- photo groups
- recipe model
- edit graph
- job state
- import/export interfaces

This is the primary reusable layer.

## photo-inference

AI capability layer:

- image analysis
- similarity
- classification
- style analysis
- future culling models

Models must remain replaceable.

## apps/windows

Desktop workflow:

- large RAW library management
- batch processing
- Lightroom-oriented workflow
- GPU acceleration

## apps/android

Mobile workflow:

- photo selection
- preview
- lightweight analysis
- mobile capture/import

Android should not duplicate core logic.

---

# Core Data Model

Primary objects:

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

Important design rule:

Photo Group is a first-class object.

A photography session contains different lighting and scenes. A single global adjustment is not sufficient.

---

# Photo Selection Architecture

Culling happens before editing.

The system evaluates:

- Sharpness
- Focus quality
- Blur
- Closed eyes
- Facial expression
- Duplicate burst images
- Exposure problems

Output:

```text
Photo
 |
Quality Score
 |
Keep / Review / Reject suggestion
```

Culling only provides recommendations. Original files are never deleted automatically.

---

# Reference Style Architecture

Reference photos are the source of visual intent.

Flow:

```text
Reference Photos
        |
        v
Style Analysis
        |
        v
Recipe Template
        |
        v
Apply to Photo Group
```

Analyzed attributes:

- Exposure preference
- White balance
- Contrast
- Color tone
- Skin tone preference
- Lighting style

The goal is consistent batch editing, not pixel copying.

---

# Non-destructive Editing

Original RAW files are immutable.

Architecture:

```text
RAW
 |
 v
Edit Graph
 |
 v
Recipe
 |
 +-------------+
 |             |
 v             v
XMP          Export
```

RAW is never modified.

---

# Recipe System

Recipe is the unified editing decision format.

All output paths use the same Recipe:

```text
Recipe
 |
 +------+
 |      |
XMP   Export
```

This allows Lightroom continuation and direct export without changing workflow design.

---

# Batch Processing

Large photo sessions are processed as jobs.

Requirements:

- Progress tracking
- Checkpoint stages
- Resume after interruption
- Failed item isolation
- Safe export
- Source protection

---

# Local AI Architecture

Local-first operations:

- Quality analysis
- Similarity search
- Scene classification
- Portrait analysis
- Style analysis
- Recipe suggestion

Models and caches must be versioned.

Large generated files should not be created unless explicitly requested.

---

# Platform Boundary

Windows and Android share the same workflow model.

Platform-specific code must stay isolated:

- File access
- Hardware acceleration
- UI
- Packaging

Business logic belongs in shared crates whenever possible.

---

# Future Expansion

Later phases may add:

- Advanced RAW processing
- GPU acceleration
- AI retouch planning
- Personal style learning

These must not block the core workflow:

RAW → Recipe → XMP → Lightroom
