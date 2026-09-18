//! Smart culling foundation for photographer workflow.
//!
//! This module provides non-destructive review suggestions for RAW collections.
//! It never deletes originals automatically.

use serde::{Deserialize, Serialize};

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
        let technical = self.sharpness * (1.0 - self.blur_penalty);
        (technical + self.exposure + self.expression + self.composition) / 4.0
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
