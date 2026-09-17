# RAW import and grouping

Photo-Cake uses a reference-only RAW ingest model. The user's existing folder structure remains the source of truth.

## Source safety

Importing a RAW file means registering its path and metadata in the Photo-Cake project database.

Photo-Cake must not during import or ordinary processing:

- move the RAW
- rename the RAW
- copy the RAW into a managed library
- modify the RAW bytes
- delete the RAW
- create XMP/ACR sidecars unless the user explicitly requests a Lightroom handoff

A directory such as:

```text
Photos/
  2026-01/
  2026-02-Croatia/
  2026-05-Europe/
```

stays exactly as the user maintains it.

## RAW-only ingest

The importer accepts common RAW containers such as CR2, CR3, NEF, NRW, ARW, SR2, SRF, RAF, ORF, RW2, PEF, DNG, 3FR and IIQ.

Rendered files such as JPEG, PNG, TIFF and HEIC are ignored by the ingest path.

Each source RAW receives a stable asset ID in the Photo-Cake catalog. Re-importing the same source reference preserves that asset ID.

Once a RAW is registered, its batch job starts at `ANALYZE`; the `IMPORT` stage is already complete.

## Level 1 grouping: immediate metadata grouping

The first grouping pass is intentionally cheap and conservative so large shoots can be organized immediately without waiting for AI inference.

It uses available information such as:

- camera identity
- capture time when available
- file modification time as a fallback
- trailing filename sequence number

Known different cameras are never merged into the same initial group.

The default rule prefers splitting too often over incorrectly merging unrelated moments.

Groups are scoped to the current import batch/collection. Separate trips, months or events are never globally regrouped together just because their filenames or timestamps happen to look similar.

## Level 2 grouping: visual regrouping

After thumbnails/previews exist, Photo-Cake may refine the initial groups using image content.

Planned signals include:

- perceptual/embedding similarity
- burst/near-duplicate detection
- scene similarity
- face/person identity
- pose/expression similarity

This second pass may create `SIMILAR` subgroups inside a metadata `MOMENT` group.

It must not alter source folders or batch job state. Grouping is an organizational layer only.

## Manual grouping

Manual grouping/locking will override automatic regrouping. Once a user intentionally fixes a group, later automatic analysis must not silently replace that organization.

## Relationship to the editing pipeline

```text
RAW path reference
  -> register stable asset
  -> metadata MOMENT grouping
  -> preview generation
  -> visual/person regrouping
  -> ANALYZE
  -> batch correction / retouch / QA
  -> direct export OR Lightroom/Photoshop handoff
```
