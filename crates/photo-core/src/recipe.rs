use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recipe {
    pub id: Uuid,
    pub name: String,
    pub source_reference_ids: Vec<Uuid>,
    pub adjustments: EditAdjustments,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EditAdjustments {
    pub exposure: Option<f32>,
    pub contrast: Option<f32>,
    pub highlights: Option<f32>,
    pub shadows: Option<f32>,
    pub temperature: Option<f32>,
    pub tint: Option<f32>,
    pub saturation: Option<f32>,
}

impl Recipe {
    pub fn from_reference(name: impl Into<String>, references: Vec<Uuid>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            source_reference_ids: references,
            adjustments: EditAdjustments::default(),
        }
    }
}
