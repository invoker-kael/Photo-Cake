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
- reference candidate shortlist ordering that respects photographer Cull decisions first, then reuses measured technical quality and group rank, while showing people/face evidence as context only;
- adaptive `color_sync`;
- target-bound per-photo Recipe materialization plus persisted additive per-photo review overrides;
- Lightroom XMP document and same-basename sidecar writing;
- optional direct export path;
- Windows workstation plus a real Android companion bridge for Library, Cull, effective Groups and Reference decisions.

These should be strengthened and wired together, not recreated.

## Milestone A — Reliable End-to-End RAW → XMP

Goal: make one real shoot usable through the entire non-destructive path.

Close remaining gaps around:

- preview-relative exposure analysis drives adaptive exposure Recipes; exact RAW/DNG white-balance provenance is captured and persisted, while validated conversion to Lightroom Temperature+Tint remains a later enrichment and not a blocker;
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
- user override persistence;
- concise measurable reason labels for each recommendation so the photographer can see whether blur, exposure or near-duplicate evidence drove the suggestion.

Acceptance: a large burst or event reduces to sensible Keep/Review/RejectSuggestion candidates without deleting anything.

## Milestone C — Better Group and Reference Experience

- tune moment-group thresholds using real shoots;
- use scene tags/embeddings/person evidence for semantic refinement;
- preserve manual locks;
- surface and explain reference candidates; the workstation shortlist now prioritizes photographer decisions, then AI class, measured technical quality and group rank, while people/face evidence remains context-only;
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
- exception-first Recipe review reuses saved per-photo exceptions plus existing Cull decisions to surface uncertain photos first, with a full All view still available;
- Lightroom handoff counts only deliverable Recipes after photographer-confirmed Reject photos are excluded and surfaces per-photo exception counts before writing;
- exception-first Cull triage is implemented for unconfirmed Review/RejectSuggestion items and analysis-pending photos; future model-specific confidence can refine ordering when reliable confidence exists;
- explicit "Confirm visible" batch confirmation is implemented for currently surfaced AI suggestions; it writes photographer decisions transactionally, skips pending items/existing decisions and protects selected References from batch Reject; richer batch-scope controls can follow only if real shoots need them;
- learn from accepted/rejected Recipes and user corrections;
- keep personal style local and editable.

## Milestone F — Platform Polish

Windows remains the primary production surface. Android now reuses the same project stores and shared UI for Library, Cull, effective Groups and Reference selection without duplicating editing logic. The Android companion contract is wired end-to-end at the domain/backend level: workstation snapshot export/baseline retention, Android snapshot hydration, mobile diff-to-patch generation, and workstation validated patch application are implemented. The remaining platform gap is the actual private transfer mechanism for snapshot JSON plus preview artifacts; workstation-only Recipe editing and Lightroom handoff stay intentionally absent on Android.

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


Current workstation also exposes RAW/EXIF camera identity and capture time in Library so initial grouping evidence is inspectable.

Current workstation status: RAW folder import, preparation progress, initial group overview, Groups view, evidence-backed Cull view, persisted photographer Cull decisions, and persisted per-group Reference selection are connected to the Rust core. ReferenceSet now previews adaptive exposure-only Recipes from cached exposure evidence and the Windows workstation has an explicit reviewed XMP handoff that leaves white balance untouched when unknown, excludes photographer-confirmed Reject photos, and refuses partial/overwrite writes. Cull, Reference and Review now use real cached RAW previews. Review has an on-demand canonical Before/After approximation; StyleProfile edits persist across reference changes; per-photo Recipe exception overrides persist by stable asset ID and feed both preview and explicit XMP handoff. Moment parents plus semantic children now persist separately and effective groups drive downstream workflow. Equivalent semantic children also preserve their IDs across repeated refinement, so reference/style bindings do not drift when membership is unchanged. Newly written XMP is parsed back and validated locally before success. The import path now prefers standard RAW/EXIF capture time and camera Make/Model for Moment grouping, falling back to filesystem time when unavailable. Exact RAW/DNG white-balance source evidence is now captured and persisted, including AsShotNeutral/AsShotWhiteXY where available. The remaining color gap is validated conversion from that provenance into Lightroom-compatible Temperature+Tint pairs. Cull now exposes existing segmentation-based people/face/subject evidence without changing quality scoring; true eye-state/expression remains pending a dedicated model. Other next gaps include real Lightroom/Camera Raw fixture validation and Android project/preview transport; the core Android Cull/Groups/Reference companion contract is already wired.


Implemented since the previous milestone: the workstation Cull view supports persisted photographer Keep/Review/Reject overrides plus a Triage mode that hides already-confirmed and obvious AI Keep items, orders unresolved RejectSuggestion/Review candidates for attention, and keeps analysis-pending photos visible. Review has an on-demand Before/After approximation generated from the canonical Recipe. Remaining selection work is dedicated eye/expression evidence when a reliable local model is available.


Current WB status: provenance capture and persistence are implemented; Lightroom Temperature/Tint synthesis is intentionally still gated. Partial WB never reaches Recipe/XMP.

Recipe triage now defaults to an exception-first attention set built from existing photographer/AI Cull evidence plus persisted per-photo overrides; it does not introduce a second scoring model. The Lightroom page now previews the real deliverable XMP count after confirmed Rejects are removed and shows how many deliverable photos carry photo-specific exceptions.

Review completion is now explicit: "Looks good" confirmations are persisted against a fingerprint of the current final Recipe, so resolved attention disappears from Triage but automatically returns after meaningful Reference/style/override changes. Lightroom handoff surfaces remaining review attention without forcing it to zero.

Direct Export remains a guarded gap rather than a fake workstation feature. The reusable export coordinator/checkpoint/raster pieces exist, but production Windows Direct Export should wait for a real RAW demosaic + canonical Recipe renderer with trustworthy color and metadata handling.

Lightroom handoff now has a visible read-only preflight: the workstation resolves the actual XMP targets when the handoff view opens, surfaces existing-sidecar conflicts before write, and keeps the final core write-time preflight as the authoritative race-safe guard. The handoff summary also counts photographer-confirmed Rejects from group membership rather than from the already-filtered Recipe list, so skipped-photo reporting remains accurate.

Reference workflow now supports reusing a look across groups while keeping each group's own Reference and adaptive baseline. This targets travel/family shoots where several editing contexts should feel consistent without copying fixed exposure edits between different scenes.

Lightroom handoff is now idempotent for Photo-Cake-owned edit state: matching existing sidecars are preserved and counted as already current, missing peers can still be created, and any different or unverifiable XMP remains a hard whole-group conflict. This keeps reruns safe without introducing overwrite behavior.


Batch Lightroom handoff now closes the large-shoot delivery loop: the workstation can write all currently ready groups in one action, preflights the full selected set before the first write, preserves already-current Photo-Cake sidecars, and rolls back sidecars newly created earlier in the batch if a later group hits a race-time conflict or write failure.


Recipe review now supports large-shoot completion as one explicit workflow unit: `Confirm visible` re-resolves canonical final Recipes on the workstation, validates the full requested set, and persists all fingerprints transactionally. Review surfaces attention/confirmed/exception/Reject-skipped counts, and Lightroom carries remaining review attention into the delivery summary without turning it into a mandatory gate.

Lightroom delivery is now exception-first as well: after writing, the core verifies the complete target set against current Recipe state and rolls back newly-created sidecars on verification failure. The workstation defaults to a Needs action delivery queue, prioritizing conflicts/unresolved/review-attention/missing-XMP groups and carrying session-scoped verified-target counts. A representative Adobe Camera Raw-style XMP fixture is committed to exercise namespace/extra-metadata compatibility; validation against fixtures captured from real Lightroom/Camera Raw installations remains a separate evidence step before claiming full interoperability parity.


Reference consistency now scales beyond one target at a time: one group can act as a standard look and synchronize its StyleProfile to multiple selected referenced groups transactionally. The UI uses source-plus-multi-select ergonomics, omits already-matching groups, and then relies on the existing adaptive Recipe and exception-review path rather than introducing a preset-copy editing engine. This closes a common large travel/family workflow: establish look -> synchronize similar contexts -> review only exceptions -> Lightroom handoff.


The workstation now has a workflow cockpit instead of a static process legend: canonical batch/project/XMP state is reduced to a deterministic next photographer focus and live per-stage counts. A Continue workflow action jumps directly to the current attention stage while all destructive or photographer-authoritative transitions remain explicit. This adopts the useful “guided batch flow” behavior of mature AI photo editors without adding a generic node builder or a second workflow engine.


Grouping correction now closes the main real-shoot gap in Milestone C: semantic splits can be reverted to their original Moment, automatically locked Moments can be reopened, adjacent parent groups can be merged, and a group can be split at a chosen photo. Manual merge/split persists an explicit MANUAL basis and is locked before Reference lineage begins, so automatic grouping handles the common case while photographer corrections remain authoritative.


Reference setup now scales across large shoots without weakening photographer authority: unreferenced groups can multi-select their existing best-supported shortlist candidate and create all chosen References transactionally. Batch eligibility excludes evidence-pending and unresolved RejectSuggestion candidates, existing References are never overwritten, and every group remains individually adjustable afterward.

Recipe review now closes the standard-photo batch loop for large shoots: after Reference and adaptive Recipe synchronization, the workstation can explicitly confirm one clear group or all currently clear groups without forcing a full All-view pass. Eligibility reuses the same Cull/exception evidence as Triage, the backend revalidates every selected group, and all new Recipe fingerprints commit transactionally. Exception or evidence-pending groups remain individual review work before Lightroom handoff.

Lightroom handoff now uses a safe delivery cohort for large shoots: review-clear, conflict-free groups can be explicitly multi-selected and delivered together while XMP conflicts, preflight failures and Recipe-attention groups remain isolated in Needs action. Per-group preflight failures no longer collapse the whole delivery view, missing XMP is reported explicitly, and photographers can refresh filesystem checks after resolving external Lightroom edits. Individual handoff remains available for deliberate exception groups.

Recipe exceptions now scale across repeated conditions without replacing adaptive editing: one photo's saved exception can act as a source for selected same-group peers, with Exposure/Contrast/Saturation synchronized independently. Target photos keep their own Reference-derived adaptive baselines and unselected exception fields, changed overrides commit transactionally, and affected photos automatically return to Review before safe Lightroom delivery.


Exposure-bracket protection now extends through delivery routing: Analyze evidence identifies conservative 3/5/7/9-frame same-composition exposure ladders, Cull preserves them, full bracket Moments survive semantic refinement, bracket RAWs are excluded from ordinary Adaptive Recipe targets, automatic batch Reference and safe batch XMP skip pending bracket groups, and Lightroom preflight exposes the exact HDR source set as a merge action. The photographer can mark an external HDR merge complete; that completion is tied to the current bracket membership fingerprint and automatically invalidates if the stack changes. Mixed groups can then rejoin safe batch delivery for ordinary peers, while pure bracket source groups can become workflow-current without fake XMP. Actual HDR RAW merging remains intentionally outside the implemented path until the RAW renderer/merge/color/metadata boundary is production-grade; Lightroom/Camera Raw remains the merge destination for now.


Moment-level Quick Cull now closes another large-shoot efficiency gap: groups with complete non-HDR evidence can expose an explicit best-frame plan, multi-select eligible moments, and persist the whole selected set transactionally. The shortcut keeps the rank-1 frame, protects every people/family alternate as Review, treats missing people evidence conservatively as Review-only, and only rejects clear non-people near duplicates with a material quality gap after people evidence is complete. Existing photographer decisions, References, pending evidence and bracket groups block the batch path, so rapid travel/burst selection does not bypass exception review or overwrite established lineage.


Quick Cull safety now covers scenic work as well as people photography. Cull recommendations expose measured duplicate similarity plus existing scene tags; people alternates remain protected, while landscape/architecture/other non-people frames can only enter the Quick Cull Reject cohort when scene evidence is complete and consistent, similarity is at least 98.5%, and the primary has a material quality lead. Selected batches show projected Keep/Review/Reject counts and persist an atomic operation record with guarded Undo. Undo refuses changed decisions, changed group topology or downstream References instead of erasing later photographer intent. This keeps the PixelCake-style efficiency principle—standardize the obvious batch, then review exceptions—without turning Photo-Cake into a portrait-only or destructive workflow.


### Reference readiness for mixed travel shoots

Completed: Cull → Reference handoff now uses a canonical, scene-visible readiness preflight. Large travel/family/scenery sessions can select all groups that are genuinely ready, see which groups are blocked by pending Cull evidence or HDR merge, and keep photographer Keep decisions authoritative. The UI and backend share the same suggested candidate instead of maintaining separate ranking rules.

This follows the useful part of the PixCake-style workflow pattern—establish a trusted standard image, synchronize the shared look in batches, then spend attention on exceptions—while retaining Photo-Cake's own adaptive Reference → Recipe model. It does not clone proprietary algorithms, does not make landscape work behave like portrait work, and does not introduce fake Direct Export.


### Mixed-scene look synchronization

Completed: Reference look reuse now has scene-aware preflight for mixed travel shoots. The default batch selection favors people/family-to-people/family and scenic groups that share existing scene evidence, while landscape↔food, portrait↔pure scenery, or incomplete-evidence pairs remain visible as explicit Review targets instead of being auto-selected.

This extends the useful “standard image → synchronize shared look → review exceptions” workflow pattern without making Photo-Cake portrait-only. StyleProfile stays shared, each group keeps an independent Reference/adaptive exposure baseline, and Recipe Review remains the place for single-photo exceptions.


### Canonical exception-first Recipe Review

Completed: Recipe Review triage and clear-group confirmation now consume the same backend preflight instead of duplicating Cull/exception logic in the UI. Saved exceptions and uncertain photos rise first, straightforward Recipes form safe clear cohorts, missing evidence stays Pending, and current confirmations remain fingerprint-bound.

Mixed travel sets remain first-class: scene context is visible for landscape/architecture/night/people groups, while HDR bracket sources are reported separately and never flattened into ordinary Recipes. This keeps the workflow on the existing Reference → adaptive Recipe → exception Review → Lightroom path.


### Core editing quality: adaptive tone

Completed: the core Reference → adaptive Recipe path now goes beyond exposure-only matching. ExposureAnalysis v2 records preview luminance percentiles and clipping evidence, then generates bounded per-photo Highlights/Shadows after exposure alignment. This improves mixed travel, landscape, architecture, night and people sets without creating scene-specific duplicate pipelines.

Recipe Review now previews those tone adjustments and lets the photographer override or synchronize Highlights/Shadows per photo. Existing Lightroom XMP delivery carries the values directly. Legacy projects continue with exposure-only fallback when percentile evidence is unavailable.


### Core editing quality: contrast and color consistency

Prepared after adaptive highlight/shadow matching: extend the same Reference-relative evidence path to per-photo Contrast and Saturation. Flat targets receive modest contrast toward the Reference, overly hard targets are softened, clipped targets are protected from aggressive contrast, and muted/over-saturated targets receive bounded color-intensity normalization. No additional workflow layer is introduced.


### Core editing quality: exposure and preview fidelity

Prepared in the same batch: P50 now acts as a conservative secondary exposure refinement with P90 highlight-headroom protection, while preserving the Reference/StyleProfile exposure target. The lightweight edited preview applies Exposure in linear-light sRGB before its perceptual tone/color approximation, making Review decisions substantially closer to a real photographic exposure adjustment without claiming RAW-engine parity.


### Core editing quality: channel-aware highlight protection

Prepared as the next quality batch: ExposureAnalysis v4 makes clipping RGB-channel-aware, then feeds that evidence into adaptive Exposure, Highlights and Contrast. Review preview tone recovery also moves from equal RGB offsets to hue-friendlier luminance remapping. This specifically improves saturated sunsets, neon, stage lighting, colored architecture and skin/specular highlights.


### Core editing quality: independent white/black points

Prepared after channel-aware clipping protection: ExposureAnalysis v5 adds P02/P98 endpoint evidence and adaptive Recipe gains independent Whites/Blacks instead of forcing Highlights/Shadows to control the entire tonal range. The controls are carried through Review exceptions, preview rendering and Lightroom XMP.


### Core editing quality: protected color recovery

Prepared after white/black point control: ExposureAnalysis v6 adds colorfulness P25/P75 and adaptive color matching shifts positive recovery from global Saturation to saturation-aware Vibrance. The goal is stronger muted colors without overdriving already-saturated skies, neon, clothing and similar scene elements.

### Core editing quality: quality-first exception gate

Completed after protected adaptive color recovery: canonical Recipe Review now adds a quality-first guard before batch confirmation. Risky generated edits—large exposure/tone/endpoint/contrast/color moves, clipped or low-confidence preview evidence, and extra color pressure on already-saturated scenes—return to exception Review instead of joining the clear cohort.

This intentionally copies only the useful workflow principle from mature batch editors: automate the standard images, then spend human attention on exceptions. The gate is shared by people and scenery, does not create a portrait-only branch, and does not weaken the existing Direct Export readiness boundary.

### Core editing quality: low-light and dynamic-range protection

Completed after the quality-first exception gate: adaptive tone matching now protects deep shadows, black anchors, white headroom and naturally wide scene contrast before a Recipe reaches Review. The generator backs off shadow opening in low-key frames, limits opposing Highlights/Shadows compression on wide-range scenes, avoids casually lifting deep blacks, and reduces positive Whites near the endpoint.

The quality preflight adds explicit deep-shadow-lift and dynamic-range-compression risks for manual overrides, legacy Recipes or remaining edge cases. This improves night, landscape, architecture and mixed travel work while staying on the same Reference-driven engine used for people photography.

### Core editing quality: adaptive Reference strength

Completed after low-light/dynamic-range protection: Reference synchronization now measures whether a target frame differs only in exposure or differs materially in tonal/color structure. Exposure-only variants stay near full synchronization strength; scene outliers automatically receive a softer version of the Reference-relative tone, endpoint, contrast and color corrections.

Review surfaces the exact adaptation percentage, and very weak fits become explicit REFERENCE_MISMATCH exceptions. This moves the workflow closer to the useful mature batch-editor pattern—one trusted standard image, strong automation for ordinary variants, conservative treatment of scene outliers, and human attention only where the shared look stops being trustworthy.

### Core editing quality: low-light color integrity

Completed after adaptive Reference strength: automatic color recovery now protects two failure modes common in travel and family work—lifting chroma noise in genuinely dark frames, and over-driving already saturated highlight colors in neon, sunsets and stage lighting.

The adaptive Vibrance path now tapers its own strength from preview evidence before Recipe materialization, while Recipe Review retains specialized quality exceptions if a final edit remains risky. The goal is fewer visibly dirty night shadows and fewer clipped/unnatural saturated highlights without turning off useful batch color matching.

### Core editing quality: preview perceptual fidelity

Completed after low-light color integrity: the Review renderer now better preserves colored-highlight relationships, applies contrast through luminance, smooths tonal-zone masks and prevents positive chroma edits from creating avoidable preview clipping.

This does not replace RAW rendering. It makes the exception-review surface more trustworthy so the photographer can judge direction before the same canonical Recipe is serialized to Lightroom XMP.

### Core editing quality: midtone structure consistency

Completed after preview perceptual fidelity: Reference-driven Contrast now follows both sides of the midtone in exposure-invariant log space. This improves batch consistency when images have similar brightness but different local tonal shape.

Asymmetric scenes no longer receive an unnecessarily strong global Contrast move simply because one tonal side differs from the Reference, and endpoint pressure further limits positive hardening near black/white boundaries.

### Core editing quality: color-distribution-aware saturation

Completed after midtone structure consistency: global Saturation matching now differentiates broadly over-colorful frames from otherwise muted images that contain only a small saturated tail.

This reduces a common batch-editing artifact where neon signs, flowers or sunset highlights cause the whole image—including skin, walls and muted backgrounds—to be desaturated. Strong manual/style-driven desaturation on the same mixed distribution is surfaced as a Review exception instead of silently batch-confirming.

### Core editing quality: coordinated color controls

Completed after distribution-aware Saturation: adaptive color matching now resolves Saturation and Vibrance as one decision instead of allowing opposite global/selective controls to fight each other.

Mixed distributions that are dull in muted regions but excessively saturated in the tail are recognized as cases that need more selective color tools than Photo-Cake currently exposes. They are kept conservative and routed to Review rather than being silently batch-corrected with contradictory slider values.

### Core editing quality: selective Color Mixer saturation

Completed after coordinated global color controls: mixed-color conflict frames can now receive a conservative eight-zone Color Mixer Saturation starting point instead of relying only on global Saturation/Vibrance.

The implementation adds hue-distribution evidence, cache versioning, Reference-relative selective saturation, Lightroom XMP round-trip support, perceptual preview approximation, and Review visibility. Conflict frames remain exception-first and still require photographer confirmation. Hue and Luminance Color Mixer automation remain intentionally out of scope until stronger evidence is available.
