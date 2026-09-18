//! Smart culling foundation for photographer workflow.
//!
//! This module provides non-destructive review suggestions for RAW collections.
//! It never deletes originals automatically and it does not invent scores for
//! evidence that has not been measured yet.

use crate::{
    embedding_similarity, AnalysisCache, AnalysisCacheError, ImageEmbedding, InferenceTask,
    PhotoGroup,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CullingCandidate {
    pub asset_id: Uuid,
    pub score: CullingScore,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CullingRecommendation {
    pub asset_id: Uuid,
    pub quality_score: f32,
    pub decision: CullingDecision,
    pub group_rank: usize,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupCullingResult {
    pub group_id: Uuid,
    pub recommendations: Vec<CullingRecommendation>,
    pub pending_asset_ids: Vec<Uuid>,
}

#[derive(Debug, Error)]
pub enum CullingEvidenceError {
    #[error(transparent)]
    Analysis(#[from] AnalysisCacheError),
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
    }

    Ok(GroupCullingResult {
        group_id: group.id,
        recommendations: rank_group_candidates_with_embeddings(
            &candidates,
            &embeddings,
            duplicate_threshold,
        ),
        pending_asset_ids,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AnalysisArtifact, AnalysisCacheKey, GroupingBasis, PhotoGroupKind};
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
