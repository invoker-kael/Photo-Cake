use image::{DynamicImage, imageops::FilterType};
use photo_core::{InferenceTask, ModelBundleManifest, ModelPlatform};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;
use tract_tflite::prelude::*;

const SEGMENT_SIZE: u32 = 256;
const EMBEDDING_SIZE: u32 = 224;

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

        let segmenter = load_tflite_model(&segment_path, &seg_spec.id)?;
        let embedder = load_tflite_model(&embedding_path, &emb_spec.id)?;

        Ok(Self {
            segmenter,
            embedder,
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
            .map_err(|error| inference_error("run segmentation model", error))?;
        let output = outputs
            .first()
            .ok_or_else(|| LocalModelError::Output("segmentation model returned no output".into()))?
            .to_plain_array_view::<f32>()
            .map_err(|error| inference_error("read segmentation output", error))?;
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
        let input: Tensor = tract_ndarray::Array4::from_shape_fn(
            (1, EMBEDDING_SIZE as usize, EMBEDDING_SIZE as usize, 3),
            |(_, y, x, c)| f32::from(resized[(x as u32, y as u32)][c]) / 255.0,
        )
        .into();

        let outputs = self
            .embedder
            .run(tvec!(input.into()))
            .map_err(|error| inference_error("run image embedding model", error))?;
        let mut best = None::<Vec<f32>>;
        for value in &outputs {
            let Ok(view) = value.to_plain_array_view::<f32>() else {
                continue;
            };
            let values = view.iter().copied().collect::<Vec<_>>();
            if values.len() >= 128 && best.as_ref().is_none_or(|current| values.len() > current.len()) {
                best = Some(values);
            }
        }
        normalize_embedding(best.ok_or_else(|| {
            LocalModelError::Output("image embedder returned no usable float embedding".into())
        })?)
    }
}

fn load_tflite_model(path: &Path, model_id: &str) -> Result<Arc<TypedRunnableModel>, LocalModelError> {
    let model = tract_tflite::tflite()
        .model_for_path(path)
        .map_err(|error| inference_error(&format!("load {model_id} from {}", path.display()), error))?;
    let model = model
        .into_optimized()
        .map_err(|error| inference_error(&format!("optimize {model_id}"), error))?;
    model
        .into_runnable()
        .map_err(|error| inference_error(&format!("make {model_id} runnable"), error))
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
    let mut person_confidence = 0.0f32;
    let mut person_pixels = 0usize;
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
        if is_person_class(class as u8) {
            person_pixels += 1;
            person_confidence += score;
        }
        if class == 3 {
            face_pixels += 1;
        }
    }

    let person_components = connected_components(&classes, width, height, is_person_class);
    let face_components = connected_components(&classes, width, height, |class| class == 3);
    let min_person_area = (pixels / 500).max(24);
    let min_face_area = (pixels / 2500).max(8);
    let person_components = person_components
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
    let foreground_ratio = person_pixels as f32 / pixels as f32;
    let face_skin_ratio = face_pixels as f32 / pixels as f32;
    let people_confidence = if person_pixels == 0 {
        0.0
    } else {
        (person_confidence / person_pixels as f32).clamp(0.0, 1.0)
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

fn is_person_class(class: u8) -> bool {
    matches!(class, 1 | 2 | 3 | 4)
}

fn connected_components(
    classes: &[u8],
    width: usize,
    height: usize,
    matches: impl Fn(u8) -> bool + Copy,
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

fn inference_error(context: &str, error: impl std::fmt::Display) -> LocalModelError {
    LocalModelError::Inference(format!("{context}: {error:#}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segmentation_summary_ignores_other_class_and_keeps_main_subject() {
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
        for y in 0..3 {
            for x in 0..3 {
                let pixel = y * width + x;
                values[pixel * 6] = 0.01;
                values[pixel * 6 + 5] = 0.99;
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
