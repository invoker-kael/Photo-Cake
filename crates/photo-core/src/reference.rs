use crate::color_sync::{
    build_adaptive_group_plan, ColorSyncError, GroupColorIntent, GroupColorSyncPlan, GroupSyncMode,
    PhotoColorAnalysis,
};
use crate::{AnalysisCache, AnalysisCacheError, InferenceTask, PhotoGroup, Recipe};
use serde::{Deserialize, Serialize};
use thiserror::Error;
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

#[derive(Debug, Clone)]
pub struct ReferenceGroupResult {
    pub plan: GroupColorSyncPlan,
    pub recipes: Vec<Recipe>,
}

#[derive(Debug, Error)]
pub enum ReferenceWorkflowError {
    #[error("selected reference photo is not part of the reference set")]
    ReferenceNotInSet,
    #[error("exposure analysis missing for asset {0}")]
    MissingExposureAnalysis(Uuid),
    #[error("invalid cached exposure analysis for asset {asset_id}: {source}")]
    InvalidExposureAnalysis {
        asset_id: Uuid,
        source: serde_json::Error,
    },
    #[error(transparent)]
    AnalysisCache(#[from] AnalysisCacheError),
    #[error(transparent)]
    ColorSync(#[from] ColorSyncError),
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
            target_temperature_k: reference
                .temperature_k
                .map(|value| value + self.temperature_bias.unwrap_or(0.0)),
            target_tint: reference
                .tint
                .map(|value| value + self.tint_bias.unwrap_or(0.0)),
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

    /// Resolve from cached Analyze evidence without re-running inference.
    ///
    /// Current ExposureAnalysis is preview-relative and intentionally contains
    /// no fabricated RAW white-balance values. Recipe/XMP therefore omit WB
    /// until a reliable RAW/metadata source provides it.
    pub fn resolve_group_from_cache(
        &self,
        cache: &AnalysisCache,
        group: &PhotoGroup,
        selected_reference_asset_id: Uuid,
        revision: u64,
    ) -> Result<ReferenceGroupResult, ReferenceWorkflowError> {
        if !self.photo_ids.contains(&selected_reference_asset_id) {
            return Err(ReferenceWorkflowError::ReferenceNotInSet);
        }

        let reference = cached_exposure_analysis(cache, selected_reference_asset_id)?;
        let mut analyses = Vec::with_capacity(group.asset_ids.len());
        for asset_id in &group.asset_ids {
            analyses.push(cached_exposure_analysis(cache, *asset_id)?);
        }

        self.resolve_group(group, &reference, &analyses, revision)
    }

    /// Resolve one photographer-selected reference look against every photo in
    /// a target group, producing independent target-bound Recipes.
    ///
    /// The reference may live outside the target group, which allows a good
    /// edited image to be reused across similar groups without copying its
    /// numeric settings blindly.
    pub fn resolve_group(
        &self,
        group: &PhotoGroup,
        reference: &PhotoColorAnalysis,
        analyses: &[PhotoColorAnalysis],
        revision: u64,
    ) -> Result<ReferenceGroupResult, ReferenceWorkflowError> {
        if !self.photo_ids.contains(&reference.asset_id) {
            return Err(ReferenceWorkflowError::ReferenceNotInSet);
        }

        let intent = self.color_intent_from_reference(reference);
        let plan = build_adaptive_group_plan(
            group.id,
            &group.asset_ids,
            GroupSyncMode::ReferenceDriven,
            Some(reference.asset_id),
            intent,
            analyses,
            None,
            revision,
        )?;
        let recipes = Recipe::materialize_group(&self.name, &self.photo_ids, &plan);
        Ok(ReferenceGroupResult { plan, recipes })
    }
}

fn cached_exposure_analysis(
    cache: &AnalysisCache,
    asset_id: Uuid,
) -> Result<PhotoColorAnalysis, ReferenceWorkflowError> {
    let artifact = cache
        .latest_for_asset_task(asset_id, InferenceTask::ExposureAnalysis)?
        .ok_or(ReferenceWorkflowError::MissingExposureAnalysis(asset_id))?;
    serde_json::from_value::<PhotoColorAnalysis>(artifact.payload_json).map_err(|source| {
        ReferenceWorkflowError::InvalidExposureAnalysis { asset_id, source }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AnalysisArtifact, AnalysisCacheKey, GroupingBasis, PhotoGroupKind};
    use tempfile::tempdir;

    #[test]
    fn style_profile_builds_intent_from_reference_baseline() {
        let reference = PhotoColorAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: -0.2,
            temperature_k: Some(5400.0),
            tint: Some(3.0),
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
        assert_eq!(intent.target_temperature_k, Some(5650.0));
        assert_eq!(intent.target_tint, Some(4.0));
        assert_eq!(intent.contrast, 8.0);
        assert_eq!(intent.saturation, 4.0);
    }

    fn cache_exposure(
        cache: &AnalysisCache,
        analysis: &PhotoColorAnalysis,
    ) {
        cache
            .put(&AnalysisArtifact {
                key: AnalysisCacheKey {
                    asset_id: analysis.asset_id,
                    source_fingerprint: "raw-v1".into(),
                    preview_revision: "preview-v1".into(),
                    task: InferenceTask::ExposureAnalysis,
                    model_id: "preview-relative-exposure".into(),
                    model_version: "1".into(),
                    config_hash: "trimmed-luma-relative-v1".into(),
                },
                payload_json: serde_json::to_value(analysis).unwrap(),
            })
            .unwrap();
    }

    #[test]
    fn cached_exposure_reference_produces_recipes_without_fake_wb() {
        let dir = tempdir().unwrap();
        let cache = AnalysisCache::open(dir.path().join("project.sqlite3")).unwrap();
        let reference_id = Uuid::new_v4();
        let dark = Uuid::new_v4();
        let bright = Uuid::new_v4();

        for analysis in [
            PhotoColorAnalysis {
                asset_id: reference_id,
                exposure_ev: 0.0,
                temperature_k: None,
                tint: None,
                confidence: 0.95,
            },
            PhotoColorAnalysis {
                asset_id: dark,
                exposure_ev: -0.8,
                temperature_k: None,
                tint: None,
                confidence: 0.95,
            },
            PhotoColorAnalysis {
                asset_id: bright,
                exposure_ev: 0.6,
                temperature_k: None,
                tint: None,
                confidence: 0.95,
            },
        ] {
            cache_exposure(&cache, &analysis);
        }

        let group = PhotoGroup {
            id: Uuid::new_v4(),
            kind: PhotoGroupKind::Similar,
            basis: GroupingBasis::SemanticSimilarity,
            asset_ids: vec![dark, bright],
            manual_locked: false,
        };
        let references = ReferenceSet::from_photos("reference", vec![reference_id]);
        let result = references
            .resolve_group_from_cache(&cache, &group, reference_id, 1)
            .unwrap();

        assert_eq!(result.recipes.len(), 2);
        assert_eq!(result.recipes[0].adjustments.exposure, Some(0.8));
        assert_eq!(result.recipes[1].adjustments.exposure, Some(-0.6));
        assert!(result
            .recipes
            .iter()
            .all(|recipe| recipe.adjustments.temperature.is_none()
                && recipe.adjustments.tint.is_none()));
    }

    #[test]
    fn cached_reference_waits_when_exposure_evidence_is_missing() {
        let dir = tempdir().unwrap();
        let cache = AnalysisCache::open(dir.path().join("project.sqlite3")).unwrap();
        let reference_id = Uuid::new_v4();
        let target = Uuid::new_v4();
        let references = ReferenceSet::from_photos("reference", vec![reference_id]);
        let group = PhotoGroup {
            id: Uuid::new_v4(),
            kind: PhotoGroupKind::Similar,
            basis: GroupingBasis::SemanticSimilarity,
            asset_ids: vec![target],
            manual_locked: false,
        };

        assert!(matches!(
            references.resolve_group_from_cache(&cache, &group, reference_id, 1),
            Err(ReferenceWorkflowError::MissingExposureAnalysis(id)) if id == reference_id
        ));
    }

    #[test]
    fn external_reference_resolves_target_group_per_photo() {
        let reference_id = Uuid::new_v4();
        let dark = Uuid::new_v4();
        let bright = Uuid::new_v4();
        let mut references = ReferenceSet::from_photos("sea look", vec![reference_id]);
        references.style_profile.exposure_bias_ev = Some(0.2);
        references.style_profile.temperature_bias = Some(100.0);

        let group = PhotoGroup {
            id: Uuid::new_v4(),
            kind: PhotoGroupKind::Similar,
            basis: GroupingBasis::SemanticSimilarity,
            asset_ids: vec![dark, bright],
            manual_locked: false,
        };
        let reference = PhotoColorAnalysis {
            asset_id: reference_id,
            exposure_ev: 0.0,
            temperature_k: Some(5600.0),
            tint: Some(2.0),
            confidence: 1.0,
        };
        let analyses = vec![
            PhotoColorAnalysis {
                asset_id: dark,
                exposure_ev: -0.8,
                temperature_k: Some(5200.0),
                tint: Some(0.0),
                confidence: 1.0,
            },
            PhotoColorAnalysis {
                asset_id: bright,
                exposure_ev: 0.6,
                temperature_k: Some(5900.0),
                tint: Some(3.0),
                confidence: 1.0,
            },
        ];

        let result = references
            .resolve_group(&group, &reference, &analyses, 1)
            .unwrap();

        assert_eq!(result.recipes.len(), 2);
        assert_eq!(result.recipes[0].target_asset_id, Some(dark));
        assert_eq!(result.recipes[1].target_asset_id, Some(bright));
        assert_ne!(
            result.recipes[0].adjustments.exposure,
            result.recipes[1].adjustments.exposure
        );
        assert_eq!(result.recipes[0].adjustments.temperature, Some(5700.0));
        assert_eq!(result.recipes[1].adjustments.temperature, Some(5700.0));
    }
}
