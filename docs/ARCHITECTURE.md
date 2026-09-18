# Photo-Cake Architecture

## Scope

Photo-Cake is a single-user local-first AI photography workflow application.

The architecture is designed around professional RAW processing workflow, not a simple image converter.

## Core Pipeline

```text
RAW Catalog
    |
    v
Metadata Layer
    |
    v
Analysis Layer
    |
 +----------------+
 |                |
Quality       Similarity
Face          Scene
Focus         Exposure
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
 +-------------+
 |             |
 v             v
XMP          Export
```

## Data Model

Primary objects:

```text
Project
 |
Catalog
 |
Photo
 |
Photo Group
 |
Recipe
 |
Edit Graph
```

Photo Group is a first-class object because real photography sessions contain different scenes and lighting conditions.

## Non-destructive Editing

Original RAW files are immutable.

Stored information:

- Asset identity
- Metadata
- Preview cache
- Analysis results
- Grouping information
- Reference style information
- Recipe
- Edit Graph
- Export state

RAW is never modified.

## Recipe System

Recipe is shared by all outputs:

```text
Recipe
 |
 +------+
 |      |
XMP   Export
```

The same editing decision can continue in Lightroom or generate final images.

## Batch Processing

Large photo sets are processed as jobs.

Requirements:

- Checkpoint stages
- Resume after failure
- Isolate failed items
- Safe export
- No corruption of source files

## Local AI Architecture

Offline-first operations:

- Photo analysis
- Grouping
- Similarity search
- Portrait analysis
- Style analysis
- QA

Models and cache are versioned.

## Platform

Windows and Android share core logic.

Platform differences:

- File access
- Hardware acceleration
- UI interaction
- Packaging

## Future Expansion

Later phases may add:

- Advanced RAW processing
- GPU acceleration
- More advanced AI retouch planning

These must not block the core RAW + XMP workflow.
