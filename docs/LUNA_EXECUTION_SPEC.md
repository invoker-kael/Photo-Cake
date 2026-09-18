# Photo-Cake Luna Execution Specification

## Authority

This is Luna's execution entry. Build the existing repository into the user's personal semi-automatic photography workflow. Do not replace working foundations and do not stop at planning when an implementable gap exists.

Read in order:

1. this file;
2. `PRODUCT_REQUIREMENTS.md`;
3. `ARCHITECTURE.md`;
4. `PRODUCT_ROADMAP.md`;
5. current code and tests.

Code state is authoritative for what already exists. The roadmap is direction, not permission to reimplement completed work.

## Target Workflow

```text
RAW collection
  -> catalog + preview
  -> smart culling
  -> moment grouping
  -> semantic refinement
  -> choose/edit reference
  -> StyleProfile
  -> GroupColorIntent
  -> adaptive per-photo Recipes
  -> review
  -> same-basename XMP -> Lightroom
       or
     direct JPEG/TIFF export
```

## Mandatory Execution Rules

- Reuse `photo-core` and `photo-inference`; never create a parallel workflow engine.
- Preserve RAW bytes and source filenames.
- Prefer RAW + XMP; do not generate large intermediates by default.
- Existing XMP is user data: never silently overwrite it; preserve the current batch preflight behavior.
- Recipe is the editing source of truth. XMP and direct export are outputs.
- A group shares visual intent, not identical numeric adjustments.
- Reuse `color_sync` for per-photo resolution.
- Smart culling only recommends Keep / Review / RejectSuggestion; never auto-delete.
- Preserve manual grouping/selection decisions.
- Keep business logic cross-platform. Windows and Android use shared core models.
- Keep normal operation local/offline.
- Do not prioritize cloud services, accounts, a Lightroom plugin/database writer or a replacement RAW engine.
- Do not create additional docs for requirements already covered by the five files in `docs/`; update the owning file instead.

## Reuse Map

```text
raw/metadata/importer/catalog source + EXIF grouping evidence + identity + persistence
preview/analysis           reusable image evidence
classification             portrait/scene routing
grouping                   fast moment groups
semantic_grouping          local visual refinement
culling                    selection recommendation
reference                  ReferenceSet + StyleProfile
color_sync                 adaptive group intent resolution
recipe                     target-bound per-photo edit decisions
xmp                        Lightroom sidecars
export/renderer/workers     optional rendered output
batch/runner/stores         resumable execution
photo-inference             local models, embedding, segmentation
```

## Required Editing Chain

For reference-driven work use this chain unless a concrete code defect requires changing it:

```text
ReferenceSet::resolve_group
  -> StyleProfile -> GroupColorIntent
  -> build_adaptive_group_plan / color_sync
  -> Recipe::materialize_group
  -> write_group_sidecars
```

A reference may be external to the target group. A target Recipe must remain bound to exactly one target asset before XMP output.

## Required Grouping Chain

```text
initial_group_raw_assets
  -> Moment PhotoGroup
  -> refine_group_by_similarity
  -> SemanticPhotoGroup::to_photo_group
  -> reference/culling/edit workflow
```

Do not globally regroup unrelated collections only because embeddings look similar.

## Current Gap-first Priority

At every run inspect what is already implemented and take the smallest complete next gap. Prefer, in order:

1. compile/test/CI regressions;
2. end-to-end wiring between existing core modules;
3. add reliable RAW/metadata white-balance evidence when available, without blocking exposure-only workflow;
4. add dedicated eye/expression evidence only when a reliable local model exists; current Cull already surfaces segmentation-based people/face/subject evidence without folding it into quality scoring;
5. validate emitted XMP against real Lightroom/Camera Raw fixtures in addition to the implemented local parse-back gate;
6. Android project transport/synchronization and mobile preview availability; the real Cull/Groups/Reference companion contract is already wired;
7. direct export polish and richer local AI/edit controls.

Do not redo lower-numbered items that already pass.

## Completion Gate

A change is complete only when:

- it advances the photographer workflow;
- existing architecture is reused or a necessary refactor is justified in code;
- tests cover the changed behavior;
- relevant CI/build checks pass;
- no new duplicate workflow or documentation path is introduced.

CI is verification, not the product. Do not create repeated CI-only commits unless a failing check identifies a real problem.


## Batch Boundary Rule

Do not turn the per-photo `AutomationRunner` into the product workflow. New imports use it for import/analyze preparation only and then become ready for group-level review. Culling, grouping, reference selection, adaptive Recipe generation and XMP delivery are higher-level operations. Preserve legacy stage decoding only for compatibility.

The workstation import button must continue to call the existing `import_raw_directory` / `RawImporter` path; do not add a second importer. Android remains a companion and must reuse shared UI/core concepts.


## Culling Execution Rule

Analyze writes `QualityScoring` and image-embedding evidence into `AnalysisCache`. Downstream culling must call `build_group_culling_result` and consume that cache; do not re-run inference from the Cull page. Missing evidence is pending, not a guessed score. Duplicate ranking remains PhotoGroup-scoped, keeps the strongest candidate, and leaves alternatives reviewable.


## Human Review Authority

Treat `CullingReviewStore` as authoritative for explicit photographer Keep/Review/Reject decisions. AI output remains a suggestion underneath it. Never overwrite a saved photographer decision when analysis/model versions change, and never translate Reject into file deletion. A cleared override returns control to the latest suggestion.


## Reference Selection Boundary

The workstation already persists per-group reference selection through `ReferenceStore`. Preserve the selected `ReferenceSet` and its `StyleProfile` when the photographer changes the chosen photo. Do not generate adaptive white-balance/XMP values from an invented preview-derived Kelvin estimate; wire real color evidence first, then call the existing `ReferenceSet::resolve_group` → Recipe → XMP chain.


## Partial Color Evidence Rule

`ExposureAnalysis` is intentionally preview-relative. It may drive group-relative exposure adaptation, but it is not camera-metering EV. White balance is optional in `PhotoColorAnalysis`; if temperature/tint are unavailable, preserve `None` through `GroupColorIntent`, Recipe and XMP. Never substitute 5500K/0 tint or infer Kelvin from the embedded JPEG just to populate fields.


## Explicit Lightroom Handoff Rule

The workstation now has an explicit per-group XMP write action. Keep it user-triggered. Build the target set from the Photo Group minus photographer-confirmed Reject items; AI RejectSuggestion alone must not silently exclude a source. Recompute Recipes from the persisted ReferenceSet and cached evidence at handoff time, then use the existing all-group XMP preflight. Never overwrite or partially replace an existing Lightroom sidecar.


## Preview Reuse Rule

Cull and Reference now display cached PreviewStore artifacts through the Windows local asset protocol. Reuse that cache. Do not add another preview extraction pipeline or full-size JPEG/TIFF working copies for review UI. Missing preview means placeholder/pending, not a second decode path.


## Per-photo Review Override Rule

Use `RecipeReviewStore` only for photographer exceptions after group-level Reference/StyleProfile adaptation. Store additive deltas by stable target asset ID, not a frozen copy of the whole Recipe. At preview and XMP handoff time regenerate the base Recipe first, then apply the override. Clearing the override must return the photo to the current group-derived Recipe; changing the reference/style must not erase intentional per-photo exceptions.


## Semantic Group Persistence Rule

Moment PhotoGroups are persisted parents. Semantic refinement must write child `SemanticPhotoGroup` records and expose them through effective-group reads; never destructively replace the parent just to show refined groups. Missing classification/embedding evidence leaves the current parent/refinement untouched. Photographer Cull decisions are asset-bound and survive refinement. Once a Reference is bound to an effective group, do not silently regenerate group IDs underneath it.

## Before/After Preview Rule

Use the shared `edit_preview` evaluator only on cached embedded previews and only as a clearly labeled approximation. Reconstruct the canonical Recipe in the backend from ReferenceSet + StyleProfile + evidence + RecipeReviewOverride before rendering. Do not accept a UI-supplied edit graph as authority and do not create full-size working images for comparison.

## XMP Parse-back Gate

Every newly written Photo-Cake XMP must parse back into the supported edit state and match Recipe ID, target asset ID and mapped numeric fields within serialization tolerance. On validation failure remove the new sidecar; on group-write failure roll back sidecars created by that operation. This local gate complements, but does not replace, real Lightroom/Camera Raw fixture testing.


## RAW Metadata Rule

Initial Moment grouping should use reliable embedded metadata before filesystem timestamps. `metadata` reads standard EXIF Make/Model and DateTimeOriginal/DateTime without modifying the RAW; failure is a normal fallback, not an import error. Do not interpret EXIF Auto/Manual white-balance mode or rendered JPEG colors as numeric Kelvin/tint evidence.


## Metadata Persistence Rule

RAW/EXIF extraction is best-effort. When rescanning a known asset, newly available camera/capture metadata may enrich the catalog, but a transient metadata-read failure must not erase previously persisted `camera_id` or `capture_time_ms`. Filesystem modification time may refresh independently as fallback evidence.


## Effective Group Addressing Rule

After semantic refinement, downstream workstation actions must treat persisted semantic children as first-class effective groups. Reference selection, Recipe review and Lightroom/XMP handoff must resolve group IDs through `RawCatalog::find_group` or equivalent effective-group-aware APIs; do not fall back to parent-only `list_groups()` lookups.


## Semantic Group Identity Rule

Semantic refinement is repeatable, not identity-destructive. Before replacing persisted children, reuse an existing child ID whenever parent scope, semantic kind and member asset set are unchanged. Do not generate a fresh UUID for an equivalent group, because downstream Reference/Style/XMP state is bound to that effective group identity.


## RAW White Balance Evidence Rule

DNG/TIFF `AsShotNeutral` / `AsShotWhiteXY` are persisted as exact rational provenance in `RawMetadataStore`; do not treat those source tags as already-converted Lightroom Temperature/Tint. A future solver may consume them only with a documented color-model/calibration path and validation fixtures.

Downstream white balance is pair-gated at multiple boundaries. If either Temperature or Tint is missing, ColorSync must resolve neither, Recipe must materialize neither, and XMP must serialize neither. Do not weaken this gate for convenience.


## XMP Compatibility Rule

Do not depend on a specific XML namespace prefix when reading supported XMP attributes; namespace processors may legally rename prefixes. Continue treating existing `.xmp` or `.XMP` as protected photographer/Lightroom state. Until a safe merge strategy exists, never overwrite an existing sidecar.


## Android Companion Capability Rule

Android reuses shared `photo-core` persistence and may read/write photographer selection/reference decisions. Treat platform bridge methods as capabilities: do not add fake no-op or always-error workstation methods merely to satisfy an interface. Keep RAW import/analyze control, StyleProfile/Recipe editing, direct export and Lightroom XMP writes workstation-only.

The next Android gap is transport of project DB/state and preview artifacts between workstation and device. Solve transport separately from photography business logic; do not introduce cloud accounts or duplicate stores just to synchronize.


## Explainable Cull Rule

Cull must explain suggestions from measured evidence without creating a parallel score. Reuse `CullingRecommendation.reasons`; do not infer subjective composition quality, eye state or expression when those signals were not measured. Explanation changes must not alter photographer authority or delete/exclude files automatically.


## Companion Transport Rule

Use `CompanionSnapshot` / `CompanionDecisionPatch` as the cross-device domain boundary. Do not copy RAW absolute paths, full AnalysisCache databases or workstation editing state into the mobile contract. Snapshot assets use stable IDs plus portable metadata only; previews are represented by transport indexes.

Before applying a mobile patch, validate schema version, base snapshot ID, batch ID, asset/group membership, duplicate changes, rejected-reference invariants and concurrent workstation culling/reference edits. Reject conflicts instead of last-writer-wins. Keep the actual transport mechanism separate from this contract.


## Companion Conflict Scope Rule

Do not reject a mobile patch because unrelated project state changed after the snapshot. Compare concurrent culling state only for touched assets, reference state only for touched groups, and additionally the group of any selected reference whose culling decision is being changed. Preserve conflict rejection where the same photographer decision actually overlaps.


## Companion Hydration / Patch Rule

Hydrate a received snapshot into Android using synthetic `companion://` source references; never pretend the phone owns the workstation RAW path. Mark imported batch items prepared and reuse snapshot effective groups. Persist the original snapshot baseline and use its precomputed culling results on Android instead of rerunning desktop analysis.

Generate the return patch by diffing current mobile CullingReviewStore / ReferenceStore against that baseline. Reject a different replacement snapshot for the same batch until the existing baseline has been synchronized or explicitly cleared. On workstation, retain the exported baseline, validate/apply the returned patch, then clear that baseline only after successful application.
