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

Examples:

- Travel daytime
- Landscape
- Indoor family photos
- Portrait
- Night scene

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

Stored information:

- Asset identity
- Metadata
- Preview cache
- Analysis results
- Grouping information
- Reference style information
- Recipe
- Edit Graph
- Export status

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

This allows:

- Lightroom continuation
- Direct JPEG/TIFF export
- Future editing features

without changing the workflow model.

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

Windows and Android share workflow concepts.

Platform differences:

- File access
- Hardware acceleration
- UI
- Packaging

Core logic should remain reusable.

---

# Future Expansion

Later phases may add:

- Advanced RAW processing
- GPU acceleration
- AI retouch planning
- More intelligent personal style learning

These must not block the core workflow:

RAW → Recipe → XMP → Lightroom
