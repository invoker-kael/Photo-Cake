# Photo-Cake Architecture

## Scope

Photo-Cake is a local-first semi-automatic photography workflow application.

The architecture follows the photographer workflow:

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
     XMP        Direct Export
```

The goal is to reduce repetitive editing work while keeping photographer control.

---

# Existing Code Reuse

The repository implementation remains the foundation.

```text
apps/
 ├── windows
 └── android

crates/
 ├── photo-core
 └── photo-inference
```

Extend existing modules before creating new systems.

---

# Core Layer

## photo-core

Shared photography workflow engine.

Responsibilities:

- catalog management
- RAW asset identity
- metadata
- preview
- photo grouping
- culling decisions
- reference sets
- Recipe/Edit Graph
- batch jobs
- XMP/export interfaces

---

# Photo Group Model

Photo Group is the main editing unit.

A group represents a real photography situation:

- travel scene
- portrait session
- family event
- landscape
- indoor/night photography

Group-level editing is preferred over isolated photo processing.

```text
Photo Assets
      |
      v
Photo Group
      |
      v
Recipe
      |
      v
Multiple Photos
```

---

# Recipe Driven Editing

Recipe is the single source of editing decisions.

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
XMP / Export
```

Recipe contains future editable decisions such as:

- exposure
- white balance
- contrast
- color preference
- skin tone preference
- lighting style

RAW files remain immutable.

---

# AI Layer

## photo-inference

Responsible for AI capabilities:

Current:

- image analysis
- similarity detection
- segmentation
- local model execution

Future:

- quality scoring
- smart culling
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

# Development Priority

Implement the photographer workflow first:

```text
Catalog
 -> Preview
 -> Photo Group
 -> Culling
 -> Reference Style
 -> Recipe
 -> XMP
 -> Export
 -> Advanced AI
```

Do not prioritize:

- replacing Lightroom
- full RAW engine replacement
- cloud editing service
- Lightroom plugin

Foundation remains:

```text
RAW -> Recipe -> XMP -> Lightroom
```
