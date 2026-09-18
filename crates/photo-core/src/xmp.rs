//! Lightroom XMP bridge foundation.
//!
//! Recipe remains the source of editing decisions. This module maps those
//! decisions into an exportable XMP representation without modifying RAW data.

use crate::recipe::Recipe;

#[derive(Debug, Clone)]
pub struct XmpEditState {
    pub recipe_id: String,
    pub exposure: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub temperature: f32,
    pub tint: f32,
    pub saturation: f32,
}

impl XmpEditState {
    pub fn from_recipe(recipe: &Recipe) -> Self {
        Self {
            recipe_id: recipe.id.clone(),
            exposure: recipe.adjustments.exposure,
            contrast: recipe.adjustments.contrast,
            highlights: recipe.adjustments.highlights,
            shadows: recipe.adjustments.shadows,
            temperature: recipe.adjustments.temperature,
            tint: recipe.adjustments.tint,
            saturation: recipe.adjustments.saturation,
        }
    }
}
