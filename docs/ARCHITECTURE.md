# Photo-Cake Architecture

## Scope

Photo-Cake is a local-first semi-automatic photography workflow assistant.

The architecture follows the real photographer workflow:

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

The goal is reducing repetitive editing while keeping RAW files, Lightroom compatibility and photographer control.

---

# Existing Code Reuse

The existing workspace remains the foundation.

```text
apps/
 ├── windows
 └── android

crates/
 ├── photo-core
 └── photo-inference
```

Extend existing modules before creating parallel systems.

---

# Core Workflow Objects

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

A group of photos is the main editing unit, not an isolated image.

---

# Reference Driven Editing

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

The system learns photographer preference from selected references instead of replacing decisions.

---

# Recipe and Lightroom Bridge

Recipe is the source of editing decisions.

RAW files remain immutable.

Flow:

```text
Recipe
  |
  v
XMP Mapping
  |
  v
Lightroom
```

Direct export uses the same Recipe model.

Initial supported adjustments:

- exposure
- contrast
- highlights
- shadows
- temperature
- tint
- saturation

Future:

- HSL
- tone curve
- skin tone preference
- personal style profile

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
 -> XMP Mapping
 -> Export
 -> Advanced AI
```

Foundation:

```text
RAW -> Reference -> Recipe -> XMP -> Lightroom
```
