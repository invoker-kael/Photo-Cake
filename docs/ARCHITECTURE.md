# Photo-Cake Architecture

## Scope

Photo-Cake is a local-first semi-automatic photography workflow application.

The architecture follows the photographer workflow:

```text
RAW Collection
    |
Catalog
    |
Metadata + Preview
    |
Smart Culling
    |
Photo Group
    |
Reference Style
    |
Recipe / Edit Graph
    |
+----------+
|          |
XMP     Direct Export
```

The goal is reducing repetitive editing while keeping photographer control.

---

# Existing Code Reuse

Current repository structure remains the foundation:

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

# photo-core

Shared photography workflow engine.

Responsibilities:

- catalog
- RAW asset identity
- metadata
- preview
- photo grouping
- culling decisions
- reference sets
- Recipe/Edit Graph
- batch jobs
- XMP/export interfaces

Current workflow objects:

```text
Photo Asset
    |
Photo Group
    |
Reference Set
    |
Recipe
    |
XMP / Export
```

---

# Recipe Driven Editing

Recipe is the unified editing decision model.

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

Recipe stores reusable adjustments instead of modifying RAW files.

Initial adjustments:

- exposure
- contrast
- highlights
- shadows
- temperature
- tint
- saturation

Future extensions:

- HSL
- tone curve
- skin tone preference
- personal style profile

---

# AI Layer

## photo-inference

Responsible for:

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

---

# Platform Architecture

## Windows

Primary workstation:

- RAW collections
- batch processing
- GPU acceleration
- Lightroom workflow
- XMP generation
- direct export

## Android

Mobile companion:

- photo selection
- reference selection
- preview
- lightweight analysis

Business logic remains in shared core modules.

---

# Development Priority

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

Foundation:

```text
RAW -> Recipe -> XMP -> Lightroom
```
