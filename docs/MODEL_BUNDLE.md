# Bundled local models

Photo-Cake releases bundle pinned local vision models. Normal runtime does not need to download a model or call an online AI service.

The authoritative machine-readable list is `models/manifest.json`. Every model variant has a fixed source URL, version, expected size and SHA256. Release jobs download to a temporary runtime directory, verify the payload, and only then bundle it into the application.

Downloaded model payloads are intentionally excluded from Git history.

## Initial bundle

### MediaPipe BlazeFace short-range

Purpose: lightweight face detection and face count/location signals used by portrait classification and later portrait processing.

Deployment: shared TFLite payload for Windows and Android.

License policy: Apache-2.0 model ecosystem/source selection.

### MediaPipe Selfie Multiclass 256

Purpose: on-device semantic segmentation for person-related regions. The model provides categories covering background, hair, body skin, face skin, clothes and other regions, allowing several later stages to reuse one segmentation result.

Deployment: shared TFLite payload for Windows and Android.

### DINOv2 Small

Purpose: general image embeddings for visual similarity grouping, near-duplicate analysis and automatic group-reference selection.

Deployment:

- Windows: ONNX FP16
- Android: ONNX INT8

The platform variants represent the same semantic task; only deployment precision/performance differs.

## Runtime policy

- Online inference is disabled by default.
- A model is loaded once per worker/runtime and reused across a batch.
- Inference output is cached by source fingerprint, preview revision, task, model ID/version and config hash.
- Changing one model invalidates only artifacts produced by that model/task.
- Export recipe changes never invalidate classification, embeddings or segmentation.
- Source RAW files are never modified by model execution.

## Excluded from the default bundle

Photo-Cake does not bundle model families whose default pretrained-weight licensing creates avoidable redistribution/commercial-use ambiguity. The initial bundle therefore avoids InsightFace pretrained weights and Ultralytics YOLO packages.

## Release packaging

Use:

```text
node scripts/fetch-models.mjs windows
node scripts/fetch-models.mjs android
```

The Windows and Android Release workflows call the appropriate command automatically. Tauri release-only configuration overlays package `models/manifest.json` and the selected platform runtime payloads into the application bundle. Ordinary CI does not repeatedly download the model weights.
