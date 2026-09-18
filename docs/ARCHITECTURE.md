# Photo-Cake Architecture

## Product Shape

Photo-Cake is a local-first, semi-automatic photography workflow assistant. It keeps the original RAW as the source asset, uses small non-destructive editing metadata as the normal working output, and keeps direct JPEG/TIFF export available on demand.

```text
RAW collection
  -> catalog + preview + analysis
  -> conservative moment grouping
  -> semantic similarity refinement
  -> smart culling / review
  -> reference set + style profile
  -> group color intent
  -> adaptive per-photo recipe
  -> same-basename XMP -> Lightroom
       or
     direct export
```

The editing unit is a Photo Group, but the final numeric adjustments are resolved per photo. A reference defines visual intent; Photo-Cake must not blindly copy one photo's numeric settings across a group.

## Existing Workspace

Keep the current workspace and extend it rather than creating a second implementation.

```text
apps/
  windows/   primary workstation UI
  android/   mobile companion UI

crates/
  photo-core/       shared workflow and persistence
  photo-inference/  local analysis/model execution
```

Windows is the primary large-RAW/batch/Lightroom workflow. Android shares the same core concepts and is optimized for selection, preview, references and lightweight local work.

## photo-core Responsibilities

The existing modules are the implementation backbone:

- `raw`, `importer`, `catalog`: source references, stable asset identity and persistence.
- `preview`, `analysis`, `classification`: reusable evidence for later decisions.
- `grouping`: fast metadata/time/sequence moment groups.
- `semantic_grouping`: portrait/scene similarity refinement inside a parent moment group; promote results through `SemanticPhotoGroup::to_photo_group`.
- `culling`: Keep / Review / RejectSuggestion only; never destructive deletion.
- `reference`: `ReferenceSet` and `StyleProfile`; a reference may be outside the target group.
- `reference_store`: persistent ReferenceSet storage and active group→reference binding; changing the selected photo preserves the set's StyleProfile.
- `color_sync`: shared style intent resolved against each target photo.
- `recipe`: one target-bound Recipe per photo; Recipe is the source of editing decisions.
- `xmp`: same-basename Lightroom sidecar generation.
- `export` / renderer / workers: optional final rendered output, not the editing source of truth.
- batch / stores / runner: resumable background execution and persistence.

## Two-stage Grouping

```text
capture time + camera + filename sequence
  -> Moment PhotoGroup
  -> local classification + embedding
  -> SemanticPhotoGroup
  -> Similar PhotoGroup (SEMANTIC_SIMILARITY)
```

Semantic refinement stays scoped to its parent moment group so visually similar photographs from unrelated trips/events are not globally merged. Manual grouping/locking remains authoritative.

## Reference-driven Adaptive Editing

```text
selected reference photo analysis
        +
StyleProfile preferences
        |
        v
GroupColorIntent
        |
        v
color_sync resolves each target photo
        |
        v
Recipe::materialize_group
        |
        v
one Recipe per target asset
```

`StyleProfile` is an editable preference layer on top of measured reference values. `color_sync` is the only group color resolution engine; do not add a parallel preset-copy system.

References may come from the target group or from another compatible group. In-group reference promotion still validates membership. The workstation now persists the selected reference for each group before any adaptive edit is applied.

Reference selection is intentionally separated from unsupported color guesses. Analyze now records preview-relative exposure evidence, so a selected reference can safely produce per-photo exposure Recipes from the cache. Embedded JPEG previews do not provide a sufficiently reliable RAW white-balance/temperature measurement, so temperature/tint remain optional and are omitted from Recipe/XMP until reliable RAW/metadata evidence exists.

## Lightroom Bridge

```text
Recipe
  -> XmpEditState
  -> IMG_0001.xmp beside IMG_0001.CR3
  -> Lightroom / Camera Raw
```

Current mapped adjustments:

- exposure
- contrast
- highlights
- shadows
- temperature
- tint
- saturation

XMP stores Recipe/target identity for traceability. Group sidecar output matches target-bound Recipes back to catalog RAW assets. RAW bytes are never changed.

Future mappings such as HSL, tone curve, masks and richer skin/color controls extend the Recipe/XMP model rather than creating a second editing model.

## Storage and Safety

Default storage behavior:

- keep existing RAW files in place;
- create previews/caches in managed application data;
- create small XMP sidecars only when the user applies/handoffs edits;
- preflight a batch before sidecar writing and never silently overwrite an existing Lightroom XMP;
- do not automatically create full-size TIFF/JPEG working copies;
- direct export creates rendered files only when requested.

## Platform Boundary

Shared business logic belongs in `photo-core` and `photo-inference`. Platform shells own file access, UI, packaging, acceleration bindings and device resource policy. Windows and Android must not maintain separate photography logic.

## Development Order

Close gaps in the existing workflow instead of restarting phases:

```text
catalog/preview
 -> two-stage grouping
 -> culling evidence
 -> reference/style
 -> adaptive per-photo recipes
 -> XMP handoff
 -> review UI
 -> direct export polish
 -> richer local AI/edit controls
```

Full RAW-engine replacement, cloud editing, accounts and a Lightroom database/plugin integration are not early dependencies.


## Batch Runner Boundary

The persistent batch runner is preparation infrastructure, not the whole photographer workflow. New RAW items run `IMPORT -> ANALYZE -> DONE`, where DONE means reusable local evidence is ready. Grouping, culling review, reference selection, adaptive Recipes and XMP handoff operate above that per-photo preparation queue. Legacy preset/retouch/QA/export stage values remain loadable for old project data and explicit adapters, but new RAW imports do not automatically traverse no-op editing/export stages.

The Windows workstation now exposes the existing `RawImporter::import_directory` through a native directory picker. Android keeps the companion role and shares UI/core contracts without duplicating Windows filesystem behavior.


## Smart Culling Evidence Flow

The implemented culling path reuses Analyze output instead of running a second inference pass:

```text
RAW preview
  -> deterministic technical quality (sharpness/blur/exposure)
  -> cached QualityScoring artifact
  + cached image embedding
  -> PhotoGroup-scoped duplicate similarity
  -> Keep / Review / RejectSuggestion ranking
  -> workstation Cull view
```

Unknown semantic evidence such as expression or composition remains absent rather than being given invented neutral scores. If an asset has no cached quality evidence yet, the Cull view reports it as pending. Near-duplicate comparison stays inside the current Photo Group and only demotes lower-ranked alternatives to Review; originals are never deleted.


## Culling Review Persistence

`CullingReviewStore` persists photographer decisions independently from model suggestions:

```text
cached evidence -> AI recommendation
                     +
              photographer override
                     |
                     v
        effective review state
```

The review is keyed by stable RAW asset ID so re-importing the same source keeps the photographer's decision. Clearing a review restores the current AI suggestion. Reject remains metadata/state and never removes the source file.


## Partial Evidence Rule

`PhotoColorAnalysis`, `GroupColorIntent` and resolved edits support missing white-balance evidence. Preview-relative exposure can drive adaptive exposure immediately; absent temperature/tint stays `None` all the way through Recipe and XMP. XMP serialization therefore writes only supported/measured fields rather than filling unknown values with defaults.
