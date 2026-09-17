# Photo-Cake

Local-first AI photo workflow for Windows and high-end tablets, focused on non-destructive editing and reliable batch automation.

## Product direction

Photo-Cake is designed for a single user. The desktop/tablet UI stays lightweight while the photo pipeline handles large batches safely and resumably.

### Core principles

- Local-first: photos remain on the device by default.
- Single-user: no accounts, teams, cloud control plane, or multi-tenant services.
- Non-destructive: originals are never modified.
- Batch-first: import, analyze, edit, QA, and export are queueable jobs.
- Resumable: interrupted batches continue from checkpoints.
- Adaptive: one reference edit can be applied intelligently across a batch.
- Touch-friendly: the UI is designed for mouse/keyboard and large touch screens.
- Replaceable AI: models are isolated behind stable interfaces.

## Initial scope

### v0.1

- Project/library shell
- JPG/PNG import first; RAW pipeline interface reserved
- Responsive desktop/tablet UI
- Non-destructive edit model
- Batch queue and state machine
- Pause/resume/cancel/retry
- Per-photo checkpoints
- Preset application
- Automatic QA stage
- Export stage
- Local project persistence interface
- Tauri 2 + React + TypeScript shell
- Rust `photo-core` library for workflow logic

### Later

- RAW decoding and color management
- Face and skin masks
- Portrait retouching
- Reference-based batch consistency
- GPU inference via ONNX Runtime / TensorRT on Windows
- Tablet-specific inference backends
- AI culling and duplicate grouping

## Batch pipeline

```text
IMPORT
  -> ANALYZE
  -> APPLY_PRESET
  -> RETOUCH
  -> QA
  -> EXPORT
  -> DONE

Any stage can become:
  PAUSED / FAILED / CANCELLED

FAILED jobs can be retried from the last completed checkpoint.
```

A batch is intentionally represented as many independent photo jobs instead of one monolithic task. One bad image must not stop the rest of the batch.

## Repository layout

```text
Photo-Cake/
├─ apps/
│  └─ desktop/            # Tauri + React application (Windows first, mobile-ready)
├─ crates/
│  └─ photo-core/         # Core project/batch/domain logic
├─ docs/
│  ├─ ARCHITECTURE.md
│  └─ BATCH_AUTOMATION.md
├─ .github/workflows/
└─ package.json
```

## Development

Requirements:

- Node.js 22+
- pnpm 10+
- Rust stable
- Windows: WebView2 and Tauri build prerequisites

```bash
pnpm install
pnpm dev
```

Rust core only:

```bash
cargo test --manifest-path crates/photo-core/Cargo.toml
```

## Status

Foundation stage. The first implementation target is a working batch queue with persistence before image-processing models are added.
