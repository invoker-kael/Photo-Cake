use crate::{InferenceTask, OnlineInferencePolicy};
use serde::{Deserialize, Serialize};

const BUNDLED_MODEL_MANIFEST: &str = include_str!("../../../models/manifest.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ModelPlatform {
    Shared,
    Windows,
    Android,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelVariant {
    pub platform: ModelPlatform,
    pub format: String,
    pub file: String,
    pub source_url: String,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundledModelSpec {
    pub id: String,
    pub version: String,
    pub task: InferenceTask,
    pub license: String,
    pub homepage: String,
    pub labels: Vec<String>,
    pub variants: Vec<ModelVariant>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelBundleManifest {
    pub schema_version: u32,
    pub online_inference_policy: OnlineInferencePolicy,
    pub models: Vec<BundledModelSpec>,
}

impl ModelBundleManifest {
    pub fn bundled() -> Result<Self, serde_json::Error> {
        serde_json::from_str(BUNDLED_MODEL_MANIFEST)
    }

    pub fn model_for_task(&self, task: InferenceTask) -> Option<&BundledModelSpec> {
        self.models.iter().find(|model| model.task == task)
    }

    pub fn variant_for(
        &self,
        task: InferenceTask,
        platform: ModelPlatform,
    ) -> Option<(&BundledModelSpec, &ModelVariant)> {
        let model = self.model_for_task(task)?;
        let variant = model
            .variants
            .iter()
            .find(|variant| variant.platform == platform)
            .or_else(|| {
                model
                    .variants
                    .iter()
                    .find(|variant| variant.platform == ModelPlatform::Shared)
            })?;
        Some((model, variant))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_manifest_is_offline_first_and_has_core_models() {
        let manifest = ModelBundleManifest::bundled().unwrap();
        assert_eq!(manifest.online_inference_policy, OnlineInferencePolicy::Disabled);
        assert!(manifest
            .variant_for(InferenceTask::FaceDetection, ModelPlatform::Windows)
            .is_some());
        assert!(manifest
            .variant_for(InferenceTask::Segmentation, ModelPlatform::Android)
            .is_some());
        assert!(manifest
            .variant_for(InferenceTask::ImageEmbedding, ModelPlatform::Windows)
            .is_some());
        assert!(manifest
            .variant_for(InferenceTask::ImageEmbedding, ModelPlatform::Android)
            .is_some());
    }

    #[test]
    fn platform_specific_embedding_variants_are_distinct() {
        let manifest = ModelBundleManifest::bundled().unwrap();
        let (_, windows) = manifest
            .variant_for(InferenceTask::ImageEmbedding, ModelPlatform::Windows)
            .unwrap();
        let (_, android) = manifest
            .variant_for(InferenceTask::ImageEmbedding, ModelPlatform::Android)
            .unwrap();
        assert_ne!(windows.file, android.file);
        assert_eq!(windows.format, "ONNX_FP16");
        assert_eq!(android.format, "ONNX_INT8");
    }
}
