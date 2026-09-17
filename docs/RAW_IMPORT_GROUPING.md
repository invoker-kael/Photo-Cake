# RAW Import and Initial Grouping

Photo-Cake imports RAW files only. JPEG/PNG/TIFF/HEIC are not primary import assets.

## Import pipeline

```text
SELECT FILES / FOLDER
    -> RAW FILTER
    -> METADATA PROBE
    -> INITIAL GROUPING
    -> PERSIST ASSETS/GROUPS
    -> CREATE BATCH
    -> THUMBNAIL/PREVIEW ANALYSIS
    -> VISUAL REGROUPING (later)
```

## Supported RAW intake extensions

Initial intake recognizes common camera RAW containers:

- CR3 / CR2
- NEF / NRW
- ARW / SR2 / SRF
- RAF
- ORF
- RW2
- PEF
- DNG
- 3FR
- IIQ

Recognizing a file as RAW does not imply that full decode support is already implemented for every camera model. Decode capability will be provided by platform photo-engine backends later.

## Grouping strategy

Grouping is deliberately two-stage.

### Stage A: import-time grouping

Cheap and immediate. It uses available metadata only:

1. camera identity when available
2. capture timestamp, otherwise filesystem timestamp fallback
3. filename sequence number when available
4. a short time-gap threshold

This produces `MOMENT` groups quickly without running AI.

Default heuristic:

- same camera identity, when known
- capture/file time gap <= 4 seconds
- and either sequential filenames or gap <= 1.5 seconds

The heuristic is conservative: uncertain images start a new group rather than being forced into a wrong group.

### Stage B: visual regrouping

Runs after thumbnails/previews exist and may split/merge initial groups using:

- perceptual/embedding similarity
- face/person overlap
- composition similarity
- duplicate/near-duplicate detection

This stage will be added with the image-analysis pipeline. It must preserve manual grouping overrides.

## Why group before retouching

A group becomes the unit for:

- comparing burst shots
- choosing a reference image
- syncing color/retouch intent
- person profile application
- QA consistency checks
- selecting the best image from similar shots

Batch jobs remain per-photo. Grouping organizes photos; it does not merge job state.
