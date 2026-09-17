# Local inference and cache policy

Photo-Cake is offline-first. Normal ingest, classification, grouping, color analysis, portrait analysis, masks, QA and export must be able to run without an online AI service.

## Principles

- Online inference is disabled by default.
- Cloud inference may only be added as an explicit optional action; it must never be required for the normal batch pipeline.
- Prefer metadata and deterministic algorithms when a model is unnecessary.
- Prefer small reusable local vision models over large generative models.
- Store model ID/version and inference configuration with every cached result.
- Reuse an existing result when source fingerprint, preview revision, model ID/version and configuration are unchanged.
- Recompute only the affected task when one dependency changes.
- Source RAW files are never modified by inference or cache operations.

## Shared local artifacts

A single preview may produce reusable artifacts for several later stages:

1. Person/face detection
   - portrait classification
   - subject count
   - portrait routing
   - later face/skin mask scheduling

2. Image embedding
   - visual similarity grouping
   - duplicate/near-duplicate detection
   - group reference selection

3. Face embedding
   - same-person clustering
   - subject-aware grouping

4. Segmentation
   - person/skin/face/hair/sky/background masks
   - local color and portrait adjustments
   - QA overlays

These artifacts are cached and shared instead of being recomputed by each feature.

## Cache identity

The cache key includes:

- asset ID
- source fingerprint
- preview revision
- inference task
- model ID
- model version
- configuration hash

A change in model version does not invalidate unrelated tasks. A change to export settings does not invalidate analysis. A change to a portrait segmentation model does not invalidate image embeddings.

## Platform execution

Windows and Android use the same semantic artifact contracts.

Windows backends may include CPU, DirectML, CUDA or TensorRT.
Android backends may include CPU, NNAPI, QNN or Vulkan.

Backend choice changes performance only. It must not change the meaning of cached artifacts or batch/project behavior.

## Suggested model families

Keep the initial footprint small:

- one lightweight person/face detector/classifier
- one general image embedding model
- one segmentation model

Add face embedding separately only when same-person grouping is implemented.

## Resource reuse

Load a model once per worker/runtime and reuse it across a batch. Do not repeatedly initialize the same model for each image. Keep bounded concurrency so model instances, RAM/VRAM and thermal load remain predictable.

Preview analysis should run once and feed classification, grouping, reference selection and QA where possible.
