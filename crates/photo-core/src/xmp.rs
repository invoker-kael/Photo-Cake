//! Lightroom XMP sidecar bridge.
//!
//! Recipe remains the source of editing decisions. This module serializes
//! supported non-destructive adjustments into small Adobe Camera Raw /
//! Lightroom-compatible sidecars without touching source RAW bytes.

use crate::{RawAsset, Recipe};
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub struct XmpEditState {
    pub recipe_id: String,
    pub target_asset_id: Option<String>,
    pub exposure: Option<f32>,
    pub contrast: Option<f32>,
    pub highlights: Option<f32>,
    pub shadows: Option<f32>,
    pub temperature: Option<f32>,
    pub tint: Option<f32>,
    pub saturation: Option<f32>,
}

#[derive(Debug, Error)]
pub enum XmpWriteError {
    #[error("recipe {0} is not bound to a target asset")]
    MissingTargetAsset(Uuid),
    #[error("target asset {0} is missing from the supplied RAW assets")]
    MissingRawAsset(Uuid),
    #[error("XMP sidecar already exists and will not be overwritten: {0}")]
    ExistingSidecar(PathBuf),
    #[error(transparent)]
    Io(#[from] io::Error),
}

impl XmpEditState {
    pub fn from_recipe(recipe: &Recipe) -> Self {
        Self {
            recipe_id: recipe.id.to_string(),
            target_asset_id: recipe.target_asset_id.map(|id| id.to_string()),
            exposure: recipe.adjustments.exposure,
            contrast: recipe.adjustments.contrast,
            highlights: recipe.adjustments.highlights,
            shadows: recipe.adjustments.shadows,
            temperature: recipe.adjustments.temperature,
            tint: recipe.adjustments.tint,
            saturation: recipe.adjustments.saturation,
        }
    }

    pub fn to_xmp_document(&self) -> String {
        let mut attributes = vec![
            r#"crs:Version="17.0""#.to_string(),
            r#"crs:ProcessVersion="15.4""#.to_string(),
            format!(r#"pc:RecipeId="{}""#, self.recipe_id),
        ];
        if let Some(target_asset_id) = &self.target_asset_id {
            attributes.push(format!(r#"pc:TargetAssetId="{target_asset_id}""#));
        }
        if self.temperature.is_some() || self.tint.is_some() {
            attributes.push(r#"crs:WhiteBalance="Custom""#.to_string());
        }

        push_attr(&mut attributes, "crs:Exposure2012", self.exposure);
        push_attr(&mut attributes, "crs:Contrast2012", self.contrast);
        push_attr(&mut attributes, "crs:Highlights2012", self.highlights);
        push_attr(&mut attributes, "crs:Shadows2012", self.shadows);
        push_attr(&mut attributes, "crs:Temperature", self.temperature);
        push_attr(&mut attributes, "crs:Tint", self.tint);
        push_attr(&mut attributes, "crs:Saturation", self.saturation);

        format!(
            "<?xpacket begin='\u{feff}' id='W5M0MpCehiHzreSzNTczkc9d'?>\n\
<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n\
  <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n\
    <rdf:Description rdf:about=\"\" xmlns:crs=\"http://ns.adobe.com/camera-raw-settings/1.0/\" xmlns:pc=\"https://photo-cake.local/ns/1.0/\"\n      {} />\n\
  </rdf:RDF>\n\
</x:xmpmeta>\n\
<?xpacket end='w'?>\n",
            attributes.join("\n      ")
        )
    }
}

fn push_attr(attributes: &mut Vec<String>, name: &str, value: Option<f32>) {
    if let Some(value) = value {
        attributes.push(format!(r#"{name}="{}""#, format_xmp_number(value)));
    }
}

fn format_xmp_number(value: f32) -> String {
    if (value - value.round()).abs() < 0.0001 {
        return format!("{:.0}", value);
    }

    let mut text = format!("{value:.4}");
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    text
}

pub fn sidecar_path_for_raw(raw_path: &Path) -> PathBuf {
    raw_path.with_extension("xmp")
}

pub fn write_recipe_sidecar(raw_path: &Path, recipe: &Recipe) -> io::Result<PathBuf> {
    let path = sidecar_path_for_raw(raw_path);
    let mut file = OpenOptions::new().write(true).create_new(true).open(&path)?;
    file.write_all(XmpEditState::from_recipe(recipe).to_xmp_document().as_bytes())?;
    file.sync_all()?;
    Ok(path)
}

/// Write one same-basename XMP sidecar for each target-bound Recipe.
///
/// The caller supplies catalog assets, so Photo-Cake never needs to copy RAW
/// files into a managed library just to hand edits to Lightroom.
pub fn write_group_sidecars(
    assets: &[RawAsset],
    recipes: &[Recipe],
) -> Result<Vec<PathBuf>, XmpWriteError> {
    let mut planned = Vec::with_capacity(recipes.len());

    // Preflight the whole group before writing anything so an existing
    // Lightroom sidecar cannot leave a partially updated group.
    for recipe in recipes {
        let target_id = recipe
            .target_asset_id
            .ok_or(XmpWriteError::MissingTargetAsset(recipe.id))?;
        let asset = assets
            .iter()
            .find(|asset| asset.id == target_id)
            .ok_or(XmpWriteError::MissingRawAsset(target_id))?;
        let raw_path = PathBuf::from(&asset.source_path);
        let sidecar = sidecar_path_for_raw(&raw_path);
        if sidecar.exists() {
            return Err(XmpWriteError::ExistingSidecar(sidecar));
        }
        planned.push((raw_path, recipe));
    }

    let mut written = Vec::with_capacity(planned.len());
    for (raw_path, recipe) in planned {
        written.push(write_recipe_sidecar(&raw_path, recipe)?);
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EditAdjustments;
    use tempfile::tempdir;

    fn recipe(target_asset_id: Option<Uuid>) -> Recipe {
        Recipe {
            id: Uuid::nil(),
            name: "test".into(),
            target_asset_id,
            source_reference_ids: vec![],
            adjustments: EditAdjustments {
                exposure: Some(0.35),
                contrast: None,
                highlights: Some(-40.0),
                shadows: Some(25.0),
                temperature: None,
                tint: None,
                saturation: None,
            },
        }
    }

    #[test]
    fn serializes_only_present_adjustments() {
        let xmp = XmpEditState::from_recipe(&recipe(Some(Uuid::nil()))).to_xmp_document();
        assert!(xmp.contains(r#"pc:TargetAssetId="00000000-0000-0000-0000-000000000000""#));
        assert!(xmp.contains(r#"crs:Exposure2012="0.35""#));
        assert!(xmp.contains(r#"crs:Highlights2012="-40""#));
        assert!(!xmp.contains("crs:Contrast2012"));
        assert!(!xmp.contains("crs:Temperature"));
        assert!(!xmp.contains("crs:Tint"));
        assert!(!xmp.contains("crs:WhiteBalance"));
    }

    #[test]
    fn measured_white_balance_marks_custom_and_uses_compact_numbers() {
        let mut recipe = recipe(Some(Uuid::nil()));
        recipe.adjustments.exposure = Some(0.60000002);
        recipe.adjustments.temperature = Some(5700.0);
        recipe.adjustments.tint = Some(3.0);

        let xmp = XmpEditState::from_recipe(&recipe).to_xmp_document();

        assert!(xmp.contains(r#"crs:WhiteBalance="Custom""#));
        assert!(xmp.contains(r#"crs:Exposure2012="0.6""#));
        assert!(xmp.contains(r#"crs:Temperature="5700""#));
        assert!(xmp.contains(r#"crs:Tint="3""#));
    }

    #[test]
    fn sidecar_keeps_raw_basename() {
        assert_eq!(
            sidecar_path_for_raw(Path::new("IMG_0001.CR3")),
            PathBuf::from("IMG_0001.xmp")
        );
    }

    #[test]
    fn refuses_existing_lightroom_sidecar_before_group_write() {
        let dir = tempdir().unwrap();
        let first_path = dir.path().join("IMG_0001.CR3");
        let second_path = dir.path().join("IMG_0002.CR3");
        std::fs::write(&first_path, b"raw-one").unwrap();
        std::fs::write(&second_path, b"raw-two").unwrap();
        std::fs::write(dir.path().join("IMG_0002.xmp"), b"lightroom-edit").unwrap();

        let first_id = Uuid::new_v4();
        let second_id = Uuid::new_v4();
        let assets = vec![
            RawAsset {
                id: first_id,
                source_path: first_path.to_string_lossy().into_owned(),
                filename: "IMG_0001.CR3".into(),
                extension: "cr3".into(),
                camera_id: None,
                capture_time_ms: None,
                file_time_ms: None,
                sequence_number: Some(1),
            },
            RawAsset {
                id: second_id,
                source_path: second_path.to_string_lossy().into_owned(),
                filename: "IMG_0002.CR3".into(),
                extension: "cr3".into(),
                camera_id: None,
                capture_time_ms: None,
                file_time_ms: None,
                sequence_number: Some(2),
            },
        ];

        let error = write_group_sidecars(
            &assets,
            &[recipe(Some(first_id)), recipe(Some(second_id))],
        )
        .unwrap_err();

        assert!(matches!(error, XmpWriteError::ExistingSidecar(_)));
        assert!(!dir.path().join("IMG_0001.xmp").exists());
        assert_eq!(
            std::fs::read(dir.path().join("IMG_0002.xmp")).unwrap(),
            b"lightroom-edit"
        );
    }

    #[test]
    fn writes_target_bound_group_sidecars_without_copying_raws() {
        let dir = tempdir().unwrap();
        let first_path = dir.path().join("IMG_0001.CR3");
        let second_path = dir.path().join("IMG_0002.CR3");
        std::fs::write(&first_path, b"raw-one").unwrap();
        std::fs::write(&second_path, b"raw-two").unwrap();

        let first_id = Uuid::new_v4();
        let second_id = Uuid::new_v4();
        let assets = vec![
            RawAsset {
                id: first_id,
                source_path: first_path.to_string_lossy().into_owned(),
                filename: "IMG_0001.CR3".into(),
                extension: "cr3".into(),
                camera_id: None,
                capture_time_ms: None,
                file_time_ms: None,
                sequence_number: Some(1),
            },
            RawAsset {
                id: second_id,
                source_path: second_path.to_string_lossy().into_owned(),
                filename: "IMG_0002.CR3".into(),
                extension: "cr3".into(),
                camera_id: None,
                capture_time_ms: None,
                file_time_ms: None,
                sequence_number: Some(2),
            },
        ];

        let before_first = std::fs::read(&first_path).unwrap();
        let before_second = std::fs::read(&second_path).unwrap();
        let paths = write_group_sidecars(
            &assets,
            &[recipe(Some(first_id)), recipe(Some(second_id))],
        )
        .unwrap();

        assert_eq!(paths.len(), 2);
        assert!(dir.path().join("IMG_0001.xmp").is_file());
        assert!(dir.path().join("IMG_0002.xmp").is_file());
        assert_eq!(std::fs::read(&first_path).unwrap(), before_first);
        assert_eq!(std::fs::read(&second_path).unwrap(), before_second);
    }
}
