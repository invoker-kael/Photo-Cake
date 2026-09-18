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
