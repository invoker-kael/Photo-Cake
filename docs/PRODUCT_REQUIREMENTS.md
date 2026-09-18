# Photo-Cake Product Requirements

## Product Goal

Photo-Cake is a personal, local-first, semi-automatic photography assistant for processing large RAW shoots with less repetitive Lightroom work.

It is not intended to replace Lightroom or become a generic image editor. The normal result is an intelligently prepared RAW + XMP workflow that the photographer can continue editing, with direct final export available when Lightroom is unnecessary.

## User Workflow

```text
RAW import
  -> fast catalog + previews
  -> smart culling suggestions
  -> meaningful photo groups
  -> choose/edit one or more references
  -> learn desired look
  -> adapt that look per photo
  -> review exceptions/results
  -> Lightroom XMP or direct export
```

## Non-destructive and Storage Requirements

- Original RAW bytes are immutable.
- Existing source folders remain the photo source of truth.
- Normal Lightroom handoff is the original RAW plus a small same-basename `.xmp`.
- An existing XMP is treated as user-owned editing data and must not be silently overwritten.
- Do not create full-size TIFF/JPEG working copies by default.
- Preview/model/cache data belongs in managed application storage.
- JPEG/TIFF output is generated only when direct export is requested.
- No mandatory cloud upload, account or subscription workflow.

## Smart Culling

Culling reduces what the user must inspect. It should combine technical and content evidence such as:

- focus/sharpness and blur;
- severe exposure failure;
- closed eyes and expression quality where people are present;
- duplicate/near-duplicate burst detection;
- obvious low-value frames.

Output is advisory: Keep, Review or RejectSuggestion. The user owns the decision and originals are never deleted automatically.

Within duplicates/bursts, the goal is to surface the strongest candidates rather than merely mark every similar image as bad.

## Photo Grouping

Photo Group is the main editing context.

Grouping is two-stage:

1. immediately create conservative moment groups from RAW/EXIF capture time, camera identity and filename sequence; use filesystem modification time only when embedded metadata is unavailable;
2. refine only inside those groups using local classification and visual embeddings.

Moment groups remain persisted as parent structure. Semantic refinement is stored as child groups and becomes the effective editing/culling context only when evidence is complete. This keeps capture chronology available for future model/version re-refinement instead of destructively replacing it.

Useful contexts include portrait sequences, travel scenes, landscape moments, indoor/family scenes and night photography. Manual group decisions override automatic refinement.

## Reference-driven Editing

The photographer can select a preferred edited photo or a reusable reference from another compatible group.

```text
Reference photo
   + editable StyleProfile
   -> shared GroupColorIntent
   -> compare against each target photo
   -> per-photo Recipe
```

The reference establishes the desired look. Photo-Cake must adapt exposure/white balance and later semantic/local controls to each target image instead of blindly copying reference numbers.

Multiple reference sets may coexist for different looks/scenes. The photographer's selected reference must persist independently from AI suggestions and survive reopening the project. A newly imported collection may form new Photo Group IDs and can require explicit rebinding.

Selecting a reference does not itself imply that edits are applied. On Windows the photographer can persist group-level exposure bias, contrast and saturation in the ReferenceSet StyleProfile; changing the selected reference preserves those preferences. White-balance controls remain unavailable until reliable RAW/metadata evidence exists, so Photo-Cake never fabricates Kelvin/tint merely to populate XMP.

Reference candidate ordering must remain evidence-backed and photographer-first. Exclude photographer-confirmed Reject items, prioritize explicit photographer Keep decisions, then reuse the existing AI Keep/Review class, measured technical quality score and group-relative rank. Do not invent a separate reference-confidence score. Existing people/face evidence may be displayed as context but must not silently change candidate ranking. AI RejectSuggestion remains advisory and may stay available as a last-resort candidate when the photographer has not rejected it.

## Recipe Requirements

Recipe is the canonical editable representation of Photo-Cake decisions.

Current/basic controls include:

- exposure;
- contrast;
- highlights;
- shadows;
- temperature;
- tint;
- saturation.

The model should expand without changing the workflow to support HSL, curves, skin/color preferences, masks and other non-destructive controls.

Every applied target photo has its own Recipe with reference lineage. Photographer exceptions are stored as small per-asset additive Recipe review overrides (currently exposure, contrast and saturation) rather than frozen full-Recipe copies, so changing the reference or group StyleProfile can regenerate the base Recipe without losing intentional single-photo corrections.

## Lightroom Workflow

Primary handoff:

```text
IMG_0001.CR3
IMG_0001.xmp
```

The XMP must contain only mapped edits, remain small, and be traceable to the target Recipe/asset. Photo-Cake re-parses every newly written sidecar and verifies Recipe identity, target asset and mapped numeric values before reporting handoff success. Lightroom/Camera Raw should be able to continue from those edits.

No early requirement for Lightroom catalog modification, database writing or a Lightroom plugin.

## Direct Export

The same Recipe can feed the existing renderer/export pipeline for requested JPEG/TIFF output. Direct export is a supported product path, but it must not become a separate editing model.

## Platforms

Windows is the primary workstation for large RAW collections, batch processing, GPU acceleration and Lightroom handoff.

Android is a companion for selection, preview, reference management and lightweight processing. Both platforms reuse shared core workflow rules and data semantics.

## Success Criteria

For a large personal shoot, the user can:

1. point Photo-Cake at existing RAWs without copying them;
2. quickly see real cached RAW previews and useful groups;
3. reduce manual review with culling suggestions;
4. select a preferred look/reference;
5. have Photo-Cake adapt it across similar photos;
6. review exceptions rather than every repetitive adjustment, with persistent per-photo corrections only where needed and an on-demand Before/After preview;
7. create tiny XMP sidecars for Lightroom or explicitly export final images.


## Workstation Entry

On Windows, the user can choose an existing RAW folder with the native folder picker. Photo-Cake registers the RAWs in place, creates initial moment groups and runs local analysis. Finishing the preparation queue means the photos are ready for culling/group/reference work; it does not imply that Photo-Cake silently exported or destructively edited them.


## Culling Evidence Behavior

Technical culling starts from locally measured preview evidence: sharpness/blur and severe exposure quality. Existing embeddings are reused for near-duplicate/burst ranking inside the same photo group. Expression, composition and other semantic quality dimensions are only added when dedicated evidence exists; Photo-Cake must not fabricate those scores. The workstation shows pending items while analysis is incomplete.


## Photographer Culling Decisions

AI culling remains advisory. The workstation separates the suggested decision from the photographer's saved decision. The photographer can mark Keep, Review or Reject, clear that override to return to the suggestion, and re-import the same RAW without losing the decision because it is stored against the stable asset ID. Reject is workflow state only; it never deletes the RAW.


## Partial Edit Evidence

Photo-Cake may apply a subset of trustworthy adjustments. Current local preview analysis supports relative exposure adaptation between photos in the same photographic context. If RAW white-balance evidence is unavailable, temperature and tint remain untouched rather than guessed. Lightroom XMP contains only fields backed by the current Recipe/evidence.


## Explicit Lightroom Delivery

Lightroom delivery is a deliberate photographer action, not an automatic batch stage. Before writing, Photo-Cake shows the selected reference and adaptive Recipe count. Photographer-confirmed Reject photos are excluded; AI suggestions alone do not remove photos from delivery. If any target RAW already has a same-basename XMP, Photo-Cake stops the entire group before creating new sidecars so existing edits and group consistency are preserved.


## Visual Review Surface

The workstation reuses the existing PreviewStore artifacts in Cull and Reference views. Cached embedded RAW JPEG previews are displayed directly from managed local storage; the UI must not create full-size rendered working copies merely to show thumbnails. Preview availability follows Analyze progress and missing previews degrade to a lightweight RAW placeholder.


## Before / After Review

Review can render a small edited preview from the already-cached embedded RAW JPEG using the current canonical Recipe. It is explicitly an approximation for visual direction and exception review, not a replacement RAW renderer. The backend regenerates the Recipe from the current ReferenceSet, StyleProfile, cached evidence and per-photo override before rendering, so the preview cannot diverge from the decision chain used for XMP handoff. No full-size working copy is created.


## RAW Metadata Evidence

Import performs a best-effort, read-only EXIF metadata pass before initial grouping. Standard `DateTimeOriginal`/`DateTime` populates the capture timeline and Make/Model forms camera identity; unreadable or unsupported containers fall back to the existing file timestamp without blocking import. EXIF white-balance mode alone is not sufficient to synthesize Lightroom temperature/tint, so Photo-Cake still leaves WB untouched until reliable numeric RAW/color evidence exists.


## Portrait Culling Evidence

Cull may surface cached portrait evidence already produced by local segmentation: detected people/faces, primary-subject ratio and people confidence. These values are advisory context and do not replace technical quality scoring or photographer decisions. Photo-Cake must not label eyes as closed/open or infer expression quality until a dedicated reliable model provides that evidence.


## Metadata Transparency

The workstation Library should expose the camera identity and capture time used for grouping so the photographer can see whether a shoot is using embedded RAW/EXIF evidence or a fallback. Missing metadata remains visible as unavailable/fallback rather than being silently invented.


## RAW White Balance Evidence

Photo-Cake now captures exact DNG/TIFF white-balance source evidence when present, including `AsShotNeutral` and `AsShotWhiteXY`, and persists that evidence by stable asset ID. These raw values are provenance, not Lightroom slider values.

White balance remains a paired adjustment contract: Temperature and Tint must both be supported by reliable derived evidence before Reference, Recipe or XMP may apply them. A partial measurement is treated as unknown and omitted. Photo-Cake must not convert rendered preview colors or EXIF Auto/Manual white-balance mode into fabricated Lightroom Kelvin/Tint values.


## Android Companion Boundary

Android is a decision companion, not a second RAW workstation. Its real bridge reuses the same local project stores for Library context, effective groups, Cull recommendations/reviews and Reference selection. Workstation-only capabilities such as RAW import/analyze control, StyleProfile editing, Recipe exception editing, direct export and Lightroom XMP writing must remain absent from the Android capability surface.

Cross-device project transfer/synchronization is still a separate gap. Until that transport exists, the companion operates on project state present in its own app data; do not invent cloud accounts or duplicate photography logic to bridge devices.


## Explainable Culling

Every AI culling recommendation should expose concise reasons derived only from measured evidence. Current reasons cover strong technical candidate, low sharpness, blur risk, exposure risk, near-duplicate status and low technical quality. Subjective composition, eye state and expression must not be invented as explanations when dedicated evidence is absent.


## Companion Transport Contract

Cross-device companion transport is local-first and business-logic-neutral. The workstation exports a portable CompanionSnapshot containing only the mobile decision context: stable asset identities, filenames/metadata, effective groups, cached culling recommendations, photographer culling reviews, reference state, RAW metadata evidence and preview transport indexes. Desktop RAW absolute source paths are intentionally excluded.

The companion returns a CompanionDecisionPatch containing only culling and reference changes. Applying a patch must validate snapshot/batch identity, asset/group membership, rejected-reference conflicts and concurrent workstation edits before any write. Transport medium is intentionally unspecified so local file transfer, LAN transfer or another private mechanism can be added without changing the photography model.


Companion conflict handling is scoped to the decisions being changed. Unrelated workstation edits do not invalidate a mobile patch; overlapping culling/reference changes still fail closed instead of silently overwriting photographer work.


## Companion Snapshot Hydration

Android can now hydrate a received CompanionSnapshot into its local project stores without RAW files. Imported assets use synthetic companion:// source references, imported groups are the workstation's effective groups, batch items are marked prepared, and the snapshot baseline is persisted separately.

Android culling must use the snapshot's precomputed recommendations instead of rerunning workstation analysis. Mobile changes are reduced to a CompanionDecisionPatch against the persisted baseline. A different snapshot for the same batch is rejected until the previous mobile decisions are synchronized, preventing silent replacement of unsynced photographer work.


## Exception-first Culling Triage

Cull defaults to an exception-first Triage view for large shoots. Triage shows unresolved AI Review / RejectSuggestion items and analysis-pending photos, while hiding AI Keep items and photos that already have an explicit photographer decision. Within unresolved scored items, RejectSuggestion comes first and lower technical quality is surfaced before stronger candidates. This is presentation ordering only: it does not create a new score, change AI evidence, override photographer decisions or delete/exclude source files. The photographer can switch to All at any time to inspect the complete culling set.

"Confirm visible" is an explicit photographer action that transactionally persists only currently visible, scored and previously unconfirmed AI suggestions. Pending photos and existing photographer decisions are untouched. A currently selected group Reference is protected from batch conversion of RejectSuggestion into Reject; the photographer must first choose another Reference or make that decision individually. In Triage this confirms unresolved Review/RejectSuggestion items; in All it can also confirm visible AI Keep suggestions.

## Exception-first Recipe Review

Recipe Review defaults to a Triage view instead of asking the photographer to inspect every adaptive Recipe. Triage reuses existing decision evidence only: saved per-photo Recipe exceptions, explicit photographer `Review` culling decisions, and unresolved AI `Review` / `RejectSuggestion` recommendations. Photographer-confirmed `Reject` photos are not part of Recipe triage because they are excluded from Lightroom delivery. Switching to All restores the complete Recipe set for a full visual pass.

This ordering is presentation logic, not a new quality model. Within the attention set, persisted per-photo exceptions come first, then explicit photographer Review decisions, then unresolved AI culling warnings; measured culling quality and group rank only break ties. Photo-Cake does not invent a new review score or silently modify a Recipe.

Lightroom handoff must report the actual deliverable target count after photographer-confirmed Reject photos are excluded. The handoff summary also exposes how many confirmed Rejects are skipped and how many deliverable photos contain persisted per-photo Recipe exceptions, so the number shown before writing matches the intended XMP batch.

### Recipe review completion

A photographer can explicitly mark the current adaptive Recipe as "Looks good" without creating a fake zero-value override. The confirmation is bound to a deterministic fingerprint of the actual target, Reference lineage and final Recipe adjustments after any per-photo override. Ephemeral Recipe IDs and display names do not affect the fingerprint.

A matching confirmation removes that photo from Recipe Triage. If the Reference, group style, adaptive result or per-photo override changes, the fingerprint no longer matches and the photo automatically returns to attention. The photographer can also reopen a confirmed review manually. Lightroom handoff reports remaining review-attention items but does not block the photographer from writing XMP.

### Batch Recipe review completion

Recipe Review exposes an explicit `Confirm visible` action for large shoots. In Triage it confirms only the currently surfaced attention set; in All it confirms every currently visible, unconfirmed adaptive Recipe. Existing confirmations are skipped rather than rewritten.

The UI sends only group and asset identities. The workstation resolves the current canonical final Recipes again from project state, validates that every requested photo is still editable and belongs to the requested group, rejects duplicate asset requests, and only then asks the core store to persist confirmations.

The core computes every Recipe fingerprint before opening the write transaction and persists the complete set in one SQLite transaction. Any invalid Recipe, missing target, duplicate target or storage failure leaves the batch unconfirmed instead of producing a partial review state. Individual `Looks good` and `Reopen review` actions remain available for deliberate exceptions.

Review exposes attention, confirmed, per-photo exception and skipped-Reject counts at a glance. Lightroom handoff carries the remaining attention count forward as delivery context, but it remains advisory rather than a hard XMP gate.

### Direct export boundary

The existing direct-export core provides collision-safe planning, checkpoints and a baseline raster renderer for already-decoded images. It is not yet a production RAW demosaic/edit renderer. The Windows photography workflow must therefore keep Lightroom XMP as the real RAW handoff and must not expose a misleading "Direct Export" action until canonical Recipe evaluation can be rendered from RAW with reliable color/metadata behavior.

### Lightroom XMP preflight

Opening the Lightroom handoff view performs a read-only group preflight against the same target-bound Recipes used for writing. It resolves the actual sidecar paths and reports existing same-basename XMP files, including mixed-case extensions, before any write starts.

A detected conflict disables the write action and shows the conflicting filenames. The final writer still repeats the same whole-group preflight immediately before create-new writes, so an XMP created externally after the UI check cannot cause a silent overwrite or partial group update.

### Reusable group look

A photographer can copy the shared StyleProfile from another referenced group without replacing the target group's selected Reference. The copied values are look preferences, not a blind copy of the source group's resolved per-photo numeric edits.

The target group keeps its own Reference photo and adaptive photographic baseline, then regenerates its own Recipes from that baseline plus the copied look. Any previously confirmed Recipe review whose final adjustments change becomes stale automatically through the existing Recipe fingerprint contract.

### Idempotent Lightroom handoff

An existing same-basename XMP is not automatically a conflict when it is a Photo-Cake sidecar whose target asset and currently supported edit fields already match the current Recipe. Recipe IDs are intentionally ignored for this equivalence check because adaptive Recipes are regenerated and receive new runtime IDs even when their effective edit state is unchanged.

Matching sidecars are preserved byte-for-byte and skipped. Missing sidecars may be created in the same group. Any existing XMP that cannot be parsed as Photo-Cake state or whose supported fields differ remains a hard conflict and aborts the group before new files are created. Extra Lightroom fields that Photo-Cake does not manage are never overwritten.


### Batch Lightroom handoff

The workstation can hand off every currently ready Lightroom group in one explicit action. A group is batch-ready only when it has a selected Reference, resolved target-bound Recipes, no pending evidence, a completed read-only XMP preflight and at least one missing sidecar.

Before creating the first new XMP, the core preflights every selected group. Any conflicting or unverifiable existing XMP aborts the whole batch before writes begin. Matching Photo-Cake sidecars are treated as already current and are preserved byte-for-byte.

Each group repeats its race-safe preflight immediately before writing. If a later group fails because the filesystem changed after the batch preflight, Photo-Cake removes sidecars newly created by earlier groups in that same batch. Pre-existing matching sidecars and source RAW files are never removed or modified.


### Verified Lightroom delivery

A Lightroom handoff is successful only after the complete deliverable group is re-read from disk and every sidecar still matches the current canonical Recipe state. This verification includes both newly-created sidecars and previously-current Photo-Cake sidecars. If any expected sidecar is missing or has changed between preflight/write and final verification, the operation fails and removes sidecars newly created by that operation. Batch handoff keeps its existing cross-group rollback behavior.

The Lightroom page defaults to a `Needs action` delivery view for large shoots. It prioritizes XMP conflicts, unresolved groups, remaining Recipe attention and missing sidecars, while hiding groups that are already current and review-clear. `All` restores the complete group list. The summary exposes action groups, current groups, conflict groups and the number of XMP targets verified in the current session.

A handoff result is invalidated whenever canonical delivery inputs change, including culling deliverability, group/reference state, StyleProfile, per-photo Recipe exceptions, grouping or active project. Review completion alone does not invalidate a verified XMP because it does not change the Recipe.


### Batch Reference look synchronization

For shoots with several related scenes, the photographer can choose one referenced group as the source look, multi-select other referenced groups, and explicitly `Sync look to selected`. Groups whose effective visual StyleProfile already matches the source are omitted from the selectable target list.

The synchronization copies only the shared StyleProfile preference layer. Every target keeps its own selected Reference photo, scene evidence and adaptive Recipe baseline. This allows a family/travel set to feel consistent without blindly copying a source group's absolute exposure or target-specific edits.

The backend validates the source and every target before writing and persists all selected target StyleProfiles in one transaction. Duplicate targets, the source group appearing as a target, an unknown group or a target without a Reference binding fails the whole operation before partial style changes are committed.

After a successful sync, normal canonical recomputation applies: adaptive Recipes are regenerated from each target group's own Reference/evidence plus the shared look; stale Recipe-review confirmations naturally return to attention; Lightroom preflight and previous session verification are recalculated through the existing state dependencies. Per-photo Recipe exceptions remain separate and continue to override only their own photos.


### Workflow cockpit and next action

The workstation should continuously summarize the current batch into one next-action focus without asking the photographer to inspect every workspace manually. The priority order is preparation failures/incomplete analysis, unresolved Cull attention, groups needing a usable Reference, Recipe review attention/pending evidence, then Lightroom delivery conflicts/missing XMP. Only when none remain is the batch shown as delivery-current.

The status is derived read-only from existing project truth. The workflow cockpit does not create a parallel task database and does not mark work complete just because a user visited a page. Counts must come from the same evidence, stores, Recipe fingerprints and XMP preflight used by the underlying views.

A `Continue workflow` action only navigates to the relevant workspace. Photographer-authoritative actions remain explicit: AI Cull suggestions are not silently accepted, References are not auto-selected, Recipe review confirmations are not auto-created and Lightroom XMP is never written automatically. This keeps the speed benefit of a guided batch editor while preserving Photo-Cake's non-destructive decision boundaries.
