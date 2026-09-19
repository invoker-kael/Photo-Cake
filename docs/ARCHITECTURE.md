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

XMP stores Recipe/target identity for traceability. Group sidecar output matches target-bound Recipes back to catalog RAW assets. At preview and handoff time, Photo-Cake regenerates the base Recipe from the current ReferenceSet/StyleProfile/evidence and then applies the persisted per-photo review override, so preview and XMP share the same final values. Newly written XMP is parsed back and validated before success; a failed validation removes the new sidecar and group writing rolls back sidecars created by that operation. The workstation writes sidecars only after an explicit user action, excludes only photographer-confirmed Reject photos, and preflights the entire group so an existing XMP prevents any partial write. RAW bytes are never changed. After all missing sidecars are created, the core immediately re-preflights the complete target set and requires every existing sidecar to match the current effective Recipe state. A missing or externally changed sidecar fails the handoff and removes sidecars created by that operation; batch handoff propagates the same failure so earlier newly-created groups are rolled back.

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


## Companion Hydration Flow

```text
Workstation build_companion_snapshot
  -> persist exported baseline
  -> transport
  -> Android import_companion_snapshot
       -> synthetic companion:// asset refs
       -> effective groups
       -> metadata + culling reviews + ReferenceSet state
       -> snapshot baseline store
       -> snapshot culling recommendations
  -> mobile Cull / Reference changes
  -> build_companion_decision_patch
  -> transport back
  -> workstation apply_companion_decision_patch
       -> validate original baseline
       -> apply only touched decisions
       -> clear baseline after success
```

Android does not require AnalysisCache parity for imported projects; it reuses the workstation's portable culling results from the snapshot. Preview bytes remain a separate transport artifact and are not embedded into snapshot JSON.


## Lightroom Delivery Attention Surface

The Lightroom page is a delivery queue rather than a flat archive of groups. Its default `Needs action` mode orders groups by photographer intervention value: XMP conflicts first, unresolved Reference/evidence next, outstanding Recipe attention next, then missing XMP. Groups whose deliverable sidecars are current and whose review attention is clear are hidden until `All` is selected.

A successful handoff result is session-scoped evidence only. The UI invalidates that result when canonical delivery inputs change (Cull deliverability, Reference binding/style, per-photo Recipe exception, grouping or active project), so an old “verified” badge cannot survive a material Recipe change.


## Batch Reference Look Sync

Large travel/family shoots often contain several related scene groups that should share a visual direction without sharing a fixed numeric exposure correction. The workstation therefore treats one referenced group as the source look and can transactionally copy its `StyleProfile` to multiple selected referenced groups.

The operation validates the source plus every target before the first write, rejects duplicate/source-as-target selections, and updates all target ReferenceSets in one SQLite transaction. Each target keeps its own selected Reference photo and its own adaptive baseline; only the shared photographer preference layer is synchronized. The UI omits targets whose effective visual StyleProfile already matches the source.

This follows the scalable editing pattern “establish a standard look -> synchronize selected similar contexts -> review exceptions” while keeping Photo-Cake's single canonical path:

```text
source Reference + StyleProfile
          |
          v
selected target groups
  keep target References
          |
          v
regenerate adaptive Recipes
          |
          v
exception-first Review
          |
          v
verified Lightroom XMP
```

No preset-copy renderer or second color pipeline is introduced.


## Workflow Cockpit

Photo-Cake exposes a read-only workflow status derived from canonical project state rather than maintaining a second workflow database. The core receives aggregated `WorkflowFacts` and deterministically selects the next photographer focus:

```text
Prepare
  -> Cull attention
  -> Reference attention
  -> Recipe review attention
  -> Lightroom delivery attention
  -> Complete
```

The Windows workstation assembles those facts from the existing batch queue, culling evidence/reviews, Reference bindings, canonical reviewed Recipes and read-only XMP preflight. Lightroom facts include unresolved groups, existing-sidecar conflicts, missing sidecars and groups whose current Photo-Cake XMP already matches the current Recipes.

This is navigation automation, not decision automation. `Continue workflow` may move the photographer to the most relevant surface, but it never confirms a Cull decision, selects a Reference, accepts a Recipe or writes XMP. The cockpit is intentionally a fixed photography state machine rather than a generic node editor: the goal is to reduce scanning and setup overhead while preserving the existing canonical workflow and explicit photographer gates.


## Manual Group Corrections

Automatic grouping remains the default, but the photographer can correct the small number of mistakes before Reference selection. Manual corrections operate on the same canonical catalog rather than creating an overlay:

- a semantic child can restore its original Moment parent and lock that Moment together;
- an automatically locked Moment can be reopened for a later semantic-refinement pass;
- adjacent non-semantic parent groups can be merged transactionally;
- a parent group can be split immediately before a selected photo;
- merged/split groups are persisted as `PhotoGroupKind::Manual` with `GroupingBasis::Manual` and remain locked against automatic refinement.

Merge preserves the earliest group ID and chronological member order. Split preserves the original group ID for the left side and creates one new ID for the right side. Semantic children are removed when their parent is manually corrected.

All grouping mutations are blocked after any Reference selection in the current batch. This prevents a grouping correction from silently orphaning Reference bindings, Recipe lineage, review fingerprints or Lightroom delivery state. The photographer must clear References first if they intentionally want to regroup.


## Batch Reference Setup

Reference selection now has an explicit batch path for large shoots, but it still reuses the same per-group shortlist and ReferenceStore model. The UI may preselect the current best starting candidate for a group only when that candidate is already supported by photographer Keep/Review or completed culling evidence that is not an AI Reject suggestion.

The batch request sends explicit `group_id + asset_id` pairs. The Windows backend revalidates every pair against the current effective batch groups, current culling reviews and current cached culling recommendations. Existing Reference bindings are never overwritten by this batch path; changing an already referenced group remains an individual deliberate action.

After all requests validate, ReferenceStore creates the missing single-photo ReferenceSets and group bindings in one SQLite transaction. Duplicate groups, stale group membership, photographer Reject, missing culling evidence, AI RejectSuggestion, or an already-bound group aborts the complete request before any selected group is written.

This is a batch confirmation surface, not automatic Reference selection. It follows the same scalable pattern as Cull `Confirm visible`, Recipe `Confirm visible`, batch look sync and Lightroom batch handoff: derive useful defaults, let the photographer choose the scope, then commit that scope transactionally.


## Exposure Bracket / HDR Source Protection

Photo-Cake treats exposure bracketing as capture topology that must survive the normal batch workflow, not as ordinary near-duplicate clutter.

Analyze already produces two reusable evidence sources that make conservative detection possible without a second scanner: preview-relative exposure and image embeddings. Within one existing PhotoGroup, Photo-Cake looks for contiguous odd-sized 3/5/7/9-frame ladders whose measured exposure values are symmetric around a center frame and whose embeddings still describe essentially the same composition.

When a bracket is detected:

- every source frame is marked as an exposure-bracket member in Cull;
- near-duplicate evidence no longer acts as a reason to discard a bracket member;
- a bracket member is never left as an automatic RejectSuggestion through the ordinary culling path; uncertain frames remain Reviewable;
- if one complete Moment parent is exactly one bracket set, semantic refinement preserves that parent instead of splitting the exposure ladder;
- the measured center exposure remains a useful manual reference candidate, but groups containing bracket sources are excluded from automatic batch Reference setup;
- bracket source asset IDs are removed from the ordinary Adaptive Recipe target set, so Photo-Cake never "normalizes" -EV/+EV capture intent before HDR merge;
- Lightroom preflight reports those source RAWs as an explicit HDR-merge action, and safe batch XMP excludes any group that still contains bracket sources;
- mixed groups may still hand off ordinary non-bracket peers individually, while bracket RAWs remain untouched.

This is intentionally **not** an HDR merge implementation. Photo-Cake preserves bracket RAW capture exposure and does **not** write normalization XMP to those source frames. The current production boundary is: detect -> protect -> isolate -> merge in Lightroom/Camera Raw -> re-import/use the resulting HDR DNG if the photographer wants Photo-Cake to continue the normal Reference/Recipe path. A native HDR merge belongs only after there is a real RAW-domain merge/render path with trustworthy color and metadata behavior.

The scalable editing pattern remains the existing one: establish a standard Reference/look, synchronize only the shared preference layer, let adaptive Recipes resolve per-photo differences, then inspect exceptions. This adopts the useful standard-photo -> selective batch synchronization -> focused exception-review production pattern from mature batch photo editors without introducing a second preset-copy engine.


### Bracket-aware delivery routing

Bracket handling is a routing decision, not a second editing engine:

```text
Moment / Similar PhotoGroup
  -> bracket detection from existing Analyze evidence
  -> HDR source RAWs -----------------------> Lightroom / Camera Raw HDR merge
  -> ordinary peers -> Reference -> Recipe -> Review -> XMP
```

The Workflow Cockpit counts unresolved HDR merge groups separately from XMP conflicts/missing sidecars. A pure bracket group therefore does not need a fake Reference or an empty Recipe confirmation just to advance the workflow. It remains a Lightroom action until the photographer explicitly marks the external merge complete.

HDR completion is persisted against a fingerprint of the **current detected bracket membership + center frame**, not just the group ID. If regrouping or new analysis changes the bracket set, the saved completion no longer matches and the HDR action reopens automatically. This prevents a stale "merged" flag from hiding a materially different source stack.

Safe batch handoff is deliberately stricter than individual handoff. Any group with **pending** HDR source RAWs is excluded from the transactional batch cohort. Once the current bracket fingerprint is marked merged, ordinary non-bracket peers in a mixed group may rejoin safe batch delivery. A pure bracket source group can become workflow-current without manufacturing zero-value XMP files.


## Moment Quick Cull

Large travel/family shoots often contain many short bursts where the existing group-relative ranking already identifies a clear starting frame, but forcing the photographer to persist every obvious decision one card at a time wastes time. Moment Quick Cull is an explicit batch acceptance layer on top of the existing Cull evidence; it is not a second scoring model.

A quick-cull plan is available only when the group has at least two scored photos, no pending quality evidence, no detected exposure bracket, and rank #1 is a normal Cull `Keep`. The plan always keeps that primary frame. Remaining frames are routed conservatively:

- if any person/face evidence exists in the group, every alternate stays `Review`; expression alternatives are never batch-rejected by this shortcut;
- if people/face evidence is incomplete for any scored frame, the shortcut becomes conservative and every alternate stays `Review`; absence of evidence is never treated as proof that the group contains no people;
- only when people evidence is complete and the group is confirmed non-people may a lower-ranked frame already identified as a near duplicate and trailing the primary by a material quality gap be routed to `Reject`;
- every other alternate remains `Review`.

The workstation may multi-select eligible moments, but the Windows backend re-resolves each group from the current analysis cache before writing. A selected group is rejected from the transaction if it has acquired a Reference, any photographer Cull decision, missing evidence, HDR-bracket status, or another condition that invalidates the plan. Only after all selected groups pass does the existing transactional Cull store persist the combined decisions.

This preserves the workflow boundary:

```text
Analyze -> Group-relative Cull evidence -> Moment Quick Cull (explicit)
                                      -> Keep primary
                                      -> Review protected alternates
                                      -> Reject only clear non-people near duplicates
          -> manual Cull exceptions -> Reference -> Recipe -> Review -> XMP
```

A `Review` result is intentionally not equivalent to approval. It clears the initial selection ambiguity while keeping that frame in the later exception workflow. This is especially important for family photography, where the technically strongest frame may not contain the preferred expression.

Quick Cull is scene-aware rather than portrait-only. The existing segmentation/classification artifact also carries scene tags such as Landscape, Architecture, Food and Night. Those tags and the exact embedding near-match value are surfaced on each Cull recommendation. For a non-people group, automatic Reject requires all of the following: complete scene evidence, a shared scene tag across the moment, embedding similarity of at least 0.985, and the existing material quality gap. Mixed/unknown scenes remain Review. This makes scenic travel bursts conservative while still collapsing truly redundant frames.

Each applied Quick Cull batch is recorded transactionally with its batch/group scope, exact written decisions and write timestamp. The workstation exposes only the latest operation for Undo. Undo is refused if group topology changed, a downstream Reference now exists, or any recorded decision was modified after Quick Cull—even if the photographer later changed it back to the same label. A successful Undo removes only the decisions written by that operation and returns those photos to the ordinary AI-suggestion flow.


## Canonical Reference readiness preflight

Batch Reference setup now has one canonical planner in `photo-core::reference` instead of a second UI-only candidate algorithm. The planner consumes the existing Cull recommendations, photographer Cull reviews, pending evidence and HDR bracket routing. Candidate order is photographer Keep → AI Keep → photographer Review → AI Review, then measured quality/rank tie-breaks. A photographer Keep may advance a group while sibling evidence is still pending; AI-only automation waits for the group to finish Cull evidence. Photographer Reject and unoverridden AI Reject suggestions are excluded.

The same `ReferenceReadinessPlan` is returned to the workstation and revalidated by the batch Reference write command, so a stale browser suggestion cannot silently choose a different photo. Landscape, architecture, food, night and other scene tags remain visible context from the existing classifier; they do not create a portrait-only branch or a second Reference engine. Exposure-bracket groups remain blocked for HDR merge before batch Reference selection.


## Scene-aware StyleProfile sync preflight

Batch look sync continues to copy the existing `StyleProfile`; there is no second preset or scene-rendering engine. Before the UI proposes targets, the backend summarizes each referenced group from existing Cull evidence after excluding photographer Rejects and unoverridden AI Reject suggestions.

Compatibility is advisory, not a hard gate. People/family groups are recommended with other people/family groups. Non-people groups are recommended when they share scene evidence such as Landscape, Architecture, Food, Night or Document. People-to-scene, disjoint-scene and incomplete-evidence pairs are marked Review. The photographer can still explicitly select Review targets; each group keeps its own Reference and adaptive exposure baseline, and downstream Recipe Review remains the exception check.


## Canonical Recipe Review preflight

Recipe Review now has one backend authority for exception-first triage and clear-group batch confirmation. The planner combines the current adaptive Recipe fingerprint, saved per-photo exception, photographer Cull decision, AI Cull recommendation, evidence readiness, scene context, and HDR source routing into a per-group preflight.

Each editable asset is classified as `CONFIRMED`, `NEEDS_REVIEW`, `CLEAR`, or `PENDING`. Attention reasons are explicit: saved exception, photographer Review, AI Reject suggestion, AI Review, or incomplete evidence. The UI uses these dispositions for Triage and ordering; it no longer re-derives review eligibility from raw Cull state.

Clear-group confirmation re-runs the same backend planner immediately before persistence. A group can be batch-confirmed only when it has at least one current clear Recipe and no attention or pending Recipes. HDR bracket source RAWs remain outside ordinary adaptive Recipe review and are surfaced as separate routing context, so mixed groups can still review non-HDR peers without treating bracket EV differences as edit errors.


## Reference-relative adaptive tone matching

ExposureAnalysis v2 keeps the existing preview-relative exposure signal and adds luminance p10/p50/p90 plus shadow/highlight clipping ratios from the embedded RAW preview. These values are relative photographic evidence only; they are not treated as sensor-linear RAW measurements.

Reference-driven Recipe generation first resolves the normal per-photo exposure delta, then projects each target's tone percentiles through that exposure correction and compares them with the selected Reference. The remaining tonal difference becomes bounded per-photo Highlights/Shadows adjustments. This avoids blindly copying the Reference's numeric tone sliders while still matching its bright/dark distribution.

Old ExposureAnalysis payloads remain readable. If percentile evidence is absent, tone matching is skipped and the previous exposure-only behavior remains intact.

Recipe Review preview now approximates Highlights/Shadows in addition to Exposure/Contrast/Saturation. The preview remains an embedded-JPEG approximation; Lightroom/Camera Raw remains authoritative for RAW rendering. Per-photo exceptions can adjust and selectively synchronize Exposure, Highlights, Shadows, Contrast and Saturation. XMP already carries Highlights/Shadows through the existing non-destructive handoff path.


### Reference-relative contrast and color intensity

The same evidence path extends tonal matching without adding another workflow. ExposureAnalysis v3 records a bounded relative colorfulness statistic alongside luminance percentiles. After per-photo exposure alignment, Recipe generation compares the target's P10–P90 span with the selected Reference to derive a small per-photo Contrast correction. Positive contrast is reduced as shadow/highlight clipping increases.

Colorfulness is also compared against the Reference to derive a bounded per-photo Saturation correction. The shared StyleProfile Contrast/Saturation values remain the photographer's intentional look; the adaptive corrections are added on top only to normalize target-to-target variation. Old cached evidence without colorfulness simply skips the new saturation correction.


### Median exposure refinement and preview transfer function

Reference-relative exposure keeps the existing trimmed-mean signal as its primary estimate, then uses preview P50 only as a bounded secondary correction. The correction is limited to ±0.60 EV and is prevented from brightening when the target's projected P90 has no headroom relative to the selected Reference. StyleProfile exposure bias remains part of the desired target and is not normalized away.

Edited preview applies Exposure in linear-light sRGB rather than multiplying gamma-encoded channel values. It then converts back to sRGB before the existing perceptual Contrast, Highlights/Shadows and Saturation approximation. This materially improves visual review fidelity while keeping Lightroom/Camera Raw authoritative for final RAW rendering.
