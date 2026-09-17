# Photo-Cake

Local-first RAW photo workflow for high-volume batch correction, adaptive group color consistency and non-destructive finishing.

## Product workflow

```text
RAW
 -> import without changing source folders
 -> conservative moment grouping
 -> cached preview analysis
 -> portrait / non-portrait classification
 -> visual similarity regrouping
 -> automatic group target + adaptive per-photo color sync
 -> portrait-only retouch where applicable
 -> QA
 -> direct export for most photos
    -> optional RAW + XMP handoff to Lightroom for small manual refinements
    -> optional 16-bit TIFF handoff to Photoshop for pixel-level retouch
```

Photo-Cake never moves, renames, copies, overwrites or deletes source RAW files as part of normal import/processing. The user's month/trip/event folder structure remains authoritative.

## Local AI policy

Normal processing is offline-first. Online inference is disabled by default.

Release builds bundle pinned local models for face detection, person/skin segmentation and image embeddings. Analysis results are cached by source fingerprint, preview revision, task, model/version and config hash so the same local inference is reused across classification, grouping, masks and QA instead of being recomputed.

See:

- `docs/LOCAL_INFERENCE.md`
- `docs/MODEL_BUNDLE.md`
- `docs/PREVIEW_AND_SEMANTIC_GROUPING.md`
- `docs/ADAPTIVE_GROUP_SYNC.md`
- `docs/FINISHING_WORKFLOW.md`
- `docs/RAW_IMPORT_AND_GROUPING.md`

## Repository

```text
Photo-Cake/
├─ apps/
│  ├─ windows/              Windows Tauri host
│  └─ android/              Android Tauri host
├─ packages/
│  └─ ui/                   shared React UI
├─ crates/
│  └─ photo-core/           shared project/batch/analysis/edit contracts
├─ models/
│  └─ manifest.json         pinned bundled local model manifest
├─ scripts/
│  └─ fetch-models.mjs      verified release-time model fetcher
├─ docs/
└─ .github/workflows/
```

## Batch pipeline

```text
IMPORT
  -> ANALYZE
  -> APPLY_PRESET
  -> PORTRAIT_RETOUCH  (skipped for non-portrait/unclassified assets)
  -> QA
  -> EXPORT
  -> DONE
```

Each photo is an independent resumable job. A failed image does not stop the rest of the batch by default. Imported RAW assets retain their stable catalog IDs in the batch store.

## Development

Prerequisites:

- Node.js 22+
- pnpm 10.17.1
- Rust stable
- Tauri platform prerequisites

```bash
pnpm install
pnpm check
cargo test --workspace
```

Windows development:

```bash
pnpm dev:windows
```

Android development:

```bash
pnpm android:init
pnpm dev:android
```

Prepare pinned local model payloads manually when needed:

```bash
pnpm models:windows
pnpm models:android
```

Model payloads are not committed to Git. Release workflows fetch fixed versions, verify SHA256 and bundle them into the platform package.

## Git and releases

One `main` branch is shared by both products. Use short-lived `feature/*`, `fix/*`, and `platform/*` branches when needed. Do not maintain long-lived Windows/Android branches.

Platform versions may advance independently:

```text
windows-v0.1.0
android-v0.1.0
```

Windows and Android use separate release workflows while sharing `photo-core` and the common UI package.
