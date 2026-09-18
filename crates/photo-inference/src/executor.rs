use crate::{LocalModelError, LocalSemanticModels, RawPreviewError, extract_largest_embedded_jpeg, score_image_quality, source_fingerprint};
use image::DynamicImage;
use photo_core::{
    AnalysisArtifact, AnalysisCache, AnalysisCacheError, AnalysisCacheKey, BatchItem, BatchStage,
    ClassificationSignals, ClassificationStore, ClassificationStoreError, InferenceTask,
    ModelPlatform, PhotoClassification, PortraitClassificationPolicy, PreviewArtifact, PreviewSource,
    PreviewStore, PreviewStoreError, SceneTag, StageExecutor, classify_photo, preview_cache_path,
};
use std::path::{Path, PathBuf};
use thiserror::Error;

const PREVIEW_REVISION: &str = "embedded-jpeg-v1";
const SEGMENT_CONFIG_HASH: &str = "selfie-multiclass-summary-v1";
const EMBEDDING_CONFIG_HASH: &str = "dino-center-crop-imagenet-v1";
const CLASSIFIER_POLICY_VERSION: &str = "portrait-policy-v1";
const QUALITY_MODEL_ID: &str = "deterministic-preview-quality";
const QUALITY_MODEL_VERSION: &str = "1";
const QUALITY_CONFIG_HASH: &str = "laplacian-clipping-v1";

#[derive(Debug, Error)]
pub enum AnalyzeError {
    #[error("ANALYZE requires a catalog asset id")]
    MissingAssetId,
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Preview(#[from] RawPreviewError),
    #[error(transparent)]
    PreviewStore(#[from] PreviewStoreError),
    #[error(transparent)]
    AnalysisCache(#[from] AnalysisCacheError),
    #[error(transparent)]
    ClassificationStore(#[from] ClassificationStoreError),
    #[error(transparent)]
    Model(#[from] LocalModelError),
    #[error("image decode error: {0}")]
    Image(#[from] image::ImageError),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

pub struct LocalAnalyzeExecutor {
    cache_root: PathBuf,
    model_root: PathBuf,
    platform: ModelPlatform,
    preview_store: PreviewStore,
    analysis_cache: AnalysisCache,
    classifications: ClassificationStore,
    models: Option<LocalSemanticModels>,
}

impl LocalAnalyzeExecutor {
    pub fn new(
        project_db: impl AsRef<Path>,
        cache_root: impl AsRef<Path>,
        model_root: impl AsRef<Path>,
        platform: ModelPlatform,
    ) -> Result<Self, AnalyzeError> {
        std::fs::create_dir_all(cache_root.as_ref())?;
        Ok(Self {
            cache_root: cache_root.as_ref().to_path_buf(),
            model_root: model_root.as_ref().to_path_buf(),
            platform,
            preview_store: PreviewStore::open(project_db.as_ref())?,
            analysis_cache: AnalysisCache::open(project_db.as_ref())?,
            classifications: ClassificationStore::open(project_db.as_ref())?,
            models: None,
        })
    }

    pub fn analyze(&mut self, item: &BatchItem) -> Result<PhotoClassification, AnalyzeError> {
        let asset_id = item.asset_id.ok_or(AnalyzeError::MissingAssetId)?;
        let raw_path = Path::new(&item.source_path);
        let fingerprint = source_fingerprint(raw_path)?;
        let preview = self.ensure_preview(asset_id, raw_path, &fingerprint)?;
        let image = image::open(&preview.cache_path)?;

        let quality_key = AnalysisCacheKey {
            asset_id,
            source_fingerprint: fingerprint.clone(),
            preview_revision: preview.revision.clone(),
            task: InferenceTask::QualityScoring,
            model_id: QUALITY_MODEL_ID.to_string(),
            model_version: QUALITY_MODEL_VERSION.to_string(),
            config_hash: QUALITY_CONFIG_HASH.to_string(),
        };
        if self.analysis_cache.get(&quality_key)?.is_none() {
            let quality = score_image_quality(&image);
            self.analysis_cache.put(&AnalysisArtifact {
                key: quality_key,
                payload_json: serde_json::to_value(&quality)?,
            })?;
        }

        let (segmentation_id, embedding_id) = {
            let models = self.ensure_models()?;
            (
                models
                    .identity(InferenceTask::Segmentation)
                    .expect("segmentation identity is loaded")
                    .clone(),
                models
                    .identity(InferenceTask::ImageEmbedding)
                    .expect("embedding identity is loaded")
                    .clone(),
            )
        };

        let segmentation_key = AnalysisCacheKey {
            asset_id,
            source_fingerprint: fingerprint.clone(),
            preview_revision: preview.revision.clone(),
            task: InferenceTask::Segmentation,
            model_id: segmentation_id.id.clone(),
            model_version: segmentation_id.version.clone(),
            config_hash: SEGMENT_CONFIG_HASH.to_string(),
        };

        let signals = if let Some(cached) = self.analysis_cache.get(&segmentation_key)? {
            serde_json::from_value::<ClassificationSignals>(cached.payload_json)?
        } else {
            let summary = self.ensure_models()?.segment(&image)?;
            let signals = ClassificationSignals {
                asset_id,
                detected_person_count: summary.person_count,
                detected_face_count: summary.face_count,
                primary_subject_ratio: summary.primary_subject_ratio,
                people_confidence: summary.people_confidence,
                scene_tags: infer_scene_tags(&image),
            };
            self.analysis_cache.put(&AnalysisArtifact {
                key: segmentation_key,
                payload_json: serde_json::to_value(&signals)?,
            })?;
            signals
        };

        let classification = classify_photo(
            signals,
            format!("{}+{}", segmentation_id.id, CLASSIFIER_POLICY_VERSION),
            segmentation_id.version,
            PortraitClassificationPolicy::default(),
        );
        self.classifications.save(&classification)?;

        let embedding_key = AnalysisCacheKey {
            asset_id,
            source_fingerprint: fingerprint,
            preview_revision: preview.revision,
            task: InferenceTask::ImageEmbedding,
            model_id: embedding_id.id,
            model_version: embedding_id.version,
            config_hash: EMBEDDING_CONFIG_HASH.to_string(),
        };
        if self.analysis_cache.get(&embedding_key)?.is_none() {
            let embedding = self.ensure_models()?.embed(&image)?;
            self.analysis_cache.put(&AnalysisArtifact {
                key: embedding_key,
                payload_json: serde_json::json!({ "embedding": embedding }),
            })?;
        }

        Ok(classification)
    }

    fn ensure_preview(
        &self,
        asset_id: uuid::Uuid,
        raw_path: &Path,
        fingerprint: &str,
    ) -> Result<PreviewArtifact, AnalyzeError> {
        if self
            .preview_store
            .is_current(asset_id, fingerprint, PREVIEW_REVISION)?
        {
            if let Some(existing) = self.preview_store.get(asset_id)? {
                if Path::new(&existing.cache_path).is_file() {
                    return Ok(existing);
                }
            }
        }

        let cache_path = preview_cache_path(&self.cache_root, asset_id, PREVIEW_REVISION);
        let info = extract_largest_embedded_jpeg(raw_path, &cache_path)?;
        let artifact = PreviewArtifact {
            asset_id,
            source_fingerprint: fingerprint.to_string(),
            revision: PREVIEW_REVISION.to_string(),
            cache_path: cache_path.to_string_lossy().into_owned(),
            mime_type: "image/jpeg".to_string(),
            width: info.width,
            height: info.height,
            source: PreviewSource::EmbeddedRawPreview,
        };
        self.preview_store.save(&artifact)?;
        Ok(artifact)
    }

    fn ensure_models(&mut self) -> Result<&LocalSemanticModels, AnalyzeError> {
        if self.models.is_none() {
            self.models = Some(LocalSemanticModels::load(&self.model_root, self.platform)?);
        }
        Ok(self.models.as_ref().expect("models initialized"))
    }
}

impl StageExecutor for LocalAnalyzeExecutor {
    fn execute(&mut self, item: &BatchItem) -> Result<(), String> {
        if item.stage != BatchStage::Analyze {
            return Ok(());
        }
        self.analyze(item).map(|_| ()).map_err(|error| error.to_string())
    }
}

fn infer_scene_tags(image: &DynamicImage) -> Vec<SceneTag> {
    let width = image.width();
    let height = image.height();
    if width == 0 || height == 0 {
        return vec![SceneTag::Other];
    }

    let rgb = image.thumbnail(64, 64).to_rgb8();
    let mut luminance = 0.0f32;
    for pixel in rgb.pixels() {
        luminance += (0.2126 * f32::from(pixel[0])
            + 0.7152 * f32::from(pixel[1])
            + 0.0722 * f32::from(pixel[2]))
            / 255.0;
    }
    let average = luminance / (rgb.width() * rgb.height()).max(1) as f32;
    if average < 0.18 {
        vec![SceneTag::Night]
    } else if width >= height.saturating_mul(3) / 2 {
        vec![SceneTag::Landscape]
    } else {
        vec![SceneTag::Other]
    }
}
