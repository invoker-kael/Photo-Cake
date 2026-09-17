use image::{DynamicImage, imageops::FilterType};
use photo_core::{InferenceTask, ModelBundleManifest, ModelPlatform};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;
use tract_onnx::prelude::InferenceModelExt;
use tract_tflite::prelude::*;

const SEGMENT_SIZE: u32 = 256;
const EMBEDDING_SIZE: u32 = 224;
const DINO_HIDDEN: usize = 384;

#[derive(Debug, Error)]
pub enum LocalModelError {
    #[error("model manifest error: {0}")]
    Manifest(#[from] serde_json::Error),
    #[error("required model is missing from manifest for {0:?}")]
    MissingSpec(InferenceTask),
    #[error("bundled model file is missing: {0}")]
    MissingFile(PathBuf),
    #[error("model/inference error: {0}")]
    Inference(String),
    #[error("unexpected model output: {0}")]
    Output(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SegmentationSummary {
    pub person_count: u32,
    pub face_count: u32,
    pub primary_subject_ratio: f32,
    pub people_confidence: f32,
    pub foreground_ratio: f32,
    pub face_skin_ratio: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedModelIdentity {
    pub id: String,
    pub version: String,
    pub task: InferenceTask,
    pub file: PathBuf,
}

pub struct LocalSemanticModels {
    segmenter: Arc<TypedRunnableModel>,
    embedder: Arc<TypedRunnableModel>,
    segmentation_identity: LoadedModelIdentity,
    embedding_identity: LoadedModelIdentity,
}

impl LocalSemanticModels {
    pub fn load(
        model_root: impl AsRef<Path>,
        platform: ModelPlatform,
    ) -> Result<Self, LocalModelError> {
        let manifest = ModelBundleManifest::bundled()?;
        let (seg_spec, seg_variant) = manifest
            .variant_for(InferenceTask::Segmentation, platform)
            .ok_or(LocalModelError::MissingSpec(InferenceTask::Segmentation))?;
        let (emb_spec, emb_variant) = manifest
            .variant_for(InferenceTask::ImageEmbedding, platform)
            .ok_or(LocalModelError::MissingSpec(InferenceTask::ImageEmbedding))?;

        let segment_path = model_root.as_ref().join(&seg_variant.file);
        let embedding_path = model_root.as_ref().join(&emb_variant.file);
        if !segment_path.is_file() {
            return Err(LocalModelError::MissingFile(segment_path));
        }
        if !embedding_path.is_file() {
            return Err(LocalModelError::MissingFile(embedding_path));
        }

        let segmenter = tract_tflite::tflite()
            .model_for_path(&segment_path)
            .map_err(inference_error)?
            .into_optimized()
            .map_err(inference_error)?
            .into_runnable()
            .map_err(inference_error)?;

        let embedder = tract_onnx::onnx()
            .model_for_path(&embedding_path)
            .map_err(inference_error)?
            .with_input_fact(
                0,
                f32::fact([1, 3, EMBEDDING_SIZE as usize, EMBEDDING_SIZE as usize]).into(),
            )
            .map_err(inference_error)?
            .into_optimized()
            .map_err(inference_error)?
            .into_runnable()
            .map_err(inference_error)?;

        Ok(Self {
            segmenter: Arc::new(segmenter),
            embedder: Arc::new(embedder),
            segmentation_identity: LoadedModelIdentity {
                id: seg_spec.id.clone(),
                version: seg_spec.version.clone(),
                task: InferenceTask::Segmentation,
                file: segment_path,
            },
            embedding_identity: LoadedModelIdentity {
                id: emb_spec.id.clone(),
                version: emb_spec.version.clone(),
                task: InferenceTask::ImageEmbedding,
                file: embedding_path,
            },
        })
    }

    pub fn identity(&self, task: InferenceTask) -> Option<&LoadedModelIdentity> {
        match task {
            InferenceTask::Segmentation => Some(&self.segmentation_identity),
            InferenceTask::ImageEmbedding => Some(&self.embedding_identity),
            _ => None,
        }
    }

    pub fn segment(&self, image: &DynamicImage) -> Result<SegmentationSummary, LocalModelError> {
        let rgb = image.to_rgb8();
        let resized = image::imageops::resize(
            &rgb,
            SEGMENT_SIZE,
            SEGMENT_SIZE,
            FilterType::Triangle,
        );
        let input: Tensor = tract_ndarray::Array4::from_shape_fn(
            (1, SEGMENT_SIZE as usize, SEGMENT_SIZE as usize, 3),
            |(_, y, x, c)| f32::from(resized[(x as u32, y as u32)][c]) / 255.0,
        )
        .into();

        let outputs = self
            .segmenter
            .run(tvec!(input.into()))
            .map_err(inference_error)?;
        let output = outputs
            .first()
            .ok_or_else(|| LocalModelError::Output("segmentation model returned no output".into()))?
            .to_plain_array_view::<f32>()
            .map_err(inference_error)?;
        let shape = output.shape();
        if shape.len() != 4 || shape[0] != 1 || shape[3] != 6 {
            return Err(LocalModelError::Output(format!(
                "expected segmentation output [1,H,W,6], got {shape:?}"
            )));
        }
        let height = shape[1];
        let width = shape[2];
        let flat = output.iter().copied().collect::<Vec<_>>();
        summarize_segmentation(&flat, width, height)
    }

    pub fn embed(&self, image: &DynamicImage) -> Result<Vec<f32>, LocalModelError> {
        let input = dino_input(image);
        let outputs = self
            .embedder
            .run(tvec!(input.into()))
            .map_err(inference_error)?;

        let mut fallback = None;
        for value in &outputs {
            let Ok(view) = value.to_plain_array_view::<f32>() else {
                continue;
            };
            let shape = view.shape();
            if shape.len() == 2 && shape[0] == 1 && shape[1] == DINO_HIDDEN {
                return normalize_embedding(view.iter().copied().collect());
            }
            if shape.len() == 3 && shape[0] == 1 && shape[2] == DINO_HIDDEN {
                fallback = Some(view.iter().take(DINO_HIDDEN).copied().collect::<Vec<_>>());
            }
        }

        fallback
            .ok_or_else(|| {
                LocalModelError::Output(
                    "DINOv2 output did not expose a 384-dimensional pooled/CLS embedding".into(),
                )
            })
            .and_then(normalize_embedding)
    }
}

fn dino_input(image: &DynamicImage) -> Tensor {
    let rgb = image.to_rgb8();
    let side = rgb.width().min(rgb.height()).max(1);
    let left = (rgb.width().saturating_sub(side)) / 2;
    let top = (rgb.height().saturating_sub(side)) / 2;
    let crop = image::imageops::crop_imm(&rgb, left, top, side, side).to_image();
    let resized = image::imageops::resize(
        &crop,
        EMBEDDING_SIZE,
        EMBEDDING_SIZE,
        FilterType::Triangle,
    );
    const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
    const STD: [f32; 3] = [0.229, 0.224, 0.225];

    tract_ndarray::Array4::from_shape_fn(
        (1, 3, EMBEDDING_SIZE as usize, EMBEDDING_SIZE as usize),
        |(_, c, y, x)| {
            let value = f32::from(resized[(x as u32, y as u32)][c]) / 255.0;
            (value - MEAN[c]) / STD[c]
        },
    )
    .into()
}

fn summarize_segmentation(
    values: &[f32],
    width: usize,
    height: usize,
) -> Result<SegmentationSummary, LocalModelError> {
    let pixels = width.saturating_mul(height);
    if values.len() != pixels.saturating_mul(6) || pixels == 0 {
        return Err(LocalModelError::Output(format!(
            "segmentation payload length {} does not match {}x{}x6",
            values.len(), width, height
        )));
    }

    let mut classes = vec![0u8; pixels];
    let mut foreground_confidence = 0.0f32;
    let mut foreground_pixels = 0usize;
    let mut face_pixels = 0usize;

    for pixel in 0..pixels {
        let offset = pixel * 6;
        let scores = &values[offset..offset + 6];
        let (class, score) = scores
            .iter()
            .copied()
            .enumerate()
            .max_by(|left, right| left.1.total_cmp(&right.1))
            .unwrap_or((0, 0.0));
        classes[pixel] = class as u8;
        if class != 0 {
            foreground_pixels += 1;
            foreground_confidence += score;
        }
        if class == 3 {
            face_pixels += 1;
        }
    }

    let foreground_components = connected_components(&classes, width, height, |class| class != 0);
    let face_components = connected_components(&classes, width, height, |class| class == 3);
    let min_person_area = (pixels / 500).max(24);
    let min_face_area = (pixels / 2500).max(8);
    let person_components = foreground_components
        .iter()
        .copied()
        .filter(|area| *area >= min_person_area)
        .collect::<Vec<_>>();
    let person_count = person_components.len() as u32;
    let face_count = face_components
        .iter()
        .filter(|area| **area >= min_face_area)
        .count() as u32;
    let primary_subject_ratio = person_components
        .iter()
        .copied()
        .max()
        .map(|area| area as f32 / pixels as f32)
        .unwrap_or(0.0);
    let foreground_ratio = foreground_pixels as f32 / pixels as f32;
    let face_skin_ratio = face_pixels as f32 / pixels as f32;
    let people_confidence = if foreground_pixels == 0 {
        0.0
    } else {
        (foreground_confidence / foreground_pixels as f32).clamp(0.0, 1.0)
    };

    Ok(SegmentationSummary {
        person_count,
        face_count,
        primary_subject_ratio,
        people_confidence,
        foreground_ratio,
        face_skin_ratio,
    })
}

fn connected_components(
    classes: &[u8],
    width: usize,
    height: usize,
    matches: impl Fn(u8) -> bool,
) -> Vec<usize> {
    let mut visited = vec![false; classes.len()];
    let mut components = Vec::new();
    let mut stack = Vec::new();

    for start in 0..classes.len() {
        if visited[start] || !matches(classes[start]) {
            continue;
        }
        visited[start] = true;
        stack.push(start);
        let mut area = 0usize;

        while let Some(index) = stack.pop() {
            area += 1;
            let x = index % width;
            let y = index / width;
            let neighbors = [
                x.checked_sub(1).map(|nx| y * width + nx),
                (x + 1 < width).then_some(y * width + x + 1),
                y.checked_sub(1).map(|ny| ny * width + x),
                (y + 1 < height).then_some((y + 1) * width + x),
            ];
            for neighbor in neighbors.into_iter().flatten() {
                if !visited[neighbor] && matches(classes[neighbor]) {
                    visited[neighbor] = true;
                    stack.push(neighbor);
                }
            }
        }
        components.push(area);
    }
    components
}

fn normalize_embedding(mut embedding: Vec<f32>) -> Result<Vec<f32>, LocalModelError> {
    let norm = embedding
        .iter()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt();
    if !norm.is_finite() || norm <= f32::EPSILON {
        return Err(LocalModelError::Output("embedding has zero/invalid norm".into()));
    }
    for value in &mut embedding {
        *value /= norm;
    }
    Ok(embedding)
}

fn inference_error(error: impl std::fmt::Display) -> LocalModelError {
    LocalModelError::Inference(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segmentation_summary_ignores_tiny_noise_and_keeps_main_subject() {
        let width = 20usize;
        let height = 20usize;
        let mut values = vec![0.0f32; width * height * 6];
        for pixel in 0..width * height {
            values[pixel * 6] = 0.99;
        }
        for y in 4..16 {
            for x in 5..15 {
                let pixel = y * width + x;
                values[pixel * 6] = 0.01;
                values[pixel * 6 + 4] = 0.95;
            }
        }
        for y in 6..10 {
            for x in 8..12 {
                let pixel = y * width + x;
                values[pixel * 6 + 4] = 0.01;
                values[pixel * 6 + 3] = 0.98;
            }
        }
        let summary = summarize_segmentation(&values, width, height).unwrap();
        assert_eq!(summary.person_count, 1);
        assert_eq!(summary.face_count, 1);
        assert!(summary.primary_subject_ratio > 0.20);
        assert!(summary.people_confidence > 0.90);
    }

    #[test]
    fn embedding_normalization_has_unit_length() {
        let embedding = normalize_embedding(vec![3.0, 4.0]).unwrap();
        let norm = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
    }
}
