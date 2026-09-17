use crate::RawAsset;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HandoffFormat {
    Tiff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HandoffColorSpace {
    ProPhotoRgb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TiffCompression {
    Zip,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LightroomHandoffPreset {
    pub name: String,
    pub format: HandoffFormat,
    pub bit_depth: u8,
    pub color_space: HandoffColorSpace,
    pub compression: TiffCompression,
    pub full_resolution: bool,
    pub preserve_metadata: bool,
    pub output_sharpening: bool,
}

impl Default for LightroomHandoffPreset {
    fn default() -> Self {
        Self {
            name: "Lightroom Master".to_string(),
            format: HandoffFormat::Tiff,
            bit_depth: 16,
            color_space: HandoffColorSpace::ProPhotoRgb,
            compression: TiffCompression::Zip,
            full_resolution: true,
            preserve_metadata: true,
            output_sharpening: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LightroomHandoffPlan {
    pub id: Uuid,
    pub asset_id: Uuid,
    pub source_path: String,
    pub output_path: String,
    pub preset: LightroomHandoffPreset,
}

#[derive(Debug, Error)]
pub enum HandoffError {
    #[error("source RAW has no parent directory: {0}")]
    MissingSourceParent(String),
    #[error("Lightroom handoff directory must be outside the source RAW directory")]
    OutputInsideSource,
    #[error("source RAW has no usable file stem: {0}")]
    MissingFileStem(String),
}

pub fn plan_lightroom_handoff(
    asset: &RawAsset,
    output_root: impl AsRef<Path>,
    preset: LightroomHandoffPreset,
) -> Result<LightroomHandoffPlan, HandoffError> {
    let source = Path::new(&asset.source_path);
    let source_parent = source
        .parent()
        .ok_or_else(|| HandoffError::MissingSourceParent(asset.source_path.clone()))?;
    let output_root = output_root.as_ref();

    if same_or_descendant(output_root, source_parent) {
        return Err(HandoffError::OutputInsideSource);
    }

    let stem = source
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| HandoffError::MissingFileStem(asset.source_path.clone()))?;

    let output_path = next_available_path(output_root, stem);

    Ok(LightroomHandoffPlan {
        id: Uuid::new_v4(),
        asset_id: asset.id,
        source_path: asset.source_path.clone(),
        output_path: output_path.to_string_lossy().into_owned(),
        preset,
    })
}

fn same_or_descendant(candidate: &Path, parent: &Path) -> bool {
    candidate == parent || candidate.starts_with(parent)
}

fn next_available_path(output_root: &Path, stem: &str) -> PathBuf {
    let first = output_root.join(format!("{stem}-PC.tif"));
    if !first.exists() {
        return first;
    }

    let mut index = 2u32;
    loop {
        let candidate = output_root.join(format!("{stem}-PC-{index}.tif"));
        if !candidate.exists() {
            return candidate;
        }
        index += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn asset(source_path: String) -> RawAsset {
        RawAsset {
            id: Uuid::new_v4(),
            source_path,
            filename: "IMG_0123.CR3".to_string(),
            extension: "cr3".to_string(),
            camera_id: None,
            capture_time_ms: None,
            file_time_ms: None,
            sequence_number: Some(123),
        }
    }

    #[test]
    fn default_preset_is_16_bit_prophoto_zip_tiff() {
        let preset = LightroomHandoffPreset::default();
        assert_eq!(preset.format, HandoffFormat::Tiff);
        assert_eq!(preset.bit_depth, 16);
        assert_eq!(preset.color_space, HandoffColorSpace::ProPhotoRgb);
        assert_eq!(preset.compression, TiffCompression::Zip);
        assert!(preset.full_resolution);
        assert!(!preset.output_sharpening);
    }

    #[test]
    fn rejects_output_inside_source_directory() {
        let dir = tempdir().unwrap();
        let source_dir = dir.path().join("2026-05-Europe");
        std::fs::create_dir_all(&source_dir).unwrap();
        let raw = source_dir.join("IMG_0123.CR3");
        std::fs::write(&raw, b"raw").unwrap();

        let result = plan_lightroom_handoff(
            &asset(raw.to_string_lossy().into_owned()),
            source_dir.join("exports"),
            LightroomHandoffPreset::default(),
        );
        assert!(matches!(result, Err(HandoffError::OutputInsideSource)));
    }

    #[test]
    fn collision_uses_incrementing_suffix_without_overwriting() {
        let dir = tempdir().unwrap();
        let source_dir = dir.path().join("source");
        let output_dir = dir.path().join("handoff");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&output_dir).unwrap();
        let raw = source_dir.join("IMG_0123.CR3");
        std::fs::write(&raw, b"raw").unwrap();
        std::fs::write(output_dir.join("IMG_0123-PC.tif"), b"old").unwrap();

        let plan = plan_lightroom_handoff(
            &asset(raw.to_string_lossy().into_owned()),
            &output_dir,
            LightroomHandoffPreset::default(),
        )
        .unwrap();

        assert!(plan.output_path.ends_with("IMG_0123-PC-2.tif"));
    }
}
