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

1. immediately create conservative moment groups from capture time, camera and filename sequence;
2. refine only inside those groups using local classification and visual embeddings.

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

Multiple reference sets may coexist for different looks/scenes. The photographer's selected reference must persist independently from AI suggestions and survive reopening/re-importing the project.

Selecting a reference does not itself imply that edits are applied. Photo-Cake must have reliable measured color/exposure evidence before producing adaptive white-balance adjustments; it must not fabricate a color temperature from a rendered preview merely to populate XMP.

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

Every applied target photo has its own Recipe with reference lineage.

## Lightroom Workflow

Primary handoff:

```text
IMG_0001.CR3
IMG_0001.xmp
```

The XMP must contain only mapped edits, remain small, and be traceable to the target Recipe/asset. Lightroom/Camera Raw should be able to continue from those edits.

No early requirement for Lightroom catalog modification, database writing or a Lightroom plugin.

## Direct Export

The same Recipe can feed the existing renderer/export pipeline for requested JPEG/TIFF output. Direct export is a supported product path, but it must not become a separate editing model.

## Platforms

Windows is the primary workstation for large RAW collections, batch processing, GPU acceleration and Lightroom handoff.

Android is a companion for selection, preview, reference management and lightweight processing. Both platforms reuse shared core workflow rules and data semantics.

## Success Criteria

For a large personal shoot, the user can:

1. point Photo-Cake at existing RAWs without copying them;
2. quickly see previews and useful groups;
3. reduce manual review with culling suggestions;
4. select a preferred look/reference;
5. have Photo-Cake adapt it across similar photos;
6. review exceptions rather than every repetitive adjustment;
7. create tiny XMP sidecars for Lightroom or explicitly export final images.


## Workstation Entry

On Windows, the user can choose an existing RAW folder with the native folder picker. Photo-Cake registers the RAWs in place, creates initial moment groups and runs local analysis. Finishing the preparation queue means the photos are ready for culling/group/reference work; it does not imply that Photo-Cake silently exported or destructively edited them.


## Culling Evidence Behavior

Technical culling starts from locally measured preview evidence: sharpness/blur and severe exposure quality. Existing embeddings are reused for near-duplicate/burst ranking inside the same photo group. Expression, composition and other semantic quality dimensions are only added when dedicated evidence exists; Photo-Cake must not fabricate those scores. The workstation shows pending items while analysis is incomplete.


## Photographer Culling Decisions

AI culling remains advisory. The workstation separates the suggested decision from the photographer's saved decision. The photographer can mark Keep, Review or Reject, clear that override to return to the suggestion, and re-import the same RAW without losing the decision because it is stored against the stable asset ID. Reject is workflow state only; it never deletes the RAW.
