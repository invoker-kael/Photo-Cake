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

- RAW scanning and catalog persistence;
- stable asset identity;
- preview extraction/cache and local analysis;
- portrait/scene classification;
- first-pass moment grouping;
- semantic embedding-based group refinement;
- batch/job/export infrastructure;
- local model loading, segmentation and embeddings;
- culling decision model;
- ReferenceSet and StyleProfile;
- adaptive `color_sync`;
- target-bound per-photo Recipe materialization;
- Lightroom XMP document and same-basename sidecar writing;
- optional direct export path;
- Windows and Android application shells.

These should be strengthened and wired together, not recreated.

## Milestone A — Reliable End-to-End RAW → XMP

Goal: make one real shoot usable through the entire non-destructive path.

Close remaining gaps around:

- end-to-end orchestration from catalog group/reference to Recipes/XMP;
- persistence of semantic groups, references, recipes and review state;
- Lightroom/Camera Raw compatibility tests for emitted XMP;
- safe overwrite/update behavior for an existing sidecar;
- clear Windows workflow for apply/review/handoff.

Acceptance: a target group can use a selected reference, produce different adaptive Recipes per photo and write Lightroom-readable XMP beside the original RAWs without changing RAW bytes.

## Milestone B — Useful Smart Culling

Improve culling from a data model into a practical selection assistant:

- measurable sharpness/blur and exposure evidence cached during Analyze;
- group-scoped duplicate/burst ranking from existing embeddings;
- face/eye/expression evidence when available;
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

Windows remains the primary production surface. Android reuses the same project/workflow concepts for mobile review, selection and reference management without duplicating editing logic.

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


Current workstation status: RAW folder import, preparation progress, initial group overview, Groups view and evidence-backed Cull view are connected to the existing Rust core. Reference selection/apply and explicit Lightroom handoff are the next UI gaps; their core ReferenceSet/Recipe/XMP logic already exists.


Implemented since the previous milestone: the workstation Cull view now supports persisted photographer Keep/Review/Reject overrides on top of AI suggestions. Remaining selection work is richer preview/compare UX and semantic evidence such as eyes/expression when reliable local evidence is available.
