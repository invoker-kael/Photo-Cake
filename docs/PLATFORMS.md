# Platform strategy

Photo-Cake ships as two products from one main branch.

## Release channels

- Windows: `windows-vX.Y.Z`
- Android: `android-vX.Y.Z`

The version numbers may advance independently. Long-lived `windows` and `android` branches are intentionally avoided.

## Shared code

- `crates/photo-core`: batch state, project/edit contracts, QA/checkpoint semantics.
- `packages/ui`: reusable responsive React UI.

## Platform shells

- `apps/windows`: Tauri Windows host, desktop integration, future CUDA/TensorRT/DirectML backend.
- `apps/android`: Tauri Android host, touch/storage integration, future NNAPI/QNN/Vulkan backend.

Platform code may choose different scheduling/concurrency limits, but it must not change the semantic result of a batch recipe.

## Branching

Use `main` plus short-lived `feature/*`, `fix/*`, and `platform/*` branches. Merge shared behavior back to `main` before releasing either platform.
