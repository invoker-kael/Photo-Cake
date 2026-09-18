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
Reference Set
    |
Style Profile
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
- style profile storage
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
Style Profile
    |
Recipe
    |
XMP / Export
```

---

# Reference Driven Editing

Reference photos are the source of photographer preference.

```text
Favorite Photos
        |
        v
Reference Set
        |
        v
Style Profile
        |
        v
Recipe
        |
        v
XMP / Export
```

The system should learn from selected photos, not replace photographer decisions.

---

# Recipe Driven Editing

Recipe is the unified editing decision model.

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

RAW files remain immutable.

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

# Development Priority

```text
Catalog
 -> Preview
 -> Photo Group
 -> Culling
 -> Reference Set
 -> Style Profile
 -> Recipe
 -> XMP
 -> Export
 -> Advanced AI
```

Foundation:

```text
RAW -> Reference -> Recipe -> XMP -> Lightroom
```
