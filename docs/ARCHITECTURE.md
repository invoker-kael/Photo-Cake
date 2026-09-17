# Architecture

## Scope

Photo-Cake is intentionally a single-user local application. There is no application server, user account system, organization model, or cloud scheduler in the core design.

## Layers

```text
Responsive React UI
        |
     Tauri IPC
        |
Photo-Cake Desktop Host
        |
   photo-core (Rust)
        |
+-------+-----------+-----------+
|                   |           |
Photo Pipeline   AI Adapter   Storage Adapter
|                   |           |
RAW/Color        ONNX/TRT      SQLite/files
Render           Mobile AI     Cache/checkpoints
```

## Platform strategy

### Windows

Primary target. Local GPU acceleration will prefer NVIDIA CUDA/TensorRT when available, with ONNX Runtime/DirectML and CPU fallbacks considered later.

### High-end tablets

The React UI is touch-first and Tauri 2 keeps a path open for Android/iOS packaging. The domain layer does not depend on CUDA. Platform-specific AI backends will implement the same adapter interface.

## Non-destructive editing

Original files are immutable. A project stores:

- original file references
- edit parameters
- generated masks
- previews
- AI analysis cache
- job/checkpoint state
- export recipes

The final full-resolution image is rendered only for export or an explicit high-quality preview.

## First milestone

The first milestone is not image quality. It is a reliable automation substrate that can process a large directory without losing state when the app closes, crashes, sleeps, or encounters a bad photo.
