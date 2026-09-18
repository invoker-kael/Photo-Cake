//! Lightroom XMP sidecar bridge.
//!
//! Recipe remains the source of editing decisions. This module serializes
//! supported non-destructive adjustments into a small Adobe Camera Raw /
//! Lightroom-compatible XMP sidecar without touching the source RAW.

use crate::recipe::Recipe;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct XmpEditState {
    pub recipe_id: String,
    pub exposure: Option<f32>,
    pub contrast: Option<f32>,
    pub highlights: Option<f32>,
    pub shadows: Option<f32>,
    pub temperature: Option<f32>,
    pub tint: Option<f32>,
    pub saturation: Option<f32>,
}

impl XmpEditState {
    pub fn from_recipe(recipe: &Recipe) -> Self {
        Self {
            recipe_id: recipe.id.to_string(),
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
        attributes.push(format!(r#"{name}="{value}""#));
    }
}

pub fn sidecar_path_for_raw(raw_path: &Path) -> PathBuf {
    raw_path.with_extension("xmp")
}

pub fn write_recipe_sidecar(raw_path: &Path, recipe: &Recipe) -> io::Result<PathBuf> {
    let path = sidecar_path_for_raw(raw_path);
    fs::write(&path, XmpEditState::from_recipe(recipe).to_xmp_document())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe::{EditAdjustments, Recipe};
    use uuid::Uuid;

    #[test]
    fn serializes_only_present_adjustments() {
        let recipe = Recipe {
            id: Uuid::nil(),
            name: "test".into(),
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
        };

        let xmp = XmpEditState::from_recipe(&recipe).to_xmp_document();
        assert!(xmp.contains(r#"crs:Exposure2012="0.35""#));
        assert!(xmp.contains(r#"crs:Highlights2012="-40""#));
        assert!(!xmp.contains("crs:Contrast2012"));
    }

    #[test]
    fn sidecar_keeps_raw_basename() {
        assert_eq!(
            sidecar_path_for_raw(Path::new("IMG_0001.CR3")),
            PathBuf::from("IMG_0001.xmp")
        );
    }
}
