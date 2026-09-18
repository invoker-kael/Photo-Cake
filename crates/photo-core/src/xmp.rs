//! Lightroom XMP sidecar bridge.
//!
//! Recipe remains the source of editing decisions. This module serializes
//! supported non-destructive adjustments into small Adobe Camera Raw /
//! Lightroom-compatible sidecars without touching source RAW bytes.

use crate::{RawAsset, Recipe};
use quick_xml::{events::Event, Reader};
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
pub enum XmpParseError {
    #[error("invalid XMP: {0}")]
    Invalid(String),
    #[error("XMP is missing Photo-Cake recipe identity")]
    MissingRecipeId,
}

#[derive(Debug, Error)]
pub enum XmpWriteError {
    #[error("recipe {0} is not bound to a target asset")]
    MissingTargetAsset(Uuid),
    #[error("target asset {0} is missing from the supplied RAW assets")]
    MissingRawAsset(Uuid),
    #[error("XMP sidecar already exists and will not be overwritten: {0}")]
    ExistingSidecar(PathBuf),
    #[error("written XMP failed round-trip validation: {0}")]
    RoundTrip(String),
    #[error(transparent)]
    Io(#[from] io::Error),
}

impl XmpEditState {
    pub fn from_recipe(recipe: &Recipe) -> Self {
        let (temperature, tint) = recipe
            .adjustments
            .temperature
            .zip(recipe.adjustments.tint)
            .map(|value| (Some(value.0), Some(value.1)))
            .unwrap_or((None, None));

        Self {
            recipe_id: recipe.id.to_string(),
            target_asset_id: recipe.target_asset_id.map(|id| id.to_string()),
            exposure: recipe.adjustments.exposure,
            contrast: recipe.adjustments.contrast,
            highlights: recipe.adjustments.highlights,
            shadows: recipe.adjustments.shadows,
            temperature,
            tint,
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
        if self.temperature.is_some() && self.tint.is_some() {
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


    pub fn from_xmp_document(document: &str) -> Result<Self, XmpParseError> {
        let mut reader = Reader::from_str(document);
        reader.config_mut().trim_text(true);
        let mut state = XmpEditState {
            recipe_id: String::new(),
            target_asset_id: None,
            exposure: None,
            contrast: None,
            highlights: None,
            shadows: None,
            temperature: None,
            tint: None,
            saturation: None,
        };

        loop {
            match reader.read_event() {
                Ok(Event::Start(element)) | Ok(Event::Empty(element)) => {
                    if element.name().as_ref() != b"rdf:Description" {
                        continue;
                    }
                    for attribute in element.attributes().with_checks(false) {
                        let attribute = attribute
                            .map_err(|error| XmpParseError::Invalid(error.to_string()))?;
                        let key = std::str::from_utf8(attribute.key.as_ref())
                            .map_err(|error| XmpParseError::Invalid(error.to_string()))?;
                        let value = attribute
                            .decode_and_unescape_value(reader.decoder())
                            .map_err(|error| XmpParseError::Invalid(error.to_string()))?
                            .into_owned();
                        assign_xmp_attribute(&mut state, key, &value)?;
                    }
                }
                Ok(Event::Eof) => break,
                Ok(_) => {}
                Err(error) => return Err(XmpParseError::Invalid(error.to_string())),
            }
        }

        if state.recipe_id.is_empty() {
            return Err(XmpParseError::MissingRecipeId);
        }
        Ok(state)
    }
}

fn assign_xmp_attribute(
    state: &mut XmpEditState,
    name: &str,
    value: &str,
) -> Result<(), XmpParseError> {
    let local = name.rsplit(':').next().unwrap_or(name);
    match local {
        "RecipeId" => state.recipe_id = value.to_string(),
        "TargetAssetId" => state.target_asset_id = Some(value.to_string()),
        "Exposure2012" => state.exposure = Some(parse_xmp_number(name, value)?),
        "Contrast2012" => state.contrast = Some(parse_xmp_number(name, value)?),
        "Highlights2012" => state.highlights = Some(parse_xmp_number(name, value)?),
        "Shadows2012" => state.shadows = Some(parse_xmp_number(name, value)?),
        "Temperature" => state.temperature = Some(parse_xmp_number(name, value)?),
        "Tint" => state.tint = Some(parse_xmp_number(name, value)?),
        "Saturation" => state.saturation = Some(parse_xmp_number(name, value)?),
        _ => {}
    }
    Ok(())
}

fn parse_xmp_number(name: &str, value: &str) -> Result<f32, XmpParseError> {
    value
        .parse::<f32>()
        .map_err(|error| XmpParseError::Invalid(format!("{name}={value}: {error}")))
}

pub fn validate_recipe_xmp(recipe: &Recipe, document: &str) -> Result<(), XmpParseError> {
    let expected = XmpEditState::from_recipe(recipe);
    let actual = XmpEditState::from_xmp_document(document)?;

    if expected.recipe_id != actual.recipe_id {
        return Err(XmpParseError::Invalid("recipe id mismatch".to_string()));
    }
    if expected.target_asset_id != actual.target_asset_id {
        return Err(XmpParseError::Invalid("target asset id mismatch".to_string()));
    }

    for (name, expected, actual) in [
        ("exposure", expected.exposure, actual.exposure),
        ("contrast", expected.contrast, actual.contrast),
        ("highlights", expected.highlights, actual.highlights),
        ("shadows", expected.shadows, actual.shadows),
        ("temperature", expected.temperature, actual.temperature),
        ("tint", expected.tint, actual.tint),
        ("saturation", expected.saturation, actual.saturation),
    ] {
        if !same_xmp_number(expected, actual) {
            return Err(XmpParseError::Invalid(format!("{name} mismatch")));
        }
    }
    Ok(())
}

fn same_xmp_number(left: Option<f32>, right: Option<f32>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => (left - right).abs() <= 0.0002,
        _ => false,
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

fn existing_sidecar_path(raw_path: &Path) -> Option<PathBuf> {
    let parent = raw_path.parent().unwrap_or_else(|| Path::new("."));
    let raw_stem = raw_path.file_stem()?;

    if let Ok(entries) = std::fs::read_dir(parent) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() || path.file_stem() != Some(raw_stem) {
                continue;
            }
            let is_xmp = path
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("xmp"));
            if is_xmp {
                return Some(path);
            }
        }
    }

    None
}

pub fn write_recipe_sidecar(
    raw_path: &Path,
    recipe: &Recipe,
) -> Result<PathBuf, XmpWriteError> {
    let path = sidecar_path_for_raw(raw_path);
    let document = XmpEditState::from_recipe(recipe).to_xmp_document();
    let mut file = OpenOptions::new().write(true).create_new(true).open(&path)?;
    file.write_all(document.as_bytes())?;
    file.sync_all()?;
    drop(file);

    let written = std::fs::read_to_string(&path)?;
    if let Err(error) = validate_recipe_xmp(recipe, &written) {
        let _ = std::fs::remove_file(&path);
        return Err(XmpWriteError::RoundTrip(error.to_string()));
    }

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
        if let Some(sidecar) = existing_sidecar_path(&raw_path) {
            return Err(XmpWriteError::ExistingSidecar(sidecar));
        }
        planned.push((raw_path, recipe));
    }

    let mut written = Vec::with_capacity(planned.len());
    for (raw_path, recipe) in planned {
        match write_recipe_sidecar(&raw_path, recipe) {
            Ok(path) => written.push(path),
            Err(error) => {
                for path in &written {
                    let _ = std::fs::remove_file(path);
                }
                return Err(error);
            }
        }
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EditAdjustments;
    use tempfile::tempdir;

    fn matching_xmp_sidecars(directory: &Path, stem: &str) -> usize {
        std::fs::read_dir(directory)
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_stem().and_then(|value| value.to_str()) == Some(stem)
                    && path
                        .extension()
                        .and_then(|value| value.to_str())
                        .is_some_and(|value| value.eq_ignore_ascii_case("xmp"))
            })
            .count()
    }

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
    fn partial_white_balance_is_omitted_from_xmp() {
        let mut source = recipe(Some(Uuid::nil()));
        source.adjustments.temperature = Some(5700.0);
        source.adjustments.tint = None;

        let xmp = XmpEditState::from_recipe(&source).to_xmp_document();

        assert!(!xmp.contains("crs:WhiteBalance"));
        assert!(!xmp.contains("crs:Temperature"));
        assert!(!xmp.contains("crs:Tint"));
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
    fn xmp_round_trip_preserves_supported_recipe_state() {
        let mut source = recipe(Some(Uuid::new_v4()));
        source.id = Uuid::new_v4();
        source.adjustments.contrast = Some(12.0);
        source.adjustments.saturation = Some(-7.0);
        source.adjustments.temperature = Some(6100.0);
        source.adjustments.tint = Some(-3.0);

        let document = XmpEditState::from_recipe(&source).to_xmp_document();
        validate_recipe_xmp(&source, &document).unwrap();

        let parsed = XmpEditState::from_xmp_document(&document).unwrap();
        assert_eq!(parsed.recipe_id, source.id.to_string());
        assert_eq!(
            parsed.target_asset_id,
            source.target_asset_id.map(|value| value.to_string())
        );
        assert_eq!(parsed.temperature, Some(6100.0));
        assert_eq!(parsed.tint, Some(-3.0));
    }

    #[test]
    fn parses_lightroom_style_xmp_even_when_namespace_prefixes_change() {
        let recipe_id = Uuid::new_v4();
        let asset_id = Uuid::new_v4();
        let document = format!(
            r#"<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    <rdf:Description
      xmlns:cameraRaw="http://ns.adobe.com/camera-raw-settings/1.0/"
      xmlns:photoCake="https://photo-cake.local/ns/1.0/"
      cameraRaw:Exposure2012="0.45"
      cameraRaw:Contrast2012="12"
      photoCake:RecipeId="{recipe_id}"
      photoCake:TargetAssetId="{asset_id}"
      dc:format="image/x-canon-cr3"
      xmlns:dc="http://purl.org/dc/elements/1.1/" />
  </rdf:RDF>
</x:xmpmeta>"#
        );

        let parsed = XmpEditState::from_xmp_document(&document).unwrap();
        assert_eq!(parsed.recipe_id, recipe_id.to_string());
        assert_eq!(parsed.target_asset_id, Some(asset_id.to_string()));
        assert_eq!(parsed.exposure, Some(0.45));
        assert_eq!(parsed.contrast, Some(12.0));
    }

    #[test]
    fn malformed_or_wrong_identity_fails_round_trip_gate() {
        let source = recipe(Some(Uuid::new_v4()));
        let document = XmpEditState::from_recipe(&source)
            .to_xmp_document()
            .replace(&source.id.to_string(), &Uuid::new_v4().to_string());

        assert!(validate_recipe_xmp(&source, &document).is_err());
        assert!(XmpEditState::from_xmp_document("<x:xmpmeta />").is_err());
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
    fn refuses_uppercase_existing_lightroom_sidecar() {
        let dir = tempdir().unwrap();
        let raw_path = dir.path().join("IMG_0099.CR3");
        std::fs::write(&raw_path, b"raw").unwrap();
        let existing = dir.path().join("IMG_0099.XMP");
        std::fs::write(&existing, b"existing").unwrap();

        let asset_id = Uuid::new_v4();
        let asset = RawAsset {
            id: asset_id,
            source_path: raw_path.to_string_lossy().into_owned(),
            filename: "IMG_0099.CR3".into(),
            extension: "cr3".into(),
            camera_id: None,
            capture_time_ms: None,
            file_time_ms: None,
            sequence_number: Some(99),
        };

        let error = write_group_sidecars(&[asset], &[recipe(Some(asset_id))]).unwrap_err();
        assert!(matches!(error, XmpWriteError::ExistingSidecar(path) if path.ends_with("IMG_0099.XMP")));
        assert_eq!(std::fs::read(&existing).unwrap(), b"existing");
        assert_eq!(matching_xmp_sidecars(dir.path(), "IMG_0099"), 1);
    }

    #[test]
    fn mixed_case_existing_sidecar_is_protected() {
        let dir = tempdir().unwrap();
        let raw_path = dir.path().join("IMG_0100.CR3");
        std::fs::write(&raw_path, b"raw").unwrap();
        let existing = dir.path().join("IMG_0100.XmP");
        std::fs::write(&existing, b"existing").unwrap();

        let asset_id = Uuid::new_v4();
        let asset = RawAsset {
            id: asset_id,
            source_path: raw_path.to_string_lossy().into_owned(),
            filename: "IMG_0100.CR3".into(),
            extension: "cr3".into(),
            camera_id: None,
            capture_time_ms: None,
            file_time_ms: None,
            sequence_number: Some(100),
        };

        let error = write_group_sidecars(&[asset], &[recipe(Some(asset_id))]).unwrap_err();
        assert!(matches!(error, XmpWriteError::ExistingSidecar(path) if path.ends_with("IMG_0100.XmP")));
        assert_eq!(std::fs::read(&existing).unwrap(), b"existing");
        assert_eq!(matching_xmp_sidecars(dir.path(), "IMG_0100"), 1);
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
