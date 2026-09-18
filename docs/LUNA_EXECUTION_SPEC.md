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
raw/importer/catalog       source + identity + persistence
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
4. richer culling evidence (eyes/expression) and more accurate review-preview evaluation where evidence/rendering supports it;
5. validate emitted XMP against real Lightroom/Camera Raw fixtures in addition to the implemented local parse-back gate;
6. shared/Android review/reference UX;
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
