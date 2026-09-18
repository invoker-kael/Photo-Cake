mod executor;
mod exposure;
mod litert_runtime;
mod local_models;
mod raw_preview;
mod quality;

pub use executor::{AnalyzeError, LocalAnalyzeExecutor};
pub use exposure::analyze_preview_exposure;
pub use local_models::{
    LoadedModelIdentity, LocalModelError, LocalSemanticModels, SegmentationSummary,
};
pub use raw_preview::{
    ExtractedPreviewInfo, RawPreviewError, extract_largest_embedded_jpeg, source_fingerprint,
};
pub use quality::score_image_quality;

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageBuffer, Rgb};
    use photo_core::ModelPlatform;

    #[test]
    #[ignore = "requires pinned runtime models downloaded by scripts/fetch-models.mjs"]
    fn bundled_models_smoke() {
        let model_dir = std::env::var("PHOTO_CAKE_MODEL_DIR")
            .expect("PHOTO_CAKE_MODEL_DIR must point to models/runtime/<platform>");
        let mut pixels = ImageBuffer::new(320, 240);
        for (x, y, pixel) in pixels.enumerate_pixels_mut() {
            *pixel = Rgb([
                ((x * 255) / 319) as u8,
                ((y * 255) / 239) as u8,
                128,
            ]);
        }
        let image = DynamicImage::ImageRgb8(pixels);
        let models = LocalSemanticModels::load(model_dir, ModelPlatform::Windows).unwrap();
        let segmentation = models.segment(&image).unwrap();
        assert!(segmentation.foreground_ratio.is_finite());
        let embedding = models.embed(&image).unwrap();
        assert!(embedding.len() >= 128);
        assert!(embedding.iter().all(|value| value.is_finite()));
        let norm = embedding.iter().map(|value| value * value).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-3);
    }
}
