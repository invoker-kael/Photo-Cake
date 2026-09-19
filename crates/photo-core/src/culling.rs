//! Smart culling foundation for photographer workflow.
//!
//! This module provides non-destructive review suggestions for RAW collections.
//! It never deletes originals automatically and it does not invent scores for
//! evidence that has not been measured yet.

use crate::{
    detect_exposure_brackets, embedding_similarity, AnalysisCache, AnalysisCacheError,
    ClassificationSignals, ExposureBracketError, ExposureBracketSet, ImageEmbedding, InferenceTask,
    PhotoGroup, SceneTag,
};
use std::collections::{HashMap, HashSet};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

const QUICK_CULL_DUPLICATE_QUALITY_GAP: f32 = 0.08;
const QUICK_CULL_SCENE_DUPLICATE_SIMILARITY: f32 = 0.985;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CullingScore {
    pub sharpness: f32,
    pub blur_penalty: f32,
    pub exposure: f32,
    #[serde(default)]
    pub expression: Option<f32>,
    #[serde(default)]
    pub duplicate_similarity: Option<f32>,
    #[serde(default)]
    pub composition: Option<f32>,
}

impl CullingScore {
    pub fn review_score(&self) -> f32 {
        let technical = self.sharpness * (1.0 - self.blur_penalty.clamp(0.0, 1.0));
        let mut values = vec![technical.clamp(0.0, 1.0), self.exposure.clamp(0.0, 1.0)];
        if let Some(value) = self.expression {
            values.push(value.clamp(0.0, 1.0));
        }
        if let Some(value) = self.composition {
            values.push(value.clamp(0.0, 1.0));
        }
        values.iter().sum::<f32>() / values.len() as f32
    }

    pub fn is_burst_duplicate(&self) -> bool {
        self.duplicate_similarity.is_some_and(|value| value >= 0.95)
    }

    pub fn with_duplicate_similarity(mut self, similarity: f32) -> Self {
        self.duplicate_similarity = Some(similarity.clamp(0.0, 1.0));
        self
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CullingDecision {
    Keep,
    Review,
    RejectSuggestion,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CullingReason {
    StrongTechnicalCandidate,
    LowSharpness,
    BlurRisk,
    ExposureRisk,
    ExposureBracketMember,
    NearDuplicate,
    LowTechnicalQuality,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CullingCandidate {
    pub asset_id: Uuid,
    pub score: CullingScore,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CullingPortraitEvidence {
    pub person_count: u32,
    pub face_count: u32,
    pub primary_subject_ratio: f32,
    pub people_confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CullingRecommendation {
    pub asset_id: Uuid,
    pub quality_score: f32,
    pub decision: CullingDecision,
    pub group_rank: usize,
    #[serde(default)]
    pub reasons: Vec<CullingReason>,
    #[serde(default)]
    pub portrait_evidence: Option<CullingPortraitEvidence>,
    #[serde(default)]
    pub duplicate_similarity: Option<f32>,
    #[serde(default)]
    pub scene_tags: Vec<SceneTag>,
}

pub fn suggest_decision(score: &CullingScore) -> CullingDecision {
    if score.is_burst_duplicate() {
        return CullingDecision::Review;
    }

    match score.review_score() {
        value if value >= 0.85 => CullingDecision::Keep,
        value if value >= 0.55 => CullingDecision::Review,
        _ => CullingDecision::RejectSuggestion,
    }
}

fn recommendation_reasons(
    score: &CullingScore,
    decision: CullingDecision,
) -> Vec<CullingReason> {
    let mut reasons = Vec::new();

    if score.sharpness.clamp(0.0, 1.0) <= 0.45 {
        reasons.push(CullingReason::LowSharpness);
    }
    if score.blur_penalty.clamp(0.0, 1.0) >= 0.45 {
        reasons.push(CullingReason::BlurRisk);
    }
    if score.exposure.clamp(0.0, 1.0) <= 0.45 {
        reasons.push(CullingReason::ExposureRisk);
    }
    if score.is_burst_duplicate() {
        reasons.push(CullingReason::NearDuplicate);
    }

    if decision == CullingDecision::RejectSuggestion {
        reasons.push(CullingReason::LowTechnicalQuality);
    } else if decision == CullingDecision::Keep && reasons.is_empty() {
        reasons.push(CullingReason::StrongTechnicalCandidate);
    }

    reasons
}

/// Rank candidates inside one already-related photo group.
///
/// This is intentionally group-relative: in a burst, the strongest frame is
/// surfaced as the primary candidate while similar alternatives remain
/// reviewable instead of being auto-rejected or deleted.
pub fn rank_group_candidates(candidates: &[CullingCandidate]) -> Vec<CullingRecommendation> {
    let mut ordered = candidates.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| {
        right
            .score
            .review_score()
            .total_cmp(&left.score.review_score())
    });

    ordered
        .into_iter()
        .enumerate()
        .map(|(rank, candidate)| {
            let quality = candidate.score.review_score();
            let mut decision = suggest_decision(&candidate.score);

            if rank == 0 && candidate.score.is_burst_duplicate() && quality >= 0.55 {
                decision = CullingDecision::Keep;
            } else if rank > 0 && candidate.score.is_burst_duplicate() {
                decision = CullingDecision::Review;
            }

            CullingRecommendation {
                asset_id: candidate.asset_id,
                quality_score: quality,
                decision,
                group_rank: rank + 1,
                reasons: recommendation_reasons(&candidate.score, decision),
                portrait_evidence: None,
                duplicate_similarity: candidate.score.duplicate_similarity,
                scene_tags: Vec::new(),
            }
        })
        .collect()
}

/// Combine cached technical quality with existing image embeddings inside one
/// already-related PhotoGroup.
///
/// Similarity is only used to demote later, lower-quality near-duplicates to
/// Review. It never compares unrelated collections globally.
pub fn rank_group_candidates_with_embeddings(
    candidates: &[CullingCandidate],
    embeddings: &[ImageEmbedding],
    duplicate_threshold: f32,
) -> Vec<CullingRecommendation> {
    let mut enriched = candidates.to_vec();
    let mut order = (0..enriched.len()).collect::<Vec<_>>();
    order.sort_by(|left, right| {
        enriched[*right]
            .score
            .review_score()
            .total_cmp(&enriched[*left].score.review_score())
    });

    let threshold = duplicate_threshold.clamp(0.0, 1.0);

    for position in 1..order.len() {
        let current_index = order[position];
        let current_id = enriched[current_index].asset_id;
        let Some(current_embedding) = embeddings.iter().find(|item| item.asset_id == current_id) else {
            continue;
        };
        if current_embedding.vector.is_empty() {
            continue;
        }

        let mut best_similarity = enriched[current_index]
            .score
            .duplicate_similarity
            .unwrap_or(0.0);

        for prior_index in &order[..position] {
            let prior_id = enriched[*prior_index].asset_id;
            let Some(prior_embedding) = embeddings.iter().find(|item| item.asset_id == prior_id) else {
                continue;
            };
            if prior_embedding.vector.is_empty()
                || prior_embedding.vector.len() != current_embedding.vector.len()
            {
                continue;
            }

            best_similarity = best_similarity.max(embedding_similarity(
                &current_embedding.vector,
                &prior_embedding.vector,
            ));
        }

        if best_similarity >= threshold {
            enriched[current_index].score.duplicate_similarity = Some(best_similarity);
        }
    }

    rank_group_candidates(&enriched)
}

fn protect_exposure_bracket_members(
    recommendations: &mut [CullingRecommendation],
    brackets: &[ExposureBracketSet],
) {
    let bracket_assets = brackets
        .iter()
        .flat_map(|set| set.members.iter().map(|member| member.asset_id))
        .collect::<HashSet<_>>();

    for recommendation in recommendations {
        if !bracket_assets.contains(&recommendation.asset_id) {
            continue;
        }

        recommendation
            .reasons
            .retain(|reason| *reason != CullingReason::NearDuplicate);

        if recommendation.decision == CullingDecision::RejectSuggestion {
            recommendation.decision = CullingDecision::Review;
            recommendation
                .reasons
                .retain(|reason| *reason != CullingReason::LowTechnicalQuality);
        } else if recommendation.decision == CullingDecision::Review
            && recommendation.quality_score >= 0.85
            && recommendation.reasons.is_empty()
        {
            recommendation.decision = CullingDecision::Keep;
        }

        if !recommendation
            .reasons
            .contains(&CullingReason::ExposureBracketMember)
        {
            recommendation
                .reasons
                .push(CullingReason::ExposureBracketMember);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MomentQuickCullPlan {
    pub group_id: Uuid,
    pub primary_asset_id: Uuid,
    pub keep_asset_ids: Vec<Uuid>,
    pub review_asset_ids: Vec<Uuid>,
    pub reject_asset_ids: Vec<Uuid>,
    pub contains_people: bool,
    pub people_evidence_complete: bool,
    pub scene_evidence_complete: bool,
    pub scene_consistent: bool,
    #[serde(default)]
    pub shared_scene_tags: Vec<SceneTag>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupCullingResult {
    pub group_id: Uuid,
    pub recommendations: Vec<CullingRecommendation>,
    pub pending_asset_ids: Vec<Uuid>,
    #[serde(default)]
    pub exposure_brackets: Vec<ExposureBracketSet>,
    #[serde(default)]
    pub moment_quick_cull: Option<MomentQuickCullPlan>,
}

/// Build an explicit photographer shortcut for one already-related moment.
pub fn build_moment_quick_cull_plan(
    group_id: Uuid,
    recommendations: &[CullingRecommendation],
    pending_asset_ids: &[Uuid],
    exposure_brackets: &[ExposureBracketSet],
    people_evidence_complete: bool,
    scene_evidence_complete: bool,
) -> Option<MomentQuickCullPlan> {
    if recommendations.len() < 2
        || !pending_asset_ids.is_empty()
        || !exposure_brackets.is_empty()
    {
        return None;
    }

    let primary = recommendations
        .iter()
        .min_by_key(|item| item.group_rank)?;
    if primary.group_rank != 1 || primary.decision != CullingDecision::Keep {
        return None;
    }

    let contains_people = recommendations.iter().any(|item| {
        item.portrait_evidence.as_ref().is_some_and(|evidence| {
            evidence.person_count > 0 || evidence.face_count > 0
        })
    });
    let mut shared_scene_tags = primary.scene_tags.clone();
    for item in recommendations {
        shared_scene_tags.retain(|tag| item.scene_tags.contains(tag));
    }
    let scene_consistent = scene_evidence_complete && !shared_scene_tags.is_empty();
    let mut review_asset_ids = Vec::new();
    let mut reject_asset_ids = Vec::new();

    let mut ordered = recommendations.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|item| item.group_rank);
    for item in ordered {
        if item.asset_id == primary.asset_id {
            continue;
        }

        let duplicate_with_material_gap =
            item.reasons.contains(&CullingReason::NearDuplicate)
                && item.decision != CullingDecision::Keep
                && item
                    .duplicate_similarity
                    .is_some_and(|value| value >= QUICK_CULL_SCENE_DUPLICATE_SIMILARITY)
                && primary.quality_score - item.quality_score
                    >= QUICK_CULL_DUPLICATE_QUALITY_GAP;
        let same_supported_scene = scene_consistent
            && item
                .scene_tags
                .iter()
                .any(|tag| shared_scene_tags.contains(tag));

        if people_evidence_complete
            && scene_evidence_complete
            && !contains_people
            && same_supported_scene
            && duplicate_with_material_gap
        {
            reject_asset_ids.push(item.asset_id);
        } else {
            review_asset_ids.push(item.asset_id);
        }
    }

    Some(MomentQuickCullPlan {
        group_id,
        primary_asset_id: primary.asset_id,
        keep_asset_ids: vec![primary.asset_id],
        review_asset_ids,
        reject_asset_ids,
        contains_people,
        people_evidence_complete,
        scene_evidence_complete,
        scene_consistent,
        shared_scene_tags,
    })
}

#[derive(Debug, Error)]
pub enum CullingEvidenceError {
    #[error(transparent)]
    Analysis(#[from] AnalysisCacheError),
    #[error(transparent)]
    Bracketing(#[from] ExposureBracketError),
    #[error("invalid cached culling evidence for asset {asset_id}: {source}")]
    Json {
        asset_id: Uuid,
        source: serde_json::Error,
    },
}

/// Build culling recommendations entirely from evidence already produced by
/// Analyze. No inference is repeated here.
pub fn build_group_culling_result(
    cache: &AnalysisCache,
    group: &PhotoGroup,
    duplicate_threshold: f32,
) -> Result<GroupCullingResult, CullingEvidenceError> {
    let mut candidates = Vec::new();
    let mut embeddings = Vec::new();
    let mut portrait_evidence = HashMap::new();
    let mut scene_tags = HashMap::new();
    let mut people_evidence_complete = true;
    let mut scene_evidence_complete = true;
    let mut pending_asset_ids = Vec::new();

    for asset_id in &group.asset_ids {
        match cache.latest_for_asset_task(*asset_id, InferenceTask::QualityScoring)? {
            Some(artifact) => {
                let score = serde_json::from_value::<CullingScore>(artifact.payload_json)
                    .map_err(|source| CullingEvidenceError::Json {
                        asset_id: *asset_id,
                        source,
                    })?;
                candidates.push(CullingCandidate {
                    asset_id: *asset_id,
                    score,
                });
            }
            None => {
                pending_asset_ids.push(*asset_id);
                continue;
            }
        }

        if let Some(artifact) =
            cache.latest_for_asset_task(*asset_id, InferenceTask::ImageEmbedding)?
        {
            if let Some(value) = artifact.payload_json.get("embedding") {
                let vector = serde_json::from_value::<Vec<f32>>(value.clone()).map_err(|source| {
                    CullingEvidenceError::Json {
                        asset_id: *asset_id,
                        source,
                    }
                })?;
                if !vector.is_empty() {
                    embeddings.push(ImageEmbedding {
                        asset_id: *asset_id,
                        vector,
                        model_id: artifact.key.model_id,
                        model_version: artifact.key.model_version,
                    });
                }
            }
        }

        match cache.latest_for_asset_task(*asset_id, InferenceTask::Segmentation)? {
            Some(artifact) => {
                let signals = serde_json::from_value::<ClassificationSignals>(artifact.payload_json)
                    .map_err(|source| CullingEvidenceError::Json {
                        asset_id: *asset_id,
                        source,
                    })?;
                scene_tags.insert(*asset_id, signals.scene_tags.clone());
                if signals.detected_person_count > 0 || signals.detected_face_count > 0 {
                    portrait_evidence.insert(
                        *asset_id,
                        CullingPortraitEvidence {
                            person_count: signals.detected_person_count,
                            face_count: signals.detected_face_count,
                            primary_subject_ratio: signals.primary_subject_ratio,
                            people_confidence: signals.people_confidence,
                        },
                    );
                }
            }
            None => {
                people_evidence_complete = false;
                scene_evidence_complete = false;
            }
        }
    }

    let mut recommendations = rank_group_candidates_with_embeddings(
        &candidates,
        &embeddings,
        duplicate_threshold,
    );
    for recommendation in &mut recommendations {
        recommendation.portrait_evidence = portrait_evidence
            .get(&recommendation.asset_id)
            .cloned();
        recommendation.scene_tags = scene_tags
            .get(&recommendation.asset_id)
            .cloned()
            .unwrap_or_default();
    }

    let exposure_brackets = detect_exposure_brackets(cache, group)?;
    protect_exposure_bracket_members(&mut recommendations, &exposure_brackets);
    let moment_quick_cull = build_moment_quick_cull_plan(
        group.id,
        &recommendations,
        &pending_asset_ids,
        &exposure_brackets,
        people_evidence_complete,
        scene_evidence_complete,
    );

    Ok(GroupCullingResult {
        group_id: group.id,
        recommendations,
        pending_asset_ids,
        exposure_brackets,
        moment_quick_cull,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AnalysisArtifact, AnalysisCacheKey, ClassificationSignals, GroupingBasis, PhotoGroupKind,
    };
    use tempfile::tempdir;

    fn score(quality: f32, duplicate_similarity: Option<f32>) -> CullingScore {
        CullingScore {
            sharpness: quality,
            blur_penalty: 0.0,
            exposure: quality,
            expression: Some(quality),
            duplicate_similarity,
            composition: Some(quality),
        }
    }

    #[test]
    fn unknown_semantic_evidence_is_not_faked_into_review_score() {
        let score = CullingScore {
            sharpness: 0.95,
            blur_penalty: 0.0,
            exposure: 0.95,
            expression: None,
            duplicate_similarity: None,
            composition: None,
        };
        assert!((score.review_score() - 0.95).abs() < 1e-6);
        assert_eq!(suggest_decision(&score), CullingDecision::Keep);
    }

    #[test]
    fn low_quality_frame_is_only_a_reject_suggestion() {
        assert_eq!(
            suggest_decision(&score(0.2, None)),
            CullingDecision::RejectSuggestion
        );
    }

    fn embedding(asset_id: Uuid, vector: &[f32]) -> ImageEmbedding {
        ImageEmbedding {
            asset_id,
            vector: vector.to_vec(),
            model_id: "test-embed".to_string(),
            model_version: "1".to_string(),
        }
    }

    #[test]
    fn reasons_explain_only_measured_technical_evidence() {
        let candidate = CullingScore {
            sharpness: 0.38,
            blur_penalty: 0.62,
            exposure: 0.40,
            expression: None,
            duplicate_similarity: Some(0.98),
            composition: None,
        };

        let reasons = recommendation_reasons(&candidate, CullingDecision::RejectSuggestion);
        assert!(reasons.contains(&CullingReason::LowSharpness));
        assert!(reasons.contains(&CullingReason::BlurRisk));
        assert!(reasons.contains(&CullingReason::ExposureRisk));
        assert!(reasons.contains(&CullingReason::NearDuplicate));
        assert!(reasons.contains(&CullingReason::LowTechnicalQuality));
    }

    #[test]
    fn strong_candidate_reason_requires_no_measured_warning() {
        let candidate = CullingScore {
            sharpness: 0.94,
            blur_penalty: 0.02,
            exposure: 0.91,
            expression: None,
            duplicate_similarity: None,
            composition: None,
        };

        assert_eq!(
            recommendation_reasons(&candidate, CullingDecision::Keep),
            vec![CullingReason::StrongTechnicalCandidate]
        );
    }

    #[test]
    fn embeddings_mark_lower_quality_near_duplicate_for_review() {
        let best = Uuid::new_v4();
        let near_duplicate = Uuid::new_v4();
        let different = Uuid::new_v4();

        let ranked = rank_group_candidates_with_embeddings(
            &[
                CullingCandidate {
                    asset_id: best,
                    score: score(0.96, None),
                },
                CullingCandidate {
                    asset_id: near_duplicate,
                    score: score(0.91, None),
                },
                CullingCandidate {
                    asset_id: different,
                    score: score(0.90, None),
                },
            ],
            &[
                embedding(best, &[1.0, 0.0]),
                embedding(near_duplicate, &[0.999, 0.01]),
                embedding(different, &[0.0, 1.0]),
            ],
            0.98,
        );

        assert_eq!(ranked[0].asset_id, best);
        assert_eq!(ranked[0].decision, CullingDecision::Keep);
        let duplicate = ranked
            .iter()
            .find(|item| item.asset_id == near_duplicate)
            .unwrap();
        assert_eq!(duplicate.decision, CullingDecision::Review);
        assert!(duplicate.duplicate_similarity.is_some_and(|value| value >= 0.98));
        let distinct = ranked.iter().find(|item| item.asset_id == different).unwrap();
        assert_eq!(distinct.decision, CullingDecision::Keep);
    }

    fn cache_artifact(
        cache: &AnalysisCache,
        asset_id: Uuid,
        task: InferenceTask,
        payload_json: serde_json::Value,
    ) {
        cache
            .put(&AnalysisArtifact {
                key: AnalysisCacheKey {
                    asset_id,
                    source_fingerprint: "raw-v1".to_string(),
                    preview_revision: "preview-v1".to_string(),
                    task,
                    model_id: match task {
                        InferenceTask::QualityScoring => "quality",
                        InferenceTask::ImageEmbedding => "embedding",
                        _ => "test",
                    }
                    .to_string(),
                    model_version: "1".to_string(),
                    config_hash: "default".to_string(),
                },
                payload_json,
            })
            .unwrap();
    }

    #[test]
    fn group_culling_reuses_cached_quality_and_embedding_evidence() {
        let dir = tempdir().unwrap();
        let cache = AnalysisCache::open(dir.path().join("project.sqlite3")).unwrap();
        let best = Uuid::new_v4();
        let duplicate = Uuid::new_v4();
        let pending = Uuid::new_v4();
        let group = PhotoGroup {
            id: Uuid::new_v4(),
            kind: PhotoGroupKind::Similar,
            basis: GroupingBasis::SemanticSimilarity,
            asset_ids: vec![best, duplicate, pending],
            manual_locked: false,
        };

        cache_artifact(
            &cache,
            best,
            InferenceTask::QualityScoring,
            serde_json::to_value(score(0.95, None)).unwrap(),
        );
        cache_artifact(
            &cache,
            duplicate,
            InferenceTask::QualityScoring,
            serde_json::to_value(score(0.90, None)).unwrap(),
        );
        cache_artifact(
            &cache,
            best,
            InferenceTask::ImageEmbedding,
            serde_json::json!({"embedding": [1.0, 0.0]}),
        );
        cache_artifact(
            &cache,
            duplicate,
            InferenceTask::ImageEmbedding,
            serde_json::json!({"embedding": [0.999, 0.01]}),
        );

        let result = build_group_culling_result(&cache, &group, 0.98).unwrap();
        assert_eq!(result.recommendations.len(), 2);
        assert_eq!(result.recommendations[0].asset_id, best);
        assert_eq!(result.recommendations[0].decision, CullingDecision::Keep);
        assert_eq!(
            result
                .recommendations
                .iter()
                .find(|item| item.asset_id == duplicate)
                .unwrap()
                .decision,
            CullingDecision::Review
        );
        assert_eq!(result.pending_asset_ids, vec![pending]);
        assert!(result.exposure_brackets.is_empty());
    }

    #[test]
    fn exposure_brackets_are_preserved_from_ordinary_culling_rejection() {
        let dir = tempdir().unwrap();
        let cache = AnalysisCache::open(dir.path().join("project.sqlite3")).unwrap();
        let base = Uuid::new_v4();
        let under = Uuid::new_v4();
        let over = Uuid::new_v4();
        let group = PhotoGroup {
            id: Uuid::new_v4(),
            kind: PhotoGroupKind::Moment,
            basis: GroupingBasis::TimeAndSequence,
            asset_ids: vec![base, under, over],
            manual_locked: false,
        };

        for (asset_id, quality, exposure_ev, embedding) in [
            (base, 0.95, 0.0, vec![1.0, 0.0]),
            (under, 0.30, -1.0, vec![0.999, 0.01]),
            (over, 0.35, 1.0, vec![0.998, -0.01]),
        ] {
            cache_artifact(
                &cache,
                asset_id,
                InferenceTask::QualityScoring,
                serde_json::to_value(score(quality, None)).unwrap(),
            );
            cache_artifact(
                &cache,
                asset_id,
                InferenceTask::ExposureAnalysis,
                serde_json::to_value(crate::PhotoColorAnalysis {
                    asset_id,
                    exposure_ev,
                    temperature_k: None,
                    tint: None,
                    confidence: 0.95,
                })
                .unwrap(),
            );
            cache_artifact(
                &cache,
                asset_id,
                InferenceTask::ImageEmbedding,
                serde_json::json!({ "embedding": embedding }),
            );
        }

        let result = build_group_culling_result(&cache, &group, 1.0).unwrap();
        assert_eq!(result.exposure_brackets.len(), 1);
        assert_eq!(result.exposure_brackets[0].center_asset_id, base);
        assert!(result.recommendations.iter().all(|item| {
            item.decision != CullingDecision::RejectSuggestion
                && item.reasons.contains(&CullingReason::ExposureBracketMember)
                && !item.reasons.contains(&CullingReason::NearDuplicate)
        }));
    }

    #[test]
    fn culling_surfaces_portrait_evidence_without_changing_quality_decision() {
        let dir = tempdir().unwrap();
        let cache = AnalysisCache::open(dir.path().join("project.sqlite3")).unwrap();
        let asset_id = Uuid::new_v4();
        let group = PhotoGroup {
            id: Uuid::new_v4(),
            kind: PhotoGroupKind::Similar,
            basis: GroupingBasis::SemanticSimilarity,
            asset_ids: vec![asset_id],
            manual_locked: false,
        };

        cache_artifact(
            &cache,
            asset_id,
            InferenceTask::QualityScoring,
            serde_json::to_value(score(0.92, None)).unwrap(),
        );
        cache_artifact(
            &cache,
            asset_id,
            InferenceTask::Segmentation,
            serde_json::to_value(ClassificationSignals {
                asset_id,
                detected_person_count: 2,
                detected_face_count: 2,
                primary_subject_ratio: 0.31,
                people_confidence: 0.94,
                scene_tags: Vec::new(),
            })
            .unwrap(),
        );

        let result = build_group_culling_result(&cache, &group, 0.98).unwrap();
        let recommendation = &result.recommendations[0];
        assert_eq!(recommendation.decision, CullingDecision::Keep);
        let evidence = recommendation.portrait_evidence.as_ref().unwrap();
        assert_eq!(evidence.person_count, 2);
        assert_eq!(evidence.face_count, 2);
        assert!((evidence.primary_subject_ratio - 0.31).abs() < 1e-6);
        assert!((evidence.people_confidence - 0.94).abs() < 1e-6);
    }

    fn recommendation(
        asset_id: Uuid,
        quality_score: f32,
        decision: CullingDecision,
        rank: usize,
        reasons: Vec<CullingReason>,
        people: bool,
    ) -> CullingRecommendation {
        CullingRecommendation {
            asset_id,
            quality_score,
            decision,
            group_rank: rank,
            reasons: reasons.clone(),
            portrait_evidence: people.then_some(CullingPortraitEvidence {
                person_count: 1,
                face_count: 1,
                primary_subject_ratio: 0.4,
                people_confidence: 0.95,
            }),
            duplicate_similarity: reasons
                .contains(&CullingReason::NearDuplicate)
                .then_some(0.995),
            scene_tags: if people {
                Vec::new()
            } else {
                vec![SceneTag::Landscape]
            },
        }
    }

    #[test]
    fn quick_cull_rejects_only_clear_non_people_near_duplicates() {
        let group_id = Uuid::new_v4();
        let best = Uuid::new_v4();
        let duplicate = Uuid::new_v4();
        let alternate = Uuid::new_v4();
        let plan = build_moment_quick_cull_plan(
            group_id,
            &[
                recommendation(
                    best,
                    0.95,
                    CullingDecision::Keep,
                    1,
                    vec![CullingReason::StrongTechnicalCandidate],
                    false,
                ),
                recommendation(
                    duplicate,
                    0.82,
                    CullingDecision::Review,
                    2,
                    vec![CullingReason::NearDuplicate],
                    false,
                ),
                recommendation(
                    alternate,
                    0.91,
                    CullingDecision::Keep,
                    3,
                    vec![CullingReason::StrongTechnicalCandidate],
                    false,
                ),
            ],
            &[],
            &[],
            true,
            true,
        )
        .unwrap();

        assert_eq!(plan.primary_asset_id, best);
        assert_eq!(plan.keep_asset_ids, vec![best]);
        assert_eq!(plan.reject_asset_ids, vec![duplicate]);
        assert_eq!(plan.review_asset_ids, vec![alternate]);
        assert!(!plan.contains_people);
        assert!(plan.scene_consistent);
        assert_eq!(plan.shared_scene_tags, vec![SceneTag::Landscape]);
    }

    #[test]
    fn quick_cull_keeps_scene_changes_and_weaker_similarity_for_review() {
        let group_id = Uuid::new_v4();
        let best = Uuid::new_v4();
        let changed_scene = Uuid::new_v4();
        let weak_match = Uuid::new_v4();
        let mut changed = recommendation(
            changed_scene,
            0.78,
            CullingDecision::Review,
            2,
            vec![CullingReason::NearDuplicate],
            false,
        );
        changed.scene_tags = vec![SceneTag::Architecture];
        let mut weak = recommendation(
            weak_match,
            0.76,
            CullingDecision::Review,
            3,
            vec![CullingReason::NearDuplicate],
            false,
        );
        weak.duplicate_similarity = Some(0.982);

        let plan = build_moment_quick_cull_plan(
            group_id,
            &[
                recommendation(
                    best,
                    0.96,
                    CullingDecision::Keep,
                    1,
                    vec![CullingReason::StrongTechnicalCandidate],
                    false,
                ),
                changed,
                weak,
            ],
            &[],
            &[],
            true,
            true,
        )
        .unwrap();

        assert!(plan.reject_asset_ids.is_empty());
        assert_eq!(plan.review_asset_ids, vec![changed_scene, weak_match]);
        assert!(!plan.scene_consistent);
    }

    #[test]
    fn quick_cull_preserves_all_people_alternates_for_review() {
        let group_id = Uuid::new_v4();
        let best = Uuid::new_v4();
        let duplicate = Uuid::new_v4();
        let plan = build_moment_quick_cull_plan(
            group_id,
            &[
                recommendation(
                    best,
                    0.96,
                    CullingDecision::Keep,
                    1,
                    vec![CullingReason::StrongTechnicalCandidate],
                    true,
                ),
                recommendation(
                    duplicate,
                    0.70,
                    CullingDecision::Review,
                    2,
                    vec![CullingReason::NearDuplicate],
                    true,
                ),
            ],
            &[],
            &[],
            true,
            true,
        )
        .unwrap();

        assert!(plan.contains_people);
        assert!(plan.people_evidence_complete);
        assert!(plan.reject_asset_ids.is_empty());
        assert_eq!(plan.review_asset_ids, vec![duplicate]);
    }

    #[test]
    fn quick_cull_preserves_duplicates_when_people_evidence_is_incomplete() {
        let group_id = Uuid::new_v4();
        let best = Uuid::new_v4();
        let duplicate = Uuid::new_v4();
        let plan = build_moment_quick_cull_plan(
            group_id,
            &[
                recommendation(
                    best,
                    0.96,
                    CullingDecision::Keep,
                    1,
                    vec![CullingReason::StrongTechnicalCandidate],
                    false,
                ),
                recommendation(
                    duplicate,
                    0.70,
                    CullingDecision::Review,
                    2,
                    vec![CullingReason::NearDuplicate],
                    false,
                ),
            ],
            &[],
            &[],
            false,
            false,
        )
        .unwrap();

        assert!(!plan.people_evidence_complete);
        assert!(!plan.scene_evidence_complete);
        assert!(plan.reject_asset_ids.is_empty());
        assert_eq!(plan.review_asset_ids, vec![duplicate]);
    }

    #[test]
    fn quick_cull_blocks_pending_and_bracket_groups() {
        let group_id = Uuid::new_v4();
        let best = Uuid::new_v4();
        let alternate = Uuid::new_v4();
        let recommendations = vec![
            recommendation(
                best,
                0.95,
                CullingDecision::Keep,
                1,
                vec![CullingReason::StrongTechnicalCandidate],
                false,
            ),
            recommendation(
                alternate,
                0.80,
                CullingDecision::Review,
                2,
                vec![CullingReason::NearDuplicate],
                false,
            ),
        ];

        assert!(build_moment_quick_cull_plan(
            group_id,
            &recommendations,
            &[Uuid::new_v4()],
            &[],
            true,
            true,
        )
        .is_none());

        let bracket = ExposureBracketSet {
            group_id,
            center_asset_id: best,
            members: Vec::new(),
            span_ev: 2.0,
            minimum_embedding_similarity: 0.98,
        };
        assert!(build_moment_quick_cull_plan(
            group_id,
            &recommendations,
            &[],
            &[bracket],
            true,
            true,
        )
        .is_none());
    }

    #[test]
    fn burst_ranking_keeps_best_candidate_and_preserves_alternatives_for_review() {
        let weak = Uuid::new_v4();
        let best = Uuid::new_v4();
        let middle = Uuid::new_v4();
        let ranked = rank_group_candidates(&[
            CullingCandidate {
                asset_id: weak,
                score: score(0.68, Some(0.98)),
            },
            CullingCandidate {
                asset_id: best,
                score: score(0.94, Some(0.99)),
            },
            CullingCandidate {
                asset_id: middle,
                score: score(0.82, Some(0.97)),
            },
        ]);

        assert_eq!(ranked[0].asset_id, best);
        assert_eq!(ranked[0].decision, CullingDecision::Keep);
        assert_eq!(ranked[1].decision, CullingDecision::Review);
        assert_eq!(ranked[2].decision, CullingDecision::Review);
    }
}
