use crate::color_sync::{GroupColorIntent, PhotoColorAnalysis};
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
    #[serde(default)]
    pub exposure_bias_ev: Option<f32>,
    #[serde(default)]
    pub temperature_bias: Option<f32>,
    #[serde(default)]
    pub tint_bias: Option<f32>,
    pub contrast_preference: Option<f32>,
    pub saturation_preference: Option<f32>,
    pub notes: Option<String>,
}

impl StyleProfile {
    /// Turn photographer preference plus a measured reference photo into a
    /// group-level visual intent. The reference supplies the photographic
    /// baseline; profile values are small, editable preferences on top.
    pub fn to_group_color_intent(
        &self,
        name: impl Into<String>,
        reference: &PhotoColorAnalysis,
    ) -> GroupColorIntent {
        GroupColorIntent {
            name: name.into(),
            target_exposure_ev: reference.exposure_ev + self.exposure_bias_ev.unwrap_or(0.0),
            target_temperature_k: reference.temperature_k + self.temperature_bias.unwrap_or(0.0),
            target_tint: reference.tint + self.tint_bias.unwrap_or(0.0),
            contrast: self.contrast_preference.unwrap_or(0.0),
            saturation: self.saturation_preference.unwrap_or(0.0),
            semantic: Vec::new(),
        }
    }
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

    pub fn color_intent_from_reference(
        &self,
        reference: &PhotoColorAnalysis,
    ) -> GroupColorIntent {
        self.style_profile
            .to_group_color_intent(self.name.clone(), reference)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_profile_builds_intent_from_reference_baseline() {
        let reference = PhotoColorAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: -0.2,
            temperature_k: 5400.0,
            tint: 3.0,
            confidence: 0.95,
        };
        let profile = StyleProfile {
            exposure_bias_ev: Some(0.3),
            temperature_bias: Some(250.0),
            tint_bias: Some(1.0),
            contrast_preference: Some(8.0),
            saturation_preference: Some(4.0),
            notes: None,
        };

        let intent = profile.to_group_color_intent("travel look", &reference);
        assert!((intent.target_exposure_ev - 0.1).abs() < 1e-6);
        assert_eq!(intent.target_temperature_k, 5650.0);
        assert_eq!(intent.target_tint, 4.0);
        assert_eq!(intent.contrast, 8.0);
        assert_eq!(intent.saturation, 4.0);
    }
}
