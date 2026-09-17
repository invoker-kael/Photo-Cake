# Architecture

## Scope

Photo-Cake is intentionally a single-user local application. There is no account system, organization model, multi-tenant service, or mandatory cloud scheduler.

## Layers

```text
              packages/ui
          Responsive React UI
                   |
          +--------+--------+
          |                 |
   apps/windows        apps/android
    Tauri host          Tauri host
          |                 |
          +--------+--------+
                   |
            crates/photo-core
        batch / project / QA
                   |
        +----------+----------+
        |                     |
 platform storage      inference adapter
        |                     |
 Windows filesystem     Windows GPU backend
 Android SAF            Android NPU/GPU backend
```

## Platform contract

Windows and Android are separate deliverables, not separate codebases. Shared batch state, edit semantics, QA rules, project schema, and responsive UI belong in shared packages/crates.

Platform shells own only platform behavior such as file picking, drag/drop, touch/stylus integration, packaging, thermal/resource policy, and hardware acceleration bindings.

## Windows

Primary development target. Future acceleration should prefer NVIDIA CUDA/TensorRT where useful, with ONNX Runtime/DirectML and CPU fallbacks as appropriate.

## Android

High-end tablet target. The Android shell uses the same UI and domain contracts, while mobile-specific adapters handle Storage Access Framework, lifecycle/suspend-resume, thermal limits, and NNAPI/QNN/Vulkan-class inference backends.

## Non-destructive editing

Original files are immutable. A project stores original references, edit parameters, masks, previews, AI analysis cache, job/checkpoint state, and export recipes. Platform-specific caches may be regenerated.

## First milestone

The first milestone is a reliable automation substrate that can process large batches without losing state when the app closes, crashes, sleeps, or encounters a bad photo.
