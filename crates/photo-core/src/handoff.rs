use crate::RawAsset;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FinishTarget {
    DirectExport,
    Lightroom,
    Photoshop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HandoffMode {
    DirectExport,
    XmpNative,
    RenderedTiff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FallbackReason {
    StrictSource,
    PixelChangingEdit,
    RequiresAcrSidecar,
    UnsupportedSidecarEdit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditCompatibility {
    pub xmp_native_compatible: bool,
    pub requires_acr_sidecar: bool,
    pub pixel_changing: bool,
}

impl Default for EditCompatibility {
    fn default() -> Self {
        Self {
            xmp_native_compatible: true,
            requires_acr_sidecar: false,
            pixel_changing: false,
        }
    }
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
pub struct RenderedHandoffPreset {
    pub name: String,
    pub bit_depth: u8,
    pub color_space: HandoffColorSpace,
    pub compression: TiffCompression,
    pub full_resolution: bool,
    pub preserve_metadata: bool,
    pub output_sharpening: bool,
}

impl Default for RenderedHandoffPreset {
    fn default() -> Self {
        Self {
            name: "16-bit ProPhoto Master".to_string(),
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
pub struct FinishPlan {
    pub id: Uuid,
    pub asset_id: Uuid,
    pub source_path: String,
    pub target: FinishTarget,
    pub mode: HandoffMode,
    pub target_path: Option<String>,
    pub rendered_preset: Option<RenderedHandoffPreset>,
    pub fallback_reason: Option<FallbackReason>,
}

#[derive(Debug, Error)]
pub enum HandoffError {
    #[error("source RAW has no parent directory: {0}")]
    MissingSourceParent(String),
    #[error("source RAW has no usable file stem: {0}")]
    MissingFileStem(String),
    #[error("rendered handoff requires an output directory")]
    MissingOutputRoot,
    #[error("rendered handoff directory must be outside the source RAW directory")]
    OutputInsideSource,
    #[error("Adobe sidecar already exists and will not be overwritten: {0}")]
    ExistingSidecar(String),
}

pub fn plan_finish(
    asset: &RawAsset,
    target: FinishTarget,
    edits: EditCompatibility,
    strict_source: bool,
    rendered_output_root: Option<&Path>,
) -> Result<FinishPlan, HandoffError> {
    match target {
        FinishTarget::DirectExport => Ok(FinishPlan {
            id: Uuid::new_v4(),
            asset_id: asset.id,
            source_path: asset.source_path.clone(),
            target,
            mode: HandoffMode::DirectExport,
            target_path: None,
            rendered_preset: None,
            fallback_reason: None,
        }),
        FinishTarget::Photoshop => plan_rendered(asset, target, rendered_output_root, None),
        FinishTarget::Lightroom => {
            let fallback_reason = if strict_source {
                Some(FallbackReason::StrictSource)
            } else if edits.pixel_changing {
                Some(FallbackReason::PixelChangingEdit)
            } else if edits.requires_acr_sidecar {
                Some(FallbackReason::RequiresAcrSidecar)
            } else if !edits.xmp_native_compatible {
                Some(FallbackReason::UnsupportedSidecarEdit)
            } else {
                None
            };

            if let Some(reason) = fallback_reason {
                return plan_rendered(asset, target, rendered_output_root, Some(reason));
            }

            let xmp_path = xmp_sidecar_path(asset)?;
            if xmp_path.exists() {
                return Err(HandoffError::ExistingSidecar(
                    xmp_path.to_string_lossy().into_owned(),
                ));
            }

            Ok(FinishPlan {
                id: Uuid::new_v4(),
                asset_id: asset.id,
                source_path: asset.source_path.clone(),
                target,
                mode: HandoffMode::XmpNative,
                target_path: Some(xmp_path.to_string_lossy().into_owned()),
                rendered_preset: None,
                fallback_reason: None,
            })
        }
    }
}

pub fn xmp_sidecar_path(asset: &RawAsset) -> Result<PathBuf, HandoffError> {
    let source = Path::new(&asset.source_path);
    source
        .parent()
        .ok_or_else(|| HandoffError::MissingSourceParent(asset.source_path.clone()))?;
    let stem = source
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| HandoffError::MissingFileStem(asset.source_path.clone()))?;
    Ok(source.with_file_name(format!("{stem}.xmp")))
}

fn plan_rendered(
    asset: &RawAsset,
    target: FinishTarget,
    rendered_output_root: Option<&Path>,
    fallback_reason: Option<FallbackReason>,
) -> Result<FinishPlan, HandoffError> {
    let output_root = rendered_output_root.ok_or(HandoffError::MissingOutputRoot)?;
    let source = Path::new(&asset.source_path);
    let source_parent = source
        .parent()
        .ok_or_else(|| HandoffError::MissingSourceParent(asset.source_path.clone()))?;

    if same_or_descendant(output_root, source_parent) {
        return Err(HandoffError::OutputInsideSource);
    }

    let stem = source
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| HandoffError::MissingFileStem(asset.source_path.clone()))?;
    let target_path = next_available_tiff_path(output_root, stem);

    Ok(FinishPlan {
        id: Uuid::new_v4(),
        asset_id: asset.id,
        source_path: asset.source_path.clone(),
        target,
        mode: HandoffMode::RenderedTiff,
        target_path: Some(target_path.to_string_lossy().into_owned()),
        rendered_preset: Some(RenderedHandoffPreset::default()),
        fallback_reason,
    })
}

fn same_or_descendant(candidate: &Path, parent: &Path) -> bool {
    candidate == parent || candidate.starts_with(parent)
}

fn next_available_tiff_path(output_root: &Path, stem: &str) -> PathBuf {
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
    fn direct_export_is_primary_route_without_sidecar() {
        let item = asset("C:/photos/IMG_0123.CR3".to_string());
        let plan = plan_finish(
            &item,
            FinishTarget::DirectExport,
            EditCompatibility::default(),
            false,
            None,
        )
        .unwrap();
        assert_eq!(plan.mode, HandoffMode::DirectExport);
        assert!(plan.target_path.is_none());
    }

    #[test]
    fn lightroom_uses_same_basename_xmp_for_compatible_edits() {
        let dir = tempdir().unwrap();
        let raw = dir.path().join("IMG_0123.CR3");
        std::fs::write(&raw, b"raw").unwrap();
        let before = std::fs::read(&raw).unwrap();

        let plan = plan_finish(
            &asset(raw.to_string_lossy().into_owned()),
            FinishTarget::Lightroom,
            EditCompatibility::default(),
            false,
            None,
        )
        .unwrap();

        assert_eq!(plan.mode, HandoffMode::XmpNative);
        assert!(plan.target_path.as_ref().unwrap().ends_with("IMG_0123.xmp"));
        assert_eq!(std::fs::read(&raw).unwrap(), before);
    }

    #[test]
    fn existing_xmp_is_never_silently_overwritten() {
        let dir = tempdir().unwrap();
        let raw = dir.path().join("IMG_0123.CR3");
        let xmp = dir.path().join("IMG_0123.xmp");
        std::fs::write(&raw, b"raw").unwrap();
        std::fs::write(&xmp, b"existing").unwrap();

        let result = plan_finish(
            &asset(raw.to_string_lossy().into_owned()),
            FinishTarget::Lightroom,
            EditCompatibility::default(),
            false,
            None,
        );
        assert!(matches!(result, Err(HandoffError::ExistingSidecar(_))));
        assert_eq!(std::fs::read(&xmp).unwrap(), b"existing");
    }

    #[test]
    fn acr_required_masks_fall_back_to_tiff_until_compatible_writer_exists() {
        let dir = tempdir().unwrap();
        let source_dir = dir.path().join("source");
        let output_dir = dir.path().join("handoff");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&output_dir).unwrap();
        let raw = source_dir.join("IMG_0123.CR3");
        std::fs::write(&raw, b"raw").unwrap();

        let plan = plan_finish(
            &asset(raw.to_string_lossy().into_owned()),
            FinishTarget::Lightroom,
            EditCompatibility {
                xmp_native_compatible: false,
                requires_acr_sidecar: true,
                pixel_changing: false,
            },
            false,
            Some(&output_dir),
        )
        .unwrap();

        assert_eq!(plan.mode, HandoffMode::RenderedTiff);
        assert_eq!(plan.fallback_reason, Some(FallbackReason::RequiresAcrSidecar));
    }

    #[test]
    fn photoshop_always_uses_safe_16_bit_prophoto_tiff() {
        let dir = tempdir().unwrap();
        let source_dir = dir.path().join("source");
        let output_dir = dir.path().join("photoshop");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&output_dir).unwrap();
        let raw = source_dir.join("IMG_0123.CR3");
        std::fs::write(&raw, b"raw").unwrap();

        let plan = plan_finish(
            &asset(raw.to_string_lossy().into_owned()),
            FinishTarget::Photoshop,
            EditCompatibility::default(),
            false,
            Some(&output_dir),
        )
        .unwrap();
        let preset = plan.rendered_preset.unwrap();
        assert_eq!(plan.mode, HandoffMode::RenderedTiff);
        assert_eq!(preset.bit_depth, 16);
        assert_eq!(preset.color_space, HandoffColorSpace::ProPhotoRgb);
        assert_eq!(preset.compression, TiffCompression::Zip);
    }

    #[test]
    fn strict_source_forces_rendered_handoff() {
        let dir = tempdir().unwrap();
        let source_dir = dir.path().join("source");
        let output_dir = dir.path().join("handoff");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&output_dir).unwrap();
        let raw = source_dir.join("IMG_0123.CR3");
        std::fs::write(&raw, b"raw").unwrap();

        let plan = plan_finish(
            &asset(raw.to_string_lossy().into_owned()),
            FinishTarget::Lightroom,
            EditCompatibility::default(),
            true,
            Some(&output_dir),
        )
        .unwrap();
        assert_eq!(plan.mode, HandoffMode::RenderedTiff);
        assert_eq!(plan.fallback_reason, Some(FallbackReason::StrictSource));
    }
}