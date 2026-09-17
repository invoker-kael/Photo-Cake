# Photo-Cake

Local-first AI photo workflow focused on reliable batch automation, non-destructive editing, and Windows/Android support.

## Product shape

Photo-Cake is a single-user application with two platform packages built from one `main` branch:

- **Photo-Cake Windows** — primary desktop build, future CUDA/TensorRT/DirectML acceleration.
- **Photo-Cake Android** — high-end tablet build, future NNAPI/QNN/Vulkan acceleration.

Shared behavior must stay in common packages/crates instead of being copied between platform branches.

## Repository layout

```text
Photo-Cake/
├─ apps/
│  ├─ windows/              # Tauri Windows shell
│  └─ android/              # Tauri Android shell
├─ packages/
│  └─ ui/                   # Shared responsive React UI
├─ crates/
│  └─ photo-core/           # Shared batch/domain logic
├─ docs/
│  ├─ ARCHITECTURE.md
│  ├─ BATCH_AUTOMATION.md
│  └─ PLATFORMS.md
└─ .github/workflows/
```

## Batch pipeline

```text
IMPORT
  -> ANALYZE
  -> APPLY_PRESET
  -> PORTRAIT_RETOUCH
  -> QA
  -> EXPORT
  -> DONE
```

For RAW imports the IMPORT stage is already complete at ingest time, so the resumable batch starts at ANALYZE. ANALYZE is designed to consume cached previews and reusable local model artifacts; PORTRAIT_RETOUCH is skipped for non-portrait photos.

The primary finishing route is direct Photo-Cake export. Lightroom handoff is XMP-first for compatible non-destructive edits, while pixel-changing edits fall back to rendered TIFF/Photoshop handoff.

Each photo is an independent resumable job. A failed image does not stop the rest of the batch by default.

## Development

Requirements:

- Node.js 22+
- pnpm 10+
- Rust stable
- Windows build prerequisites for Tauri
- Android Studio/SDK/NDK for Android packaging

Install dependencies:

```bash
pnpm install
```

Windows:

```bash
pnpm dev:windows
pnpm build:windows
```

Android first-time initialization:

```bash
pnpm android:init
```

Then:

```bash
pnpm dev:android
pnpm build:android
```

Checks:

```bash
pnpm check
cargo test --workspace
```

## Git and releases

Use `main` plus short-lived `feature/*`, `fix/*`, and `platform/*` branches. Do not maintain long-lived Windows/Android branches.

Release tags are independent:

```text
windows-v0.1.0
android-v0.1.0
```

See `docs/PLATFORMS.md` for the platform contract.
