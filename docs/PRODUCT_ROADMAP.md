# Photo-Cake Product Roadmap

## Product Goal

Photo-Cake is a local-first semi-automatic AI photo workflow. It is not a Lightroom replacement; it reduces repetitive editing work by preparing and applying consistent edits across batches.

## Development Order

### Phase 1 - Automation Foundation

- persistent batch jobs
- background worker
- checkpoint/resume
- crash recovery
- safe export

### Phase 2 - Photo Library Foundation

- project model
- catalog database
- asset identity
- preview cache
- metadata indexing

### Phase 3 - AI Analysis

- local classification
- embeddings
- similarity grouping
- duplicate detection
- AI culling scores

### Phase 4 - Non-destructive Editing

- edit graph
- presets/recipes
- reference photo workflow
- history and rollback

### Phase 5 - Semi-automatic AI Retouch

Priority features:

- exposure and white balance correction
- portrait detection
- skin refinement
- face enhancement
- lighting balance
- background adjustment
- group-based batch application

### Phase 6 - Review and Export Experience

- representative preview review
- exception handling
- batch approval
- JPEG/TIFF export
- optional Lightroom/Photoshop handoff

## Non-goals for early development

- cloud dependency
- multi-user features
- enterprise workflow
- plugin ecosystem
- full professional color suite

The priority is a fast local workflow that turns large photo batches into reviewable, consistent results with minimal manual editing.
