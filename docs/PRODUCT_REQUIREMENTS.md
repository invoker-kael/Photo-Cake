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


### Manual group correction

Semantic grouping is advisory and must be cheap to correct on real travel/family shoots. Before Reference selection, Groups exposes compact previews and explicit correction actions instead of requiring threshold tuning:

- `Keep original moment together` collapses semantic children back to their Moment parent and locks it;
- `Allow semantic refine again` reopens an automatically locked Moment;
- adjacent non-semantic groups can be selected and merged;
- a non-semantic group can be split before any photo except its first.

Merge and split are transactional catalog operations and produce manual-locked groups with an explicit `MANUAL` grouping basis. Invalid/non-adjacent selections fail without partial catalog changes. Automatic refinement never rewrites a manual correction.

Once any Reference is selected, all merge/split/regroup actions are disabled in both UI and backend. Grouping changes must happen before the Reference → Recipe → Review → Lightroom lineage is established.


### Batch Reference setup

For a large shoot, Reference should not require opening every group just to accept an obvious starting candidate. The workstation therefore exposes a batch setup surface for groups that do not yet have a Reference.

Eligibility is intentionally stricter than the ordinary per-group candidate list. A batch candidate must belong to the group, must not be photographer-Rejected, and must either have an explicit photographer Keep/Review decision or completed Cull evidence whose suggestion is not RejectSuggestion. Evidence-pending photos and unresolved AI Reject suggestions stay individual-review work.

The photographer selects the target groups explicitly. `Select eligible` is only a convenience for that visible eligible set; the actual write still occurs only after the explicit `Set suggested References` action. Groups with an existing Reference are excluded and are never silently replaced.

The backend revalidates every selected group/candidate pair and then creates all missing ReferenceSets/bindings in one transaction. Any stale group, duplicate group, invalid membership, existing binding or newly-invalid Cull state aborts the whole batch without partial Reference creation. After the batch, each group remains fully editable through the normal individual candidate surface.

### Clear-group Recipe confirmation

Large travel, family and burst shoots should not require a full-card pass over every ordinary adaptive Recipe after the photographer has already established the group look. Recipe Review therefore keeps exception-first Triage as the inspection surface and adds an explicit clear-group confirmation path for groups whose remaining Recipes have no current attention signal.

A group is clear only when it has a selected Reference, resolved target-bound Recipes, at least one current unconfirmed deliverable Recipe, and every such Recipe has completed culling evidence or an explicit photographer decision. Saved per-photo exceptions, photographer Review decisions, unresolved AI Review/RejectSuggestion evidence, or pending/missing culling evidence keep the group out of clear-group confirmation. Photographer Keep remains authoritative over an AI suggestion; photographer Reject photos remain excluded from the deliverable Recipe set.

The UI may offer both one-group and all-clear-groups actions, but the backend always re-resolves current Recipes and revalidates the complete selected group set. All selected groups must still be clear before any new review fingerprint is written. Their current Recipe fingerprints are then confirmed in one transaction, so a stale group cannot produce a partial batch confirmation.

This is an explicit photographer acceptance shortcut, not automatic quality approval. Reference/style/Recipe-exception changes continue to invalidate affected confirmations through the existing fingerprint contract and return those photos to Review when appropriate. The workflow follows the efficient standard-photo pattern used by mature batch photo editors: establish the look, synchronize adaptively, inspect exceptions, explicitly accept the clear remainder, then hand off to Lightroom.

### Safe Lightroom delivery cohorts

Large shoots must not let one exceptional group stall unrelated delivery work. The Lightroom workspace therefore separates batch-safe groups from Needs action groups. A group is batch-safe only when its Reference/Recipe state resolves, the XMP preflight succeeds, no conflicting sidecar exists, at least one XMP is missing, and Recipe attention is clear. Groups with Review attention remain individually writable by explicit photographer action, preserving the existing advisory review policy without silently mixing them into the fast batch path.

The photographer explicitly selects the safe groups to hand off. `Select ready` selects only the current review-clear, conflict-free cohort; individual group checkboxes can narrow that set further. The backend still re-resolves and re-preflights every selected group before the first write, and the existing cross-group rollback remains authoritative if a race or filesystem failure happens after UI preflight.

XMP preflight is resilient per group. One unreadable or otherwise failing group records its own preflight error and remains in Needs action while successful groups keep their current/missing/conflict state. A manual `Refresh XMP checks` action lets the photographer re-read filesystem state after resolving an external Lightroom conflict without leaving and reopening the workspace.

Preflight now reports missing sidecars explicitly instead of inferring them only from total-current-conflict arithmetic. Core XMP target classification defines current, missing and conflict states once and is reused by the Windows bridge, keeping UI delivery counts tied to the same non-destructive sidecar truth used by write-time validation.

This mirrors the useful batch-production principle of processing normal, homogeneous work together while isolating exceptional cases for focused review. Photo-Cake still does not overwrite existing conflicting XMP and does not auto-deliver attention groups.

### Selective per-photo exception synchronization

Repeated exceptions inside one honest photography group should not require re-entering the same small correction photo by photo. Recipe Review can therefore use a saved per-photo exception as a source and explicitly synchronize selected exception fields to selected peer photos in the same group.

Synchronization is deliberately narrower than copying a final Recipe. Only the existing per-photo delta layer can move, currently Exposure, Contrast and Saturation. The target photo keeps its own adaptive Recipe baseline, Reference lineage, group StyleProfile, measured evidence and any unselected exception fields. This preserves Photo-Cake's adaptive model while still making burst sequences, repeated backlight frames and similar family/travel shots efficient to correct.

The photographer chooses the source photo, chooses which delta fields may synchronize, and chooses target photos. While synchronization is active the group temporarily exposes all deliverable Recipes so clear peers can be selected without abandoning exception-first Triage. A convenience action may select all deliverable peers, but the final write remains explicit.

The workstation re-resolves the current editable group before any write. The source must still have a persisted exception, every target must still be a non-Reject target-bound Recipe in the same group, duplicate targets and source-as-target are rejected, and all changed target overrides commit in one SQLite transaction. Existing target exception fields that were not selected are preserved.

Any changed target Recipe loses its current review state through the existing fingerprint contract and returns to Review. Edited previews are invalidated and Lightroom delivery is recalculated from the changed canonical Recipes. This makes synchronization a fast way to create a small exception cohort, not a shortcut around quality review.

The interaction borrows the useful standard-photo/selective-sync pattern of mature batch editors, but does not introduce a preset-copy engine or allow exception deltas to cross group boundaries.


### Exposure-bracket workflow

AEB/HDR source frames must not be treated as expendable burst duplicates. After Analyze has produced exposure and embedding evidence, the workstation conservatively detects same-composition symmetric exposure ladders and surfaces them as bracket sets in Cull/Groups.

Detected bracket members remain photographer-reviewable and are protected from ordinary duplicate-driven rejection. A full Moment that is exactly one bracket set remains intact through semantic refinement.

Bracket source RAWs are **not** ordinary Adaptive Recipe targets. Their intentional capture EV differences must remain unchanged until HDR merge, so automatic batch Reference setup and safe batch XMP delivery exclude groups that still contain bracket sources. The measured center exposure may still be shown as a useful manual reference candidate, but it must not cause -EV/+EV source frames to be normalized.

Lightroom preflight must identify the exact HDR source asset IDs. Pure bracket groups require no fake Reference before merge. Mixed groups may continue normal Recipe/XMP work only for non-bracket peers, and only through individual handoff while the HDR source set remains visible as Needs action.

After completing the merge in Lightroom/Camera Raw, the photographer must be able to explicitly mark that bracket merge complete. Completion must be fingerprint-bound to the currently detected bracket members/center so a changed stack automatically returns to Needs action. Marking a pure bracket source group complete must close its workflow action without creating placeholder XMP sidecars.

Photo-Cake does not claim to merge HDR RAWs yet. The current production path is to preserve the bracket, keep its intent visible, merge the source RAWs in Lightroom/Camera Raw, mark the source stack merged, then import/use the resulting HDR DNG when desired. HDR/DNG merging or Direct Export from brackets requires a real RAW-domain merge plus canonical Recipe/color/metadata renderer and must not be simulated with embedded-JPEG preview logic.


### Moment-level quick culling

Large travel, family and burst sessions must support an explicit Moment-level shortcut without weakening photographer control. The system may propose a Quick Cull plan only from the same cached group-relative evidence already used by ordinary Cull; it must not introduce a parallel hidden ranking model.

Eligibility requires a complete multi-photo group with a rank-1 Keep candidate, no pending quality analysis and no exposure-bracket source set. The primary candidate may be persisted as photographer Keep. If the group contains people/face evidence, every alternate must remain Review rather than being batch-Rejected. If people or scene evidence is incomplete, or scene tags disagree/are unknown, the same conservative Review-only treatment applies. Automatic Reject within this explicit action is allowed only when the group is confirmed non-people, scene evidence is complete and consistent, the lower-ranked frame has at least 0.985 embedding similarity to a stronger prior frame, and the primary has a material quality gap; all other alternates remain Review. This applies equally to landscape, architecture, food, night and other supported scene tags and must not treat “non-portrait” as permission for aggressive rejection.

The photographer must explicitly choose which eligible groups to apply. The backend must revalidate the complete selected set before any writes, must refuse groups with existing photographer Cull decisions or an established Reference, and must commit all resulting decisions transactionally. A stale/invalid selected group must abort the batch rather than partially quick-culling other groups.

Quick Cull is selection acceleration, not deletion. Source RAW files remain untouched, Review alternates remain available to Reference/Recipe workflows, and subsequent photographer decisions stay authoritative.

The selected Quick Cull batch must expose its projected Keep/Review/Reject counts before application. The resulting write must create a persistent operation record in the same transaction as the Cull decisions. The latest operation may be undone only while its exact group membership, decision values and write revisions remain unchanged and before any affected group establishes a Reference. If any photographer decision or downstream lineage has changed, Undo must fail rather than erase newer intent.


### Reference readiness preflight

- Batch Reference setup MUST use one backend-generated readiness plan derived from existing Cull evidence and saved photographer decisions; UI heuristics MUST NOT be an independent authority.
- Photographer Keep is authoritative and MAY be used as the suggested Reference even when sibling assets are still analysis-pending. AI-only candidates MUST wait until pending Cull evidence is cleared.
- Candidate precedence is photographer Keep, AI Keep, photographer Review, AI Review, with existing technical quality/group rank used only as tie-break evidence.
- Photographer Reject and unoverridden AI Reject suggestions MUST NOT become batch References.
- HDR bracket groups MUST remain routed to Lightroom/Camera Raw merge before batch Reference setup.
- The Reference UI MUST expose useful people and scene context, including landscape/architecture/food/night/document tags when available, without requiring a portrait-specific workflow.
- Batch apply MUST revalidate the current readiness plan and reject stale suggested asset IDs rather than silently choosing a replacement.


### Scene-aware batch look sync

- Batch look sync MUST continue to reuse the existing `StyleProfile` path and MUST NOT introduce a separate portrait preset, landscape preset, or copied numeric Recipe path.
- The workstation SHOULD default-select only style-sync targets with compatible existing Cull evidence.
- People/family source groups SHOULD recommend other people/family groups. Non-people groups SHOULD recommend targets sharing at least one existing scene tag.
- People↔scene mismatches, disjoint scenic tags, and incomplete evidence MUST be surfaced as Review rather than silently included in the recommended batch.
- Review targets MUST remain manually selectable so photographer intent remains authoritative.
- Photographer Rejects and unoverridden AI Reject suggestions MUST NOT define the group scene context used for look-sync recommendations.
- Every target MUST preserve its own Reference photo and adaptive baseline after StyleProfile sync, then continue through the normal Recipe exception-review path.


### Canonical exception-first Recipe Review

- Recipe Review MUST have one backend preflight that is authoritative for Triage visibility and clear-group batch confirmation.
- The preflight MUST classify editable Recipes as Confirmed, Needs Review, Clear, or Pending and MUST expose the reason for attention.
- Saved per-photo exceptions and photographer Review decisions MUST appear before AI Review/Reject suggestions in exception-first ordering.
- Missing Recipe/exposure evidence MUST be Pending and MUST never be batch-confirmable, even when the photo was explicitly kept during Cull.
- Clear-group batch confirmation MUST revalidate the current preflight immediately before writing confirmation fingerprints.
- Landscape, architecture, night, food and people/family context MAY be displayed in Review, but MUST NOT create a second Recipe engine or portrait-only workflow.
- Exposure-bracket source RAWs MUST remain outside ordinary adaptive Recipe confirmation. Mixed groups MAY continue reviewing non-HDR peers while HDR sources route separately to Lightroom/Camera Raw.


### Adaptive tonal correction

- Analyze MUST retain a robust relative exposure signal and SHOULD record luminance tail/midtone evidence sufficient to compare tonal distribution between a Reference and target photo.
- Reference-driven Recipe generation MUST resolve exposure per photo before deriving Highlights/Shadows corrections.
- Highlights/Shadows MUST be target-specific; Photo-Cake MUST NOT blindly copy the Reference photo's numeric tone sliders to every image.
- Tone corrections MUST be bounded and confidence-weighted because embedded JPEG previews have already passed through the camera rendering pipeline.
- Legacy analysis evidence without tone percentiles MUST fall back safely to exposure-only Recipe generation.
- Recipe Review MUST allow per-photo Highlights/Shadows exceptions and selective same-group exception synchronization alongside Exposure, Contrast and Saturation.
- Edited preview SHOULD visualize Highlights/Shadows direction without claiming RAW-engine parity.
- Lightroom XMP handoff MUST preserve the generated or manually adjusted Highlights/Shadows values.
- White balance MUST continue to remain unset when reliable RAW/metadata evidence is unavailable.


### Reference-relative contrast and saturation

- Adaptive Recipe generation SHOULD normalize per-photo tonal separation against the selected Reference after exposure alignment.
- Positive automatic Contrast correction MUST back off when the target already contains material highlight or shadow clipping.
- Analyze SHOULD record a relative preview colorfulness signal for within-shoot/reference matching; it MUST NOT present that signal as sensor-linear colorimetry.
- Adaptive Recipe generation MAY add a bounded per-photo Saturation correction on top of the shared StyleProfile preference.
- Missing legacy colorfulness evidence MUST preserve the prior saturation behavior rather than fabricate a value.
- Photographer Contrast/Saturation exceptions remain authoritative after automatic matching.


### Exposure robustness and preview fidelity

- The existing robust exposure estimate MUST remain the primary adaptive exposure signal.
- P50 MAY refine per-photo exposure only as a bounded secondary correction and MUST NOT erase the photographer's StyleProfile exposure bias.
- Positive P50-based exposure refinement MUST respect highlight headroom using the Reference and target upper luminance evidence.
- Automatic exposure refinement MUST remain conservative when evidence confidence is low.
- Edited preview MUST apply Exposure in linear-light rather than directly multiplying gamma-encoded sRGB values.
- Preview rendering remains an approximation and MUST NOT be presented as equivalent to Lightroom/Camera Raw demosaic, tone mapping or color management.


### Channel-aware clipping protection

- Highlight clipping evidence MUST detect single-channel RGB clipping, not only near-white luminance.
- Shadow clipping evidence SHOULD represent true multi-channel black clipping rather than dark saturated color.
- Automatic positive Exposure refinement MUST back off when the target has materially more channel clipping than its Reference.
- Adaptive Highlights SHOULD compensate for excess target clipping even when luminance percentiles alone look similar.
- Positive adaptive Contrast MUST remain conservative on clipped targets.
- Edited preview Highlights/Shadows SHOULD preserve colored-region channel relationships better than an equal RGB offset.
