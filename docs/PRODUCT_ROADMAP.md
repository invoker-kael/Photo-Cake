# Photo-Cake Product Roadmap

## Product Goal

Photo-Cake is a local-first semi-automatic AI photography workflow assistant.

It reduces repetitive Lightroom editing work while preserving a professional RAW workflow.

Primary workflow:

```
RAW Import
  ↓
Smart Culling
  ↓
Photo Grouping
  ↓
Reference Style Learning
  ↓
Recipe Generation
  ↓
Review
  ↓
XMP / Direct Export
```

## Development Order

### Phase 1 - Photography Workflow Foundation

Goal: establish a complete non-destructive workflow.

- RAW indexing
- metadata extraction
- project/catalog foundation
- preview foundation
- asset identity
- Recipe schema
- XMP sidecar generation
- direct export framework

Acceptance:

A RAW file can generate an XMP sidecar that Lightroom can read.

---

### Phase 2 - Smart Photo Organization

Goal: reduce manual photo management.

- similarity detection
- duplicate detection
- burst grouping
- scene grouping
- event grouping
- preview comparison

---

### Phase 3 - Intelligent Selection

Goal: reduce the number of photos requiring review.

- blur detection
- focus quality
- closed eyes
- expression quality
- exposure issues
- best photo recommendation

AI assists selection; original files are never removed automatically.

---

### Phase 4 - Reference Driven Editing

Goal: reproduce user's preferred photography style.

- reference photo selection
- style analysis
- color characteristics
- exposure preference
- skin tone preference
- group-based Recipe generation

---

### Phase 5 - Semi-automatic AI Processing

Goal: prepare editable professional adjustments.

- exposure adjustment planning
- white balance suggestions
- portrait enhancement planning
- lighting balance
- background adjustment
- batch Recipe application

---

### Phase 6 - Advanced Review and Export

- batch approval
- before/after comparison
- exception handling
- JPEG/TIFF export
- Lightroom workflow integration improvements

---

## Non-goals for early development

- cloud dependency
- multi-user platform
- enterprise workflow
- Lightroom database modification
- full RAW replacement engine
- automatic destructive editing

Priority:

A fast local workflow that transforms large RAW collections into consistent, reviewable Lightroom-compatible edits.