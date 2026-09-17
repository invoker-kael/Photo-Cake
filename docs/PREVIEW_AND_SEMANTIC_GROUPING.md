# Preview analysis and semantic regrouping

Photo-Cake keeps the user's source folders untouched and performs all visual analysis from cached previews.

## Pipeline

1. RAW import records a stable asset ID and source reference.
2. Initial MOMENT grouping uses import-batch metadata only: camera source, capture/file time and filename sequence.
3. A cached preview artifact is registered for each asset. Embedded RAW previews are preferred; rendered previews are a fallback.
4. Local inference runs once per unchanged preview/model revision and stores reusable artifacts in the analysis cache.
5. Portrait/non-portrait classification is resolved before second-stage grouping.
6. DINOv2 image embeddings refine each MOMENT group into visually similar subgroups.
7. Portrait and non-portrait images are never merged into the same semantic subgroup, even when their embeddings are numerically similar.
8. Each subgroup receives a reference candidate near the embedding centroid for later AUTO_GROUP color sync.

## Reuse

Face/person signals may feed portrait classification and portrait scheduling. Image embeddings feed similarity grouping and reference selection. Segmentation results can later feed masks, local color and QA. These artifacts must not be recomputed by each feature independently.

## Invalidation

A cached artifact is reusable only while these inputs are unchanged:

- source fingerprint
- preview revision
- task
- model ID/version
- inference config hash

Changing export settings does not invalidate preview analysis. Changing the segmentation model does not invalidate DINOv2 embeddings. Changing a group reference invalidates group color sync only.

## Source safety

Preview cache, embeddings, classifications, masks and group state live in Photo-Cake project/application storage. The source RAW directory is never reorganized or used as an internal cache location.
