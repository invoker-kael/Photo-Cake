use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceSet {
    pub id: Uuid,
    pub name: String,
    pub photo_ids: Vec<Uuid>,
    pub style_profile: StyleProfile,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StyleProfile {
    pub temperature_bias: Option<f32>,
    pub contrast_preference: Option<f32>,
    pub saturation_preference: Option<f32>,
    pub notes: Option<String>,
}

impl ReferenceSet {
    pub fn from_photos(name: impl Into<String>, photo_ids: Vec<Uuid>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            photo_ids,
            style_profile: StyleProfile::default(),
        }
    }
}
