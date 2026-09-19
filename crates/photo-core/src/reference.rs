use crate::bracketing::ExposureBracketSet;
use crate::classification::SceneTag;
use crate::color_sync::{
    build_adaptive_group_plan, reference_relative_contrast_adjustment,
    reference_relative_exposure_correction, reference_relative_saturation_adjustment,
    reference_relative_tone_adjustments, ColorSyncError,
    GroupColorIntent, GroupColorSyncPlan, GroupSyncMode, PhotoColorAnalysis,
    PhotoExposureAnalysis,
};
use crate::culling::{CullingDecision, CullingRecommendation};
use crate::culling_store::{CullingReview, CullingUserDecision};
use crate::{AnalysisCache, AnalysisCacheError, InferenceTask, PhotoGroup, Recipe};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReferenceReadinessStatus {
    Ready,
    NeedsCullReview,
    HdrMergeFirst,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReferenceCandidateSource {
    PhotographerKeep,
    AiKeep,
    PhotographerReview,
    AiReview,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReferenceReadinessPlan {
    pub group_id: Uuid,
    pub status: ReferenceReadinessStatus,
    pub suggested_asset_id: Option<Uuid>,
    pub candidate_source: Option<ReferenceCandidateSource>,
    pub candidate_quality_score: Option<f32>,
    #[serde(default)]
    pub candidate_scene_tags: Vec<SceneTag>,
    pub contains_people: bool,
    #[serde(default)]
    pub pending_asset_ids: Vec<Uuid>,
    #[serde(default)]
    pub excluded_asset_ids: Vec<Uuid>,
    #[serde(default)]
    pub eligible_candidate_ids: Vec<Uuid>,
}

#[derive(Debug, Clone)]
struct RankedReferenceCandidate {
    asset_id: Uuid,
    source: ReferenceCandidateSource,
    quality_score: Option<f32>,
    group_rank: usize,
    position: usize,
    scene_tags: Vec<SceneTag>,
}

fn reference_candidate_priority(source: ReferenceCandidateSource) -> u8 {
    match source {
        ReferenceCandidateSource::PhotographerKeep => 0,
        ReferenceCandidateSource::AiKeep => 1,
        ReferenceCandidateSource::PhotographerReview => 2,
        ReferenceCandidateSource::AiReview => 3,
    }
}

/// Build one canonical batch-Reference preflight from evidence already produced by Cull.
///
/// Photographer decisions remain authoritative. A photographer Keep can advance even
/// while sibling photos are still pending, but an AI-only suggestion waits until the
/// group's Cull evidence is complete. Exposure brackets always route to HDR merge first.
pub fn build_reference_readiness_plan(
    group_id: Uuid,
    asset_ids: &[Uuid],
    recommendations: &[CullingRecommendation],
    pending_asset_ids: &[Uuid],
    exposure_brackets: &[ExposureBracketSet],
    reviews: &[CullingReview],
) -> ReferenceReadinessPlan {
    let recommendations_by_asset = recommendations
        .iter()
        .map(|item| (item.asset_id, item))
        .collect::<HashMap<_, _>>();
    let reviews_by_asset = reviews
        .iter()
        .map(|item| (item.asset_id, item.decision))
        .collect::<HashMap<_, _>>();

    let contains_people = recommendations.iter().any(|item| {
        item.portrait_evidence.as_ref().is_some_and(|evidence| {
            evidence.person_count > 0 || evidence.face_count > 0
        })
    });

    let mut candidates = Vec::new();
    let mut excluded_asset_ids = Vec::new();
    for (position, asset_id) in asset_ids.iter().copied().enumerate() {
        let recommendation = recommendations_by_asset.get(&asset_id).copied();
        let user_decision = reviews_by_asset.get(&asset_id).copied();
        let source = match user_decision {
            Some(CullingUserDecision::Reject) => {
                excluded_asset_ids.push(asset_id);
                None
            }
            Some(CullingUserDecision::Keep) => Some(ReferenceCandidateSource::PhotographerKeep),
            Some(CullingUserDecision::Review) => {
                Some(ReferenceCandidateSource::PhotographerReview)
            }
            None => match recommendation.map(|item| item.decision) {
                Some(CullingDecision::Keep) => Some(ReferenceCandidateSource::AiKeep),
                Some(CullingDecision::Review) => Some(ReferenceCandidateSource::AiReview),
                Some(CullingDecision::RejectSuggestion) => {
                    excluded_asset_ids.push(asset_id);
                    None
                }
                None => None,
            },
        };
        let Some(source) = source else {
            continue;
        };
        candidates.push(RankedReferenceCandidate {
            asset_id,
            source,
            quality_score: recommendation.map(|item| item.quality_score),
            group_rank: recommendation
                .map(|item| item.group_rank)
                .unwrap_or(usize::MAX),
            position,
            scene_tags: recommendation
                .map(|item| item.scene_tags.clone())
                .unwrap_or_default(),
        });
    }

    candidates.sort_by(|left, right| {
        reference_candidate_priority(left.source)
            .cmp(&reference_candidate_priority(right.source))
            .then_with(|| {
                right
                    .quality_score
                    .unwrap_or(-1.0)
                    .total_cmp(&left.quality_score.unwrap_or(-1.0))
            })
            .then_with(|| left.group_rank.cmp(&right.group_rank))
            .then_with(|| left.position.cmp(&right.position))
    });

    let eligible_candidate_ids = candidates
        .iter()
        .map(|candidate| candidate.asset_id)
        .collect::<Vec<_>>();
    let leading = candidates.first();

    if !exposure_brackets.is_empty() {
        return ReferenceReadinessPlan {
            group_id,
            status: ReferenceReadinessStatus::HdrMergeFirst,
            suggested_asset_id: None,
            candidate_source: leading.map(|candidate| candidate.source),
            candidate_quality_score: leading.and_then(|candidate| candidate.quality_score),
            candidate_scene_tags: leading
                .map(|candidate| candidate.scene_tags.clone())
                .unwrap_or_default(),
            contains_people,
            pending_asset_ids: pending_asset_ids.to_vec(),
            excluded_asset_ids,
            eligible_candidate_ids,
        };
    }

    let ready = leading.is_some()
        && (pending_asset_ids.is_empty()
            || leading.is_some_and(|candidate| {
                candidate.source == ReferenceCandidateSource::PhotographerKeep
            }));
    let status = if ready {
        ReferenceReadinessStatus::Ready
    } else {
        ReferenceReadinessStatus::NeedsCullReview
    };

    ReferenceReadinessPlan {
        group_id,
        status,
        suggested_asset_id: ready.then(|| leading.expect("ready requires a candidate").asset_id),
        candidate_source: leading.map(|candidate| candidate.source),
        candidate_quality_score: leading.and_then(|candidate| candidate.quality_score),
        candidate_scene_tags: leading
            .map(|candidate| candidate.scene_tags.clone())
            .unwrap_or_default(),
        contains_people,
        pending_asset_ids: pending_asset_ids.to_vec(),
        excluded_asset_ids,
        eligible_candidate_ids,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StyleSyncGroupContext {
    pub group_id: Uuid,
    pub contains_people: bool,
    #[serde(default)]
    pub scene_tags: Vec<SceneTag>,
    pub evidence_complete: bool,
    #[serde(default)]
    pub pending_asset_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StyleSyncCompatibility {
    Recommended,
    Review,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StyleSyncReason {
    PeopleMatch,
    SharedScene,
    PeopleSceneMismatch,
    SceneMismatch,
    EvidenceIncomplete,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StyleSyncTargetPlan {
    pub target_group_id: Uuid,
    pub compatibility: StyleSyncCompatibility,
    pub reason: StyleSyncReason,
    pub target_context: StyleSyncGroupContext,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StyleSyncPreflight {
    pub source_context: StyleSyncGroupContext,
    #[serde(default)]
    pub targets: Vec<StyleSyncTargetPlan>,
}

/// Summarize one referenced group's existing Cull evidence for StyleProfile reuse.
///
/// Rejected photos do not define the group look: photographer Reject always drops out,
/// and an unoverridden AI Reject suggestion is also excluded. No new classifier runs.
pub fn build_style_sync_group_context(
    group_id: Uuid,
    recommendations: &[CullingRecommendation],
    pending_asset_ids: &[Uuid],
    reviews: &[CullingReview],
) -> StyleSyncGroupContext {
    let reviews_by_asset = reviews
        .iter()
        .map(|item| (item.asset_id, item.decision))
        .collect::<HashMap<_, _>>();

    let mut scene_tags = Vec::new();
    let mut contains_people = false;
    let mut active_count = 0usize;
    let mut active_evidence_complete = true;

    for recommendation in recommendations {
        let user_decision = reviews_by_asset.get(&recommendation.asset_id).copied();
        if user_decision == Some(CullingUserDecision::Reject)
            || (user_decision.is_none()
                && recommendation.decision == CullingDecision::RejectSuggestion)
        {
            continue;
        }

        active_count += 1;
        let has_people = recommendation
            .portrait_evidence
            .as_ref()
            .is_some_and(|evidence| evidence.person_count > 0 || evidence.face_count > 0);
        contains_people |= has_people;
        for tag in &recommendation.scene_tags {
            if !scene_tags.contains(tag) {
                scene_tags.push(*tag);
            }
        }
        if recommendation.scene_tags.is_empty() && !has_people {
            active_evidence_complete = false;
        }
    }

    let pending_asset_ids = pending_asset_ids
        .iter()
        .copied()
        .filter(|asset_id| {
            reviews_by_asset.get(asset_id).copied() != Some(CullingUserDecision::Reject)
        })
        .collect::<Vec<_>>();

    StyleSyncGroupContext {
        group_id,
        contains_people,
        scene_tags,
        evidence_complete: active_count > 0
            && active_evidence_complete
            && pending_asset_ids.is_empty(),
        pending_asset_ids,
    }
}

pub fn style_sync_compatibility(
    source: &StyleSyncGroupContext,
    target: &StyleSyncGroupContext,
) -> (StyleSyncCompatibility, StyleSyncReason) {
    if !source.evidence_complete || !target.evidence_complete {
        return (
            StyleSyncCompatibility::Review,
            StyleSyncReason::EvidenceIncomplete,
        );
    }

    if source.contains_people && target.contains_people {
        return (
            StyleSyncCompatibility::Recommended,
            StyleSyncReason::PeopleMatch,
        );
    }

    if source.contains_people != target.contains_people {
        return (
            StyleSyncCompatibility::Review,
            StyleSyncReason::PeopleSceneMismatch,
        );
    }

    if source
        .scene_tags
        .iter()
        .any(|tag| target.scene_tags.contains(tag))
    {
        return (
            StyleSyncCompatibility::Recommended,
            StyleSyncReason::SharedScene,
        );
    }

    (
        StyleSyncCompatibility::Review,
        StyleSyncReason::SceneMismatch,
    )
}

pub fn build_style_sync_preflight(
    source_context: StyleSyncGroupContext,
    target_contexts: Vec<StyleSyncGroupContext>,
) -> StyleSyncPreflight {
    let targets = target_contexts
        .into_iter()
        .map(|target_context| {
            let (compatibility, reason) =
                style_sync_compatibility(&source_context, &target_context);
            StyleSyncTargetPlan {
                target_group_id: target_context.group_id,
                compatibility,
                reason,
                target_context,
            }
        })
        .collect();

    StyleSyncPreflight {
        source_context,
        targets,
    }
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
        let (target_temperature_k, target_tint) = reference
            .white_balance()
            .map(|(temperature, tint)| {
                (
                    Some(temperature + self.temperature_bias.unwrap_or(0.0)),
                    Some(tint + self.tint_bias.unwrap_or(0.0)),
                )
            })
            .unwrap_or((None, None));

        GroupColorIntent {
            name: name.into(),
            target_exposure_ev: reference.exposure_ev + self.exposure_bias_ev.unwrap_or(0.0),
            target_temperature_k,
            target_tint,
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

        let reference_evidence = cached_exposure_analysis(cache, selected_reference_asset_id)?;
        let reference = reference_evidence.color_analysis();
        let mut evidence = Vec::with_capacity(group.asset_ids.len());
        let mut analyses = Vec::with_capacity(group.asset_ids.len());
        for asset_id in &group.asset_ids {
            let item = cached_exposure_analysis(cache, *asset_id)?;
            analyses.push(item.color_analysis());
            evidence.push(item);
        }

        let mut result = self.resolve_group(group, &reference, &analyses, revision)?;
        for recipe in &mut result.recipes {
            let Some(asset_id) = recipe.target_asset_id else {
                continue;
            };
            let Some(target) = evidence.iter().find(|item| item.asset_id == asset_id) else {
                continue;
            };
            let mut exposure_delta = recipe.adjustments.exposure.unwrap_or(0.0);
            if let Some(exposure_correction) = reference_relative_exposure_correction(
                &reference_evidence,
                target,
                exposure_delta,
                self.style_profile.exposure_bias_ev.unwrap_or(0.0),
            ) {
                exposure_delta = (exposure_delta + exposure_correction).clamp(-4.0, 4.0);
                recipe.adjustments.exposure = Some(exposure_delta);
            }
            if let Some((highlights, shadows)) = reference_relative_tone_adjustments(
                &reference_evidence,
                target,
                exposure_delta,
            ) {
                recipe.adjustments.highlights = Some(highlights);
                recipe.adjustments.shadows = Some(shadows);
            }
            if let Some(contrast_delta) = reference_relative_contrast_adjustment(
                &reference_evidence,
                target,
                exposure_delta,
            ) {
                let baseline = recipe.adjustments.contrast.unwrap_or(0.0);
                recipe.adjustments.contrast =
                    Some((baseline + contrast_delta).clamp(-100.0, 100.0));
            }
            if let Some(saturation_delta) =
                reference_relative_saturation_adjustment(&reference_evidence, target)
            {
                let baseline = recipe.adjustments.saturation.unwrap_or(0.0);
                recipe.adjustments.saturation =
                    Some((baseline + saturation_delta).clamp(-100.0, 100.0));
            }
        }
        Ok(result)
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
) -> Result<PhotoExposureAnalysis, ReferenceWorkflowError> {
    let artifact = cache
        .latest_for_asset_task(asset_id, InferenceTask::ExposureAnalysis)?
        .ok_or(ReferenceWorkflowError::MissingExposureAnalysis(asset_id))?;
    serde_json::from_value::<PhotoExposureAnalysis>(artifact.payload_json).map_err(|source| {
        ReferenceWorkflowError::InvalidExposureAnalysis { asset_id, source }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AnalysisArtifact, AnalysisCacheKey, GroupingBasis, PhotoGroupKind};
    use tempfile::tempdir;

    fn readiness_candidate(
        asset_id: Uuid,
        decision: CullingDecision,
        quality_score: f32,
        group_rank: usize,
        scene_tags: Vec<SceneTag>,
    ) -> CullingRecommendation {
        CullingRecommendation {
            asset_id,
            quality_score,
            decision,
            group_rank,
            reasons: Vec::new(),
            portrait_evidence: None,
            duplicate_similarity: None,
            scene_tags,
        }
    }

    #[test]
    fn scenic_ai_keep_is_ready_when_cull_evidence_is_complete() {
        let group_id = Uuid::new_v4();
        let landscape = Uuid::new_v4();
        let alternate = Uuid::new_v4();
        let plan = build_reference_readiness_plan(
            group_id,
            &[landscape, alternate],
            &[
                readiness_candidate(
                    landscape,
                    CullingDecision::Keep,
                    0.93,
                    1,
                    vec![SceneTag::Landscape],
                ),
                readiness_candidate(
                    alternate,
                    CullingDecision::Review,
                    0.82,
                    2,
                    vec![SceneTag::Landscape],
                ),
            ],
            &[],
            &[],
            &[],
        );

        assert_eq!(plan.status, ReferenceReadinessStatus::Ready);
        assert_eq!(plan.suggested_asset_id, Some(landscape));
        assert_eq!(plan.candidate_source, Some(ReferenceCandidateSource::AiKeep));
        assert_eq!(plan.candidate_scene_tags, vec![SceneTag::Landscape]);
        assert_eq!(plan.eligible_candidate_ids, vec![landscape, alternate]);
    }

    #[test]
    fn ai_reference_waits_for_pending_cull_evidence() {
        let group_id = Uuid::new_v4();
        let first = Uuid::new_v4();
        let pending = Uuid::new_v4();
        let plan = build_reference_readiness_plan(
            group_id,
            &[first, pending],
            &[readiness_candidate(
                first,
                CullingDecision::Keep,
                0.91,
                1,
                vec![SceneTag::Architecture],
            )],
            &[pending],
            &[],
            &[],
        );

        assert_eq!(plan.status, ReferenceReadinessStatus::NeedsCullReview);
        assert_eq!(plan.suggested_asset_id, None);
        assert_eq!(plan.candidate_source, Some(ReferenceCandidateSource::AiKeep));
        assert_eq!(plan.pending_asset_ids, vec![pending]);
    }

    #[test]
    fn photographer_keep_can_advance_a_pending_group() {
        let group_id = Uuid::new_v4();
        let analyzed = Uuid::new_v4();
        let photographer_keep = Uuid::new_v4();
        let plan = build_reference_readiness_plan(
            group_id,
            &[analyzed, photographer_keep],
            &[readiness_candidate(
                analyzed,
                CullingDecision::Keep,
                0.97,
                1,
                vec![SceneTag::Landscape],
            )],
            &[photographer_keep],
            &[],
            &[CullingReview {
                asset_id: photographer_keep,
                decision: CullingUserDecision::Keep,
            }],
        );

        assert_eq!(plan.status, ReferenceReadinessStatus::Ready);
        assert_eq!(plan.suggested_asset_id, Some(photographer_keep));
        assert_eq!(
            plan.candidate_source,
            Some(ReferenceCandidateSource::PhotographerKeep)
        );
        assert_eq!(plan.candidate_quality_score, None);
    }

    #[test]
    fn hdr_bracket_blocks_batch_reference_even_with_photographer_keep() {
        let group_id = Uuid::new_v4();
        let first = Uuid::new_v4();
        let bracket = ExposureBracketSet {
            group_id,
            center_asset_id: first,
            members: Vec::new(),
            span_ev: 2.0,
            minimum_embedding_similarity: 0.99,
        };
        let plan = build_reference_readiness_plan(
            group_id,
            &[first],
            &[readiness_candidate(
                first,
                CullingDecision::Keep,
                0.95,
                1,
                vec![SceneTag::Landscape],
            )],
            &[],
            &[bracket],
            &[CullingReview {
                asset_id: first,
                decision: CullingUserDecision::Keep,
            }],
        );

        assert_eq!(plan.status, ReferenceReadinessStatus::HdrMergeFirst);
        assert_eq!(plan.suggested_asset_id, None);
    }

    #[test]
    fn landscape_style_sync_recommends_shared_scene() {
        let source = StyleSyncGroupContext {
            group_id: Uuid::new_v4(),
            contains_people: false,
            scene_tags: vec![SceneTag::Landscape],
            evidence_complete: true,
            pending_asset_ids: Vec::new(),
        };
        let target = StyleSyncGroupContext {
            group_id: Uuid::new_v4(),
            contains_people: false,
            scene_tags: vec![SceneTag::Landscape, SceneTag::Night],
            evidence_complete: true,
            pending_asset_ids: Vec::new(),
        };

        assert_eq!(
            style_sync_compatibility(&source, &target),
            (
                StyleSyncCompatibility::Recommended,
                StyleSyncReason::SharedScene
            )
        );
    }

    #[test]
    fn people_style_sync_recommends_people_groups() {
        let source = StyleSyncGroupContext {
            group_id: Uuid::new_v4(),
            contains_people: true,
            scene_tags: vec![SceneTag::Architecture],
            evidence_complete: true,
            pending_asset_ids: Vec::new(),
        };
        let target = StyleSyncGroupContext {
            group_id: Uuid::new_v4(),
            contains_people: true,
            scene_tags: vec![SceneTag::Landscape],
            evidence_complete: true,
            pending_asset_ids: Vec::new(),
        };

        assert_eq!(
            style_sync_compatibility(&source, &target),
            (
                StyleSyncCompatibility::Recommended,
                StyleSyncReason::PeopleMatch
            )
        );
    }

    #[test]
    fn portrait_to_landscape_style_sync_requires_review() {
        let source = StyleSyncGroupContext {
            group_id: Uuid::new_v4(),
            contains_people: true,
            scene_tags: vec![SceneTag::Landscape],
            evidence_complete: true,
            pending_asset_ids: Vec::new(),
        };
        let target = StyleSyncGroupContext {
            group_id: Uuid::new_v4(),
            contains_people: false,
            scene_tags: vec![SceneTag::Landscape],
            evidence_complete: true,
            pending_asset_ids: Vec::new(),
        };

        assert_eq!(
            style_sync_compatibility(&source, &target),
            (
                StyleSyncCompatibility::Review,
                StyleSyncReason::PeopleSceneMismatch
            )
        );
    }

    #[test]
    fn unrelated_scenic_style_sync_requires_review() {
        let source = StyleSyncGroupContext {
            group_id: Uuid::new_v4(),
            contains_people: false,
            scene_tags: vec![SceneTag::Landscape],
            evidence_complete: true,
            pending_asset_ids: Vec::new(),
        };
        let target = StyleSyncGroupContext {
            group_id: Uuid::new_v4(),
            contains_people: false,
            scene_tags: vec![SceneTag::Food],
            evidence_complete: true,
            pending_asset_ids: Vec::new(),
        };

        assert_eq!(
            style_sync_compatibility(&source, &target),
            (
                StyleSyncCompatibility::Review,
                StyleSyncReason::SceneMismatch
            )
        );
    }

    #[test]
    fn pending_style_evidence_never_auto_recommends() {
        let source = StyleSyncGroupContext {
            group_id: Uuid::new_v4(),
            contains_people: false,
            scene_tags: vec![SceneTag::Night],
            evidence_complete: false,
            pending_asset_ids: vec![Uuid::new_v4()],
        };
        let target = StyleSyncGroupContext {
            group_id: Uuid::new_v4(),
            contains_people: false,
            scene_tags: vec![SceneTag::Night],
            evidence_complete: true,
            pending_asset_ids: Vec::new(),
        };

        assert_eq!(
            style_sync_compatibility(&source, &target),
            (
                StyleSyncCompatibility::Review,
                StyleSyncReason::EvidenceIncomplete
            )
        );
    }

    #[test]
    fn style_context_ignores_rejected_photos() {
        let kept = Uuid::new_v4();
        let rejected = Uuid::new_v4();
        let context = build_style_sync_group_context(
            Uuid::new_v4(),
            &[
                readiness_candidate(
                    kept,
                    CullingDecision::Keep,
                    0.95,
                    1,
                    vec![SceneTag::Landscape],
                ),
                readiness_candidate(
                    rejected,
                    CullingDecision::Review,
                    0.80,
                    2,
                    vec![SceneTag::Food],
                ),
            ],
            &[],
            &[CullingReview {
                asset_id: rejected,
                decision: CullingUserDecision::Reject,
            }],
        );

        assert!(context.evidence_complete);
        assert_eq!(context.scene_tags, vec![SceneTag::Landscape]);
    }

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
                payload_json: serde_json::to_value(PhotoExposureAnalysis {
                    asset_id: analysis.asset_id,
                    exposure_ev: analysis.exposure_ev,
                    temperature_k: analysis.temperature_k,
                    tint: analysis.tint,
                    confidence: analysis.confidence,
                    luminance_p10: None,
                    luminance_p50: None,
                    luminance_p90: None,
                    shadow_clip_ratio: None,
                    highlight_clip_ratio: None,
                    colorfulness: None,
                }).unwrap(),
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
    fn cached_reference_generates_per_photo_highlight_shadow_recipe() {
        let dir = tempdir().unwrap();
        let cache = AnalysisCache::open(dir.path().join("project.sqlite3")).unwrap();
        let reference_id = Uuid::new_v4();
        let target = Uuid::new_v4();

        for evidence in [
            PhotoExposureAnalysis {
                asset_id: reference_id,
                exposure_ev: 0.0,
                temperature_k: None,
                tint: None,
                confidence: 1.0,
                luminance_p10: Some(0.18),
                luminance_p50: Some(0.48),
                luminance_p90: Some(0.78),
                shadow_clip_ratio: Some(0.0),
                highlight_clip_ratio: Some(0.0),
                colorfulness: None,
            },
            PhotoExposureAnalysis {
                asset_id: target,
                exposure_ev: 0.0,
                temperature_k: None,
                tint: None,
                confidence: 1.0,
                luminance_p10: Some(0.05),
                luminance_p50: Some(0.50),
                luminance_p90: Some(0.96),
                shadow_clip_ratio: Some(0.03),
                highlight_clip_ratio: Some(0.04),
                colorfulness: None,
            },
        ] {
            cache
                .put(&AnalysisArtifact {
                    key: AnalysisCacheKey {
                        asset_id: evidence.asset_id,
                        source_fingerprint: "raw-v2".into(),
                        preview_revision: "preview-v1".into(),
                        task: InferenceTask::ExposureAnalysis,
                        model_id: "preview-relative-exposure".into(),
                        model_version: "2".into(),
                        config_hash: "trimmed-luma-percentiles-relative-v2".into(),
                    },
                    payload_json: serde_json::to_value(evidence).unwrap(),
                })
                .unwrap();
        }

        let group = PhotoGroup {
            id: Uuid::new_v4(),
            kind: PhotoGroupKind::Similar,
            basis: GroupingBasis::SemanticSimilarity,
            asset_ids: vec![target],
            manual_locked: false,
        };
        let references = ReferenceSet::from_photos("reference", vec![reference_id]);
        let result = references
            .resolve_group_from_cache(&cache, &group, reference_id, 1)
            .unwrap();
        let recipe = &result.recipes[0];
        assert!(recipe.adjustments.highlights.unwrap() < -20.0);
        assert!(recipe.adjustments.shadows.unwrap() > 20.0);
    }

    #[test]
    fn cached_reference_refines_exposure_with_median_and_highlight_headroom() {
        let dir = tempdir().unwrap();
        let cache = AnalysisCache::open(dir.path().join("project.sqlite3")).unwrap();
        let reference_id = Uuid::new_v4();
        let dark = Uuid::new_v4();

        for evidence in [
            PhotoExposureAnalysis {
                asset_id: reference_id,
                exposure_ev: 0.0,
                temperature_k: None,
                tint: None,
                confidence: 1.0,
                luminance_p10: Some(0.15),
                luminance_p50: Some(0.50),
                luminance_p90: Some(0.85),
                shadow_clip_ratio: Some(0.0),
                highlight_clip_ratio: Some(0.0),
                colorfulness: Some(0.20),
            },
            PhotoExposureAnalysis {
                asset_id: dark,
                exposure_ev: 0.0,
                temperature_k: None,
                tint: None,
                confidence: 1.0,
                luminance_p10: Some(0.08),
                luminance_p50: Some(0.25),
                luminance_p90: Some(0.45),
                shadow_clip_ratio: Some(0.0),
                highlight_clip_ratio: Some(0.0),
                colorfulness: Some(0.20),
            },
        ] {
            cache
                .put(&AnalysisArtifact {
                    key: AnalysisCacheKey {
                        asset_id: evidence.asset_id,
                        source_fingerprint: "raw-v3".into(),
                        preview_revision: "preview-v1".into(),
                        task: InferenceTask::ExposureAnalysis,
                        model_id: "preview-relative-exposure".into(),
                        model_version: "3".into(),
                        config_hash: "trimmed-luma-percentiles-color-relative-v3".into(),
                    },
                    payload_json: serde_json::to_value(evidence).unwrap(),
                })
                .unwrap();
        }

        let group = PhotoGroup {
            id: Uuid::new_v4(),
            kind: PhotoGroupKind::Similar,
            basis: GroupingBasis::SemanticSimilarity,
            asset_ids: vec![dark],
            manual_locked: false,
        };
        let references = ReferenceSet::from_photos("reference", vec![reference_id]);
        let result = references
            .resolve_group_from_cache(&cache, &group, reference_id, 1)
            .unwrap();
        assert!(result.recipes[0].adjustments.exposure.unwrap() > 0.30);
    }

    #[test]
    fn cached_reference_adapts_contrast_and_saturation_per_photo() {
        let dir = tempdir().unwrap();
        let cache = AnalysisCache::open(dir.path().join("project.sqlite3")).unwrap();
        let reference_id = Uuid::new_v4();
        let flat_muted = Uuid::new_v4();

        for evidence in [
            PhotoExposureAnalysis {
                asset_id: reference_id,
                exposure_ev: 0.0,
                temperature_k: None,
                tint: None,
                confidence: 1.0,
                luminance_p10: Some(0.15),
                luminance_p50: Some(0.50),
                luminance_p90: Some(0.85),
                shadow_clip_ratio: Some(0.0),
                highlight_clip_ratio: Some(0.0),
                colorfulness: Some(0.30),
            },
            PhotoExposureAnalysis {
                asset_id: flat_muted,
                exposure_ev: 0.0,
                temperature_k: None,
                tint: None,
                confidence: 1.0,
                luminance_p10: Some(0.30),
                luminance_p50: Some(0.50),
                luminance_p90: Some(0.70),
                shadow_clip_ratio: Some(0.0),
                highlight_clip_ratio: Some(0.0),
                colorfulness: Some(0.10),
            },
        ] {
            cache
                .put(&AnalysisArtifact {
                    key: AnalysisCacheKey {
                        asset_id: evidence.asset_id,
                        source_fingerprint: "raw-v3".into(),
                        preview_revision: "preview-v1".into(),
                        task: InferenceTask::ExposureAnalysis,
                        model_id: "preview-relative-exposure".into(),
                        model_version: "3".into(),
                        config_hash: "trimmed-luma-percentiles-color-relative-v3".into(),
                    },
                    payload_json: serde_json::to_value(evidence).unwrap(),
                })
                .unwrap();
        }

        let group = PhotoGroup {
            id: Uuid::new_v4(),
            kind: PhotoGroupKind::Similar,
            basis: GroupingBasis::SemanticSimilarity,
            asset_ids: vec![flat_muted],
            manual_locked: false,
        };
        let mut references = ReferenceSet::from_photos("reference", vec![reference_id]);
        references.style_profile.contrast_preference = Some(5.0);
        references.style_profile.saturation_preference = Some(3.0);
        let result = references
            .resolve_group_from_cache(&cache, &group, reference_id, 1)
            .unwrap();
        let recipe = &result.recipes[0];
        assert!(recipe.adjustments.contrast.unwrap() > 15.0);
        assert!(recipe.adjustments.saturation.unwrap() > 13.0);
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
