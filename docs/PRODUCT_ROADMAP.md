# Photo-Cake Product Roadmap

## Product Goal

Photo-Cake is a local-first semi-automatic photography workflow assistant.

It reduces repetitive editing work for photographers while preserving RAW files, Lightroom workflow, and user decisions.

The product direction is:

```text
Large RAW Collection
        |
        v
Selection
        |
        v
Organization
        |
        v
Reference-based Editing
        |
        v
Recipe
        |
        v
XMP / Export
```

---

# Development Strategy

Existing code is the foundation.

Development should improve the current workflow instead of rebuilding the application around new frameworks.

Main reusable layers:

```text
photo-core
 - catalog
 - assets
 - metadata
 - grouping
 - recipe
 - export

photo-inference
 - analysis
 - similarity
 - segmentation
 - future AI models
```

---

# Phase 1 - Photography Workflow Foundation

Goal: create a complete non-destructive workflow foundation.

Priority:

- RAW indexing
- catalog management
- metadata extraction
- preview foundation
- asset identity
- Photo Group foundation
- Recipe schema
- XMP generation
- export framework

Acceptance:

A RAW photo can generate an XMP sidecar readable by Lightroom.

---

# Phase 2 - Photo Organization

Goal: reduce manual management of large photo collections.

Features:

- similarity detection
- duplicate detection
- burst grouping
- scene grouping
- event grouping
- preview comparison

---

# Phase 3 - Smart Culling

Goal: reduce the number of photos requiring manual review.

Features:

- sharpness analysis
- focus quality
- blur detection
- closed eyes
- expression quality
- exposure problems
- best photo recommendation

AI only recommends. Original photos are never deleted automatically.

---

# Phase 4 - Reference Style Workflow

Goal: learn the photographer's preferred editing style.

Features:

- reference photo selection
- style analysis
- color preference
- exposure preference
- skin tone preference
- group Recipe generation

---

# Phase 5 - Semi Automatic Editing

Goal: generate editable professional adjustments.

Features:

- exposure suggestions
- white balance suggestions
- portrait enhancement planning
- lighting adjustment planning
- batch Recipe application

---

# Phase 6 - Advanced Workflow

Features:

- batch review
- before/after comparison
- exception handling
- improved export
- deeper Lightroom workflow support

---

# Long Term Direction

Possible future improvements:

- advanced RAW processing
- GPU acceleration
- personal style learning
- more AI-assisted editing

These must not block the core workflow:

```text
RAW
 ->
Recipe
 ->
XMP
 ->
Lightroom
```

---

# Early Non-goals

Do not prioritize:

- cloud dependency
- multi-user platform
- enterprise workflow
- destructive automatic editing
- Lightroom database replacement
- full RAW converter replacement
