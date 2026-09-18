//! Smart culling foundation for photographer workflow.
//!
//! This module does not delete photos. It provides analysis results that can
//! help users review large RAW collections before editing.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CullingScore {
    pub sharpness: f32,
    pub exposure: f32,
    pub expression: f32,
    pub duplicate_similarity: f32,
}

impl CullingScore {
    pub fn review_score(&self) -> f32 {
        (self.sharpness + self.exposure + self.expression) / 3.0
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CullingDecision {
    Keep,
    Review,
    RejectSuggestion,
}

pub fn suggest_decision(score: &CullingScore) -> CullingDecision {
    if score.review_score() >= 0.85 {
        CullingDecision::Keep
    } else if score.review_score() >= 0.55 {
        CullingDecision::Review
    } else {
        CullingDecision::RejectSuggestion
    }
}
