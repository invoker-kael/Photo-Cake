# Photo-Cake Product Roadmap

## Direction

Photo-Cake evolves the existing Windows + Android + shared Rust workspace into a personal Pixel-Cake-style workflow without replacing Lightroom.

Core path:

```text
RAW
 -> organize/select
 -> reference-driven adaptive edit
 -> review
 -> XMP / direct export
```

The roadmap is gap-driven. Do not restart completed foundations just because they appear in an earlier phase.

## Implemented Foundation

Current code already contains substantial reusable groundwork:

- RAW scanning and catalog persistence, with best-effort EXIF capture-time/camera metadata for initial grouping;
- stable asset identity;
- preview extraction/cache and local analysis;
- portrait/scene classification;
- first-pass moment grouping;
- semantic embedding-based group refinement with persisted moment parents, semantic children and effective-group reads;
- batch/job/export infrastructure;
- local model loading, segmentation and embeddings;
- cached technical culling evidence, group-relative duplicate ranking and persisted photographer decisions;
- ReferenceSet and StyleProfile, plus persistent per-group reference selection and editable exposure/contrast/saturation preferences;
- adaptive `color_sync`;
- target-bound per-photo Recipe materialization plus persisted additive per-photo review overrides;
- Lightroom XMP document and same-basename sidecar writing;
- optional direct export path;
- Windows workstation plus a real Android companion bridge for Library, Cull, effective Groups and Reference decisions.

These should be strengthened and wired together, not recreated.

## Milestone A — Reliable End-to-End RAW → XMP

Goal: make one real shoot usable through the entire non-destructive path.

Close remaining gaps around:

- preview-relative exposure analysis is implemented and can drive adaptive exposure Recipes; reliable RAW/metadata white-balance evidence remains a later enrichment, not a blocker;
- end-to-end orchestration from the persisted group/reference selection to Recipes/XMP;
- persistence/refinement of semantic groups and additional review provenance where needed; Reference selection, group StyleProfile and per-photo review overrides are already persisted;
- Lightroom/Camera Raw compatibility tests for emitted XMP, including parse-back validation, namespace-prefix-tolerant parsing and case-safe existing-sidecar preflight;
- safe overwrite/update behavior for an existing sidecar;
- clear Windows workflow for apply/review/handoff.

Acceptance: a target group can use a selected reference, produce different adaptive Recipes per photo, visually review exceptions, and write parse-back-validated Lightroom XMP beside the original RAWs without changing RAW bytes.

## Milestone B — Useful Smart Culling

Improve culling from a data model into a practical selection assistant:

- measurable sharpness/blur and exposure evidence cached during Analyze;
- group-scoped duplicate/burst ranking from existing embeddings;
- people/face/subject evidence from existing segmentation is surfaced now; eye-state/expression must wait for a dedicated reliable local model;
- exposure-failure detection;
- group-relative best-candidate ranking;
- user override persistence.

Acceptance: a large burst or event reduces to sensible Keep/Review/RejectSuggestion candidates without deleting anything.

## Milestone C — Better Group and Reference Experience

- tune moment-group thresholds using real shoots;
- use scene tags/embeddings/person evidence for semantic refinement;
- preserve manual locks;
- surface reference candidates;
- support reusable external reference sets;
- allow multiple looks for one project.

Acceptance: travel/family/event photos separate into editing contexts that feel natural and can each receive an intentional reference look.

## Milestone D — Richer Non-destructive Editing

Extend the existing Recipe/XMP model:

- HSL;
- tone curve;
- vibrance/clarity/dehaze where compatible;
- crop/straighten where safe;
- richer portrait/skin intent;
- semantic/local adjustment representation.

Keep graceful fallback: edits that cannot be represented in Lightroom XMP may be used by direct export, but must not silently pretend to be round-trippable.

## Milestone E — Review and Personal Style

- fast before/after review;
- show exceptions and low-confidence items first;
- batch approve/reject;
- learn from accepted/rejected Recipes and user corrections;
- keep personal style local and editable.

## Milestone F — Platform Polish

Windows remains the primary production surface. Android now reuses the same project stores and shared UI for Library, Cull, effective Groups and Reference selection without duplicating editing logic. The remaining platform gap is cross-device project/preview transport; workstation-only Recipe editing and Lightroom handoff stay intentionally absent on Android.

## Later / Optional

Only after the core workflow is reliable:

- deeper GPU optimization;
- advanced RAW rendering;
- more sophisticated local AI retouch;
- optional additional interoperability.

Not early priorities:

- mandatory cloud services;
- multi-user/enterprise features;
- Lightroom database replacement;
- a separate Lightroom plugin architecture;
- destructive generative replacement as the default workflow.


Current workstation also exposes RAW/EXIF camera identity and capture time in Library so initial grouping evidence is inspectable.\n\nCurrent workstation status: RAW folder import, preparation progress, initial group overview, Groups view, evidence-backed Cull view, persisted photographer Cull decisions, and persisted per-group Reference selection are connected to the Rust core. ReferenceSet now previews adaptive exposure-only Recipes from cached exposure evidence and the Windows workstation has an explicit reviewed XMP handoff that leaves white balance untouched when unknown, excludes photographer-confirmed Reject photos, and refuses partial/overwrite writes. Cull, Reference and Review now use real cached RAW previews. Review has an on-demand canonical Before/After approximation; StyleProfile edits persist across reference changes; per-photo Recipe exception overrides persist by stable asset ID and feed both preview and explicit XMP handoff. Moment parents plus semantic children now persist separately and effective groups drive downstream workflow. Equivalent semantic children also preserve their IDs across repeated refinement, so reference/style bindings do not drift when membership is unchanged. Newly written XMP is parsed back and validated locally before success. The import path now prefers standard RAW/EXIF capture time and camera Make/Model for Moment grouping, falling back to filesystem time when unavailable. Exact RAW/DNG white-balance source evidence is now captured and persisted, including AsShotNeutral/AsShotWhiteXY where available. The remaining color gap is validated conversion from that provenance into Lightroom-compatible Temperature+Tint pairs. Cull now exposes existing segmentation-based people/face/subject evidence without changing quality scoring; true eye-state/expression remains pending a dedicated model. Other next gaps include real Lightroom/Camera Raw fixture validation and Android project/preview transport; the core Android Cull/Groups/Reference companion contract is already wired.


Implemented since the previous milestone: the workstation Cull view supports persisted photographer Keep/Review/Reject overrides, and Review has an on-demand Before/After approximation generated from the canonical Recipe. Remaining selection work is low-confidence triage and dedicated eye/expression evidence when a reliable local model is available.


Current WB status: provenance capture and persistence are implemented; Lightroom Temperature/Tint synthesis is intentionally still gated. Partial WB never reaches Recipe/XMP.
