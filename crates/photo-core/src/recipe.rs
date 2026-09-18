use crate::color_sync::{GroupColorSyncPlan, ResolvedColorEdit};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recipe {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub target_asset_id: Option<Uuid>,
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
            target_asset_id: None,
            source_reference_ids: references,
            adjustments: EditAdjustments::default(),
        }
    }

    /// Materialize a per-photo Recipe from an adaptive group synchronization plan.
    ///
    /// The group intent remains shared, while exposure is resolved per asset.
    /// White balance targets come from the shared intent so Lightroom receives
    /// an absolute target instead of an internal temperature/tint delta.
    pub fn from_group_sync(
        name: impl Into<String>,
        reference_ids: Vec<Uuid>,
        plan: &GroupColorSyncPlan,
        resolved: &ResolvedColorEdit,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            target_asset_id: Some(resolved.asset_id),
            source_reference_ids: reference_ids,
            adjustments: EditAdjustments {
                exposure: Some(resolved.exposure_delta_ev),
                contrast: Some(resolved.contrast),
                highlights: None,
                shadows: None,
                temperature: Some(plan.intent.target_temperature_k),
                tint: Some(plan.intent.target_tint),
                saturation: Some(resolved.saturation),
            },
        }
    }

    pub fn materialize_group(
        name_prefix: &str,
        reference_ids: &[Uuid],
        plan: &GroupColorSyncPlan,
    ) -> Vec<Self> {
        plan.resolved
            .iter()
            .map(|resolved| {
                Self::from_group_sync(
                    format!("{name_prefix}-{}", resolved.asset_id),
                    reference_ids.to_vec(),
                    plan,
                    resolved,
                )
            })
            .collect()
    }

    #[test]
    fn group_materialization_preserves_shared_style_and_per_photo_exposure() {
        let reference_id = Uuid::new_v4();
        let dark = Uuid::new_v4();
        let bright = Uuid::new_v4();
        let plan = GroupColorSyncPlan {
            group_id: Uuid::new_v4(),
            mode: GroupSyncMode::ReferenceDriven,
            reference_asset_id: Some(reference_id),
            intent: GroupColorIntent {
                name: "Shared look".into(),
                target_exposure_ev: 0.0,
                target_temperature_k: 5700.0,
                target_tint: 4.0,
                contrast: 6.0,
                saturation: 3.0,
                semantic: vec![],
            },
            revision: 1,
            resolved: vec![
                ResolvedColorEdit {
                    asset_id: dark,
                    exposure_delta_ev: 0.8,
                    temperature_delta_k: 500.0,
                    tint_delta: 4.0,
                    contrast: 6.0,
                    saturation: 3.0,
                    semantic: vec![],
                },
                ResolvedColorEdit {
                    asset_id: bright,
                    exposure_delta_ev: -0.35,
                    temperature_delta_k: -200.0,
                    tint_delta: 4.0,
                    contrast: 6.0,
                    saturation: 3.0,
                    semantic: vec![],
                },
            ],
        };

        let recipes = Recipe::materialize_group("group", &[reference_id], &plan);
        assert_eq!(recipes.len(), 2);
        assert_eq!(recipes[0].adjustments.exposure, Some(0.8));
        assert_eq!(recipes[1].adjustments.exposure, Some(-0.35));
        assert_eq!(recipes[0].adjustments.temperature, Some(5700.0));
        assert_eq!(recipes[1].adjustments.temperature, Some(5700.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color_sync::{GroupColorIntent, GroupColorSyncPlan, GroupSyncMode};

    #[test]
    fn group_sync_materializes_asset_specific_recipe() {
        let asset_id = Uuid::new_v4();
        let reference_id = Uuid::new_v4();
        let edit = ResolvedColorEdit {
            asset_id,
            exposure_delta_ev: 0.6,
            temperature_delta_k: 300.0,
            tint_delta: 2.0,
            contrast: 8.0,
            saturation: 4.0,
            semantic: vec![],
        };
        let plan = GroupColorSyncPlan {
            group_id: Uuid::new_v4(),
            mode: GroupSyncMode::ReferenceDriven,
            reference_asset_id: Some(reference_id),
            intent: GroupColorIntent {
                name: "Reference style".into(),
                target_exposure_ev: 0.0,
                target_temperature_k: 5900.0,
                target_tint: 6.0,
                contrast: 8.0,
                saturation: 4.0,
                semantic: vec![],
            },
            revision: 1,
            resolved: vec![edit.clone()],
        };

        let recipe = Recipe::from_group_sync("asset edit", vec![reference_id], &plan, &edit);
        assert_eq!(recipe.target_asset_id, Some(asset_id));
        assert_eq!(recipe.source_reference_ids, vec![reference_id]);
        assert_eq!(recipe.adjustments.exposure, Some(0.6));
        assert_eq!(recipe.adjustments.temperature, Some(5900.0));
        assert_eq!(recipe.adjustments.tint, Some(6.0));
    }
}
