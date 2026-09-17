use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExportFormat {
    Jpeg,
    Tiff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExportColorSpace {
    Srgb,
    DisplayP3,
    AdobeRgb,
    ProPhotoRgb,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResizeRecipe {
    pub long_edge_px: u32,
    pub allow_upscale: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportRecipe {
    pub format: ExportFormat,
    pub jpeg_quality: Option<u8>,
    pub tiff_bit_depth: Option<u8>,
    pub color_space: ExportColorSpace,
    pub resize: Option<ResizeRecipe>,
    pub preserve_metadata: bool,
    pub destination: PathBuf,
    pub qa_approved_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportPlan {
    pub source: PathBuf,
    pub output: PathBuf,
    pub recipe: ExportRecipe,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ExportPlanError {
    #[error("export destination must not be the source RAW directory")]
    SourceDirectoryDestination,
    #[error("export destination collides with source RAW")]
    SourceCollision,
    #[error("export recipe is invalid: {0}")]
    InvalidRecipe(String),
}

impl ExportRecipe {
    pub fn validate(&self) -> Result<(), ExportPlanError> {
        match self.format {
            ExportFormat::Jpeg => {
                let quality = self.jpeg_quality.ok_or_else(|| {
                    ExportPlanError::InvalidRecipe("JPEG quality is required".into())
                })?;
                if !(1..=100).contains(&quality) {
                    return Err(ExportPlanError::InvalidRecipe(
                        "JPEG quality must be between 1 and 100".into(),
                    ));
                }
            }
            ExportFormat::Tiff => {
                if !matches!(self.tiff_bit_depth, Some(8 | 16)) {
                    return Err(ExportPlanError::InvalidRecipe(
                        "TIFF bit depth must be 8 or 16".into(),
                    ));
                }
            }
        }
        if matches!(self.resize, Some(ResizeRecipe { long_edge_px: 0, .. })) {
            return Err(ExportPlanError::InvalidRecipe(
                "resize long edge must be greater than zero".into(),
            ));
        }
        Ok(())
    }
}

/// Plans a derivative path without touching the source or filesystem.
/// Existing outputs are handled by the executor, which must use create-new semantics.
pub fn plan_export(source: &Path, recipe: &ExportRecipe) -> Result<ExportPlan, ExportPlanError> {
    recipe.validate()?;
    if source.parent() == Some(recipe.destination.as_path()) {
        return Err(ExportPlanError::SourceDirectoryDestination);
    }

    let stem = source
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("photo");
    let extension = match recipe.format {
        ExportFormat::Jpeg => "jpg",
        ExportFormat::Tiff => "tif",
    };
    let output = recipe.destination.join(format!("{stem}.{extension}"));
    if output == source {
        return Err(ExportPlanError::SourceCollision);
    }

    Ok(ExportPlan {
        source: source.to_path_buf(),
        output,
        recipe: recipe.clone(),
    })
}

/// Finds the first unused derivative name. It never overwrites an existing file.
pub fn collision_safe_output(planned: &Path) -> PathBuf {
    if !planned.exists() {
        return planned.to_path_buf();
    }
    let parent = planned.parent().unwrap_or_else(|| Path::new(""));
    let stem = planned
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("photo");
    let extension = planned.extension().and_then(|value| value.to_str());
    for suffix in 1u32.. {
        let name = match extension {
            Some(extension) => format!("{stem}-{suffix}.{extension}"),
            None => format!("{stem}-{suffix}"),
        };
        let candidate = parent.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn jpeg_recipe(destination: PathBuf) -> ExportRecipe {
        ExportRecipe {
            format: ExportFormat::Jpeg,
            jpeg_quality: Some(92),
            tiff_bit_depth: None,
            color_space: ExportColorSpace::Srgb,
            resize: None,
            preserve_metadata: true,
            destination,
            qa_approved_only: true,
        }
    }

    #[test]
    fn recipe_round_trips_for_checkpoint_storage() {
        let recipe = jpeg_recipe(PathBuf::from("exports"));
        let json = serde_json::to_string(&recipe).unwrap();
        assert_eq!(serde_json::from_str::<ExportRecipe>(&json).unwrap(), recipe);
    }

    #[test]
    fn source_directory_is_rejected() {
        let source = Path::new("trip/IMG_0001.CR3");
        let error = plan_export(source, &jpeg_recipe(PathBuf::from("trip"))).unwrap_err();
        assert_eq!(error, ExportPlanError::SourceDirectoryDestination);
    }

    #[test]
    fn collision_gets_new_name_without_touching_raw() {
        let root = tempdir().unwrap();
        let raw_dir = root.path().join("raw");
        let out_dir = root.path().join("exports");
        fs::create_dir_all(&raw_dir).unwrap();
        fs::create_dir_all(&out_dir).unwrap();
        let source = raw_dir.join("IMG_0001.CR3");
        fs::write(&source, b"immutable raw bytes").unwrap();
        let original = fs::read(&source).unwrap();
        let plan = plan_export(&source, &jpeg_recipe(out_dir.clone())).unwrap();
        fs::write(&plan.output, b"existing derivative").unwrap();

        assert_eq!(collision_safe_output(&plan.output), out_dir.join("IMG_0001-1.jpg"));
        assert_eq!(fs::read(&source).unwrap(), original);
    }

    #[test]
    fn invalid_quality_is_rejected() {
        let mut recipe = jpeg_recipe(PathBuf::from("exports"));
        recipe.jpeg_quality = Some(0);
        assert!(matches!(recipe.validate(), Err(ExportPlanError::InvalidRecipe(_))));
    }
}
