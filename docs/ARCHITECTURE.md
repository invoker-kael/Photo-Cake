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

- `raw`, `metadata`, `importer`, `catalog`: source references, best-effort read-only RAW/EXIF capture metadata, stable asset identity and persistence.
- `preview`, `analysis`, `classification`: reusable evidence for later decisions.
- `grouping`: fast metadata/time/sequence moment groups.
- `semantic_grouping`: portrait/scene similarity refinement inside a parent moment group; promote results through `SemanticPhotoGroup::to_photo_group`.
- `culling`: Keep / Review / RejectSuggestion only; never destructive deletion.
- `reference`: `ReferenceSet` and `StyleProfile`; a reference may be outside the target group.
- `reference_store`: persistent ReferenceSet storage, active group→reference binding and StyleProfile updates; changing the selected photo preserves the set's photographer preferences.
- `color_sync`: shared style intent resolved against each target photo.
- `recipe`: one target-bound Recipe per photo; Recipe is the source of editing decisions.
- `recipe_review_store`: stable per-asset additive review overrides for photographer exceptions; regenerated base Recipes remain reference/style driven.
- `xmp`: same-basename Lightroom sidecar generation.
- `export` / renderer / workers: optional final rendered output, not the editing source of truth.
- batch / stores / runner: resumable background execution and persistence.

## Two-stage Grouping

```text
capture time + camera + filename sequence
  -> persisted Moment PhotoGroup (parent)
  -> local classification + embedding
  -> persisted SemanticPhotoGroup (child)
  -> effective Similar PhotoGroup (SEMANTIC_SIMILARITY)
```

Semantic refinement stays scoped to its persisted parent moment group so visually similar photographs from unrelated trips/events are not globally merged. `RawCatalog::list_effective_groups_for_collection` exposes semantic children when present and falls back to the moment parent when evidence is incomplete. Re-refinement replaces only that parent's semantic children; the parent chronology remains available. Manual grouping/locking remains authoritative.

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
per-asset RecipeReviewOverride (only when needed)
        |
        v
one reviewed Recipe per target asset
```

`StyleProfile` is an editable preference layer on top of measured reference values. `color_sync` is the only group color resolution engine; do not add a parallel preset-copy system.

References may come from the target group or from another compatible group. In-group reference promotion still validates membership. The workstation now persists the selected reference and editable exposure/contrast/saturation StyleProfile for each group before any adaptive edit is applied.

Reference selection is intentionally separated from unsupported color guesses. Analyze now records preview-relative exposure evidence, so a selected reference can safely produce per-photo exposure Recipes from the cache. Embedded JPEG previews do not provide a sufficiently reliable RAW white-balance/temperature measurement, so temperature/tint remain optional and are omitted from Recipe/XMP until reliable RAW/metadata evidence exists.

## Lightroom Bridge

```text
reviewed Reference + cached evidence
  -> per-photo Recipe preview
  -> explicit workstation handoff
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

XMP stores Recipe/target identity for traceability. Group sidecar output matches target-bound Recipes back to catalog RAW assets. At preview and handoff time, Photo-Cake regenerates the base Recipe from the current ReferenceSet/StyleProfile/evidence and then applies the persisted per-photo review override, so preview and XMP share the same final values. Newly written XMP is parsed back and validated before success; a failed validation removes the new sidecar and group writing rolls back sidecars created by that operation. The workstation writes sidecars only after an explicit user action, excludes only photographer-confirmed Reject photos, and preflights the entire group so an existing XMP prevents any partial write. RAW bytes are never changed.

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


## Preview Presentation Boundary

`photo-inference` extracts the embedded RAW JPEG once and `PreviewStore` owns the cached artifact. Windows exposes those same artifacts to the UI through the local Tauri asset protocol; Cull and Reference consume them as thumbnails. Do not add a second thumbnail decoder or duplicate full-size working files. UI refresh follows Analyze progress, while missing artifacts remain placeholders.


## Review Preview Evaluator

`edit_preview` is a lightweight review-only evaluator for cached embedded JPEGs. It currently approximates Recipe exposure, contrast and saturation at a bounded preview size. It must stay clearly separated from RAW demosaic/export and must not be treated as Lightroom rendering parity. The Windows command reconstructs the canonical reviewed Recipe server-side before generating the after-preview; the UI never supplies an arbitrary Recipe as authority.


## Culling Semantic Evidence Boundary

`build_group_culling_result` may attach cached segmentation-derived portrait evidence to each recommendation without changing the technical quality score. The UI can display people count, face count, primary-subject ratio and people confidence. Do not translate segmentation into eye-state or expression claims; those require dedicated evidence.


## Metadata Transparency Surface

Windows Library reads the persisted `RawAsset.camera_id` and `capture_time_ms` fields and shows them directly. This is an inspection surface only; it does not create a second metadata store. Missing capture metadata is presented as fallback/unavailable so grouping provenance remains understandable.


## Semantic Group Identity Stability

Repeated semantic refinement with the same parent, group kind and member asset set must reuse the persisted semantic child group ID. This keeps ReferenceSet/StyleProfile bindings stable across harmless re-runs. A new group ID is justified only when the actual semantic membership changes.


## RAW White Balance Evidence Boundary

`metadata` reads exact DNG/TIFF rational evidence such as `AsShotNeutral` and `AsShotWhiteXY`. `RawMetadataStore` persists the complete metadata evidence JSON by stable catalog asset ID, independently of the compact `raw_assets` grouping schema. Partial rescans merge evidence so a transient parser/container failure cannot erase previously captured WB provenance.

This layer intentionally stops before Lightroom slider synthesis. `PhotoColorAnalysis`, `GroupColorIntent`, Recipe materialization and XMP serialization all enforce a complete Temperature+Tint pair. If either axis is missing, both are omitted from downstream edits.


## XMP Interoperability Hardening

Photo-Cake parses supported Camera Raw attributes by local XML attribute name rather than assuming a fixed namespace prefix, so a valid XMP processor may rename `crs`/Photo-Cake prefixes without breaking parse-back validation. Existing-sidecar preflight treats both lowercase `.xmp` and uppercase `.XMP` as occupied targets before group writes. Unknown Lightroom/metadata attributes remain ignored rather than destroyed because Photo-Cake still refuses to overwrite an existing sidecar.


## Android Companion Contract

The Android shell now uses real `photo-core` stores instead of a demo command:

```text
BatchStore / RawCatalog / AnalysisCache
        + CullingReviewStore
        + ReferenceStore
              |
              v
Android Library / Cull / Groups / Reference
```

The shared UI exposes platform capabilities rather than assuming every bridge implements workstation actions. Android does not expose RAW import/analyze controls, semantic-refine execution, StyleProfile editing, per-photo Recipe review or Lightroom/XMP handoff. It may display locally available cached previews through the app-data asset protocol.

The remaining platform gap is project transport/synchronization between workstation and companion. That transport should move/share project state and preview context, not fork the domain model or introduce a second editing engine.


## Culling Explanation Layer

`CullingRecommendation.reasons` is derived from the same measured `CullingScore` and duplicate evidence that produced the recommendation. It is presentation evidence, not a second scoring engine. Reasons currently cover sharpness, blur, exposure, duplicate relation and overall technical weakness/strength. Unknown semantic factors remain absent.


## Companion Snapshot / Patch Boundary

`photo-core::companion` defines a transport-neutral mobile contract:

```text
Workstation project stores
        |
        v
CompanionSnapshot
  - no RAW source_path
  - effective groups
  - culling recommendations + photographer reviews
  - ReferenceSet/binding state
  - metadata provenance
  - portable preview index
        |
        v
transport (file / LAN / future private mechanism)
        |
        v
Android companion decisions
        |
        v
CompanionDecisionPatch
        |
        v
identity + membership + reject + concurrency validation
```

A patch cannot silently overwrite workstation decisions made after the snapshot. It also cannot reject the currently selected reference unless the patch clears/replaces that reference in the same validated change set.


## Companion Conflict Scope

Concurrency checks are entity-scoped, not project-global. A mobile patch compares current workstation state only for assets/groups it changes, plus any currently selected Reference whose asset decision is being changed. Unrelated workstation edits must not block an otherwise valid patch.
