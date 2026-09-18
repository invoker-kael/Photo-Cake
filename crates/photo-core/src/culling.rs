//! Smart culling foundation for photographer workflow.
//!
//! This module provides non-destructive review suggestions for RAW collections.
//! It never deletes originals automatically.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CullingScore {
    pub sharpness: f32,
    pub blur_penalty: f32,
    pub exposure: f32,
    pub expression: f32,
    pub duplicate_similarity: f32,
    pub composition: f32,
}

impl CullingScore {
    pub fn review_score(&self) -> f32 {
        let technical = self.sharpness * (1.0 - self.blur_penalty.clamp(0.0, 1.0));
        ((technical + self.exposure + self.expression + self.composition) / 4.0)
            .clamp(0.0, 1.0)
    }

    pub fn is_burst_duplicate(&self) -> bool {
        self.duplicate_similarity >= 0.95
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
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

#[cfg(test)]
mod tests {
    use super::*;

    fn score(quality: f32, duplicate_similarity: f32) -> CullingScore {
        CullingScore {
            sharpness: quality,
            blur_penalty: 0.0,
            exposure: quality,
            expression: quality,
            duplicate_similarity,
            composition: quality,
        }
    }

    #[test]
    fn low_quality_frame_is_only_a_reject_suggestion() {
        assert_eq!(
            suggest_decision(&score(0.2, 0.0)),
            CullingDecision::RejectSuggestion
        );
    }

    #[test]
    fn burst_ranking_keeps_best_candidate_and_preserves_alternatives_for_review() {
        let weak = Uuid::new_v4();
        let best = Uuid::new_v4();
        let middle = Uuid::new_v4();
        let ranked = rank_group_candidates(&[
            CullingCandidate {
                asset_id: weak,
                score: score(0.68, 0.98),
            },
            CullingCandidate {
                asset_id: best,
                score: score(0.94, 0.99),
            },
            CullingCandidate {
                asset_id: middle,
                score: score(0.82, 0.97),
            },
        ]);

        assert_eq!(ranked[0].asset_id, best);
        assert_eq!(ranked[0].decision, CullingDecision::Keep);
        assert_eq!(ranked[1].decision, CullingDecision::Review);
        assert_eq!(ranked[2].decision, CullingDecision::Review);
    }
}
