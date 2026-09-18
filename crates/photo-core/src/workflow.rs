use crate::culling::CullingDecision;
use crate::culling_store::CullingUserDecision;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkflowFocus {
    Prepare,
    Cull,
    Reference,
    Review,
    Lightroom,
    Complete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WorkflowFacts {
    pub preparation_active: usize,
    pub preparation_failed: usize,
    pub cull_attention: usize,
    pub cull_pending: usize,
    pub groups_total: usize,
    pub reference_attention_groups: usize,
    pub review_attention: usize,
    pub review_pending_groups: usize,
    pub lightroom_conflict_groups: usize,
    pub lightroom_missing_sidecars: usize,
    pub lightroom_unresolved_groups: usize,
    pub lightroom_current_groups: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowStatus {
    pub next_focus: WorkflowFocus,
    pub facts: WorkflowFacts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RecipeReviewSignal {
    pub confirmed: bool,
    pub has_exception: bool,
    pub user_decision: Option<CullingUserDecision>,
    pub ai_decision: Option<CullingDecision>,
    pub evidence_pending: bool,
}

pub fn recipe_review_requires_attention(signal: &RecipeReviewSignal) -> bool {
    if signal.confirmed {
        return false;
    }
    if signal.has_exception {
        return true;
    }
    match signal.user_decision {
        Some(CullingUserDecision::Review) => return true,
        Some(CullingUserDecision::Keep | CullingUserDecision::Reject) => return false,
        None => {}
    }
    matches!(
        signal.ai_decision,
        Some(CullingDecision::Review | CullingDecision::RejectSuggestion)
    )
}

pub fn recipe_review_group_can_confirm(signals: &[RecipeReviewSignal]) -> bool {
    signals.iter().any(|signal| !signal.confirmed)
        && signals.iter().all(|signal| {
            signal.confirmed
                || (!signal.evidence_pending && !recipe_review_requires_attention(signal))
        })
}

pub fn derive_workflow_status(facts: WorkflowFacts) -> WorkflowStatus {
    let next_focus = if facts.preparation_failed > 0 || facts.preparation_active > 0 {
        WorkflowFocus::Prepare
    } else if facts.cull_pending > 0 || facts.cull_attention > 0 {
        WorkflowFocus::Cull
    } else if facts.reference_attention_groups > 0 {
        WorkflowFocus::Reference
    } else if facts.review_pending_groups > 0 || facts.review_attention > 0 {
        WorkflowFocus::Review
    } else if facts.lightroom_conflict_groups > 0
        || facts.lightroom_unresolved_groups > 0
        || facts.lightroom_missing_sidecars > 0
    {
        WorkflowFocus::Lightroom
    } else {
        WorkflowFocus::Complete
    };

    WorkflowStatus { next_focus, facts }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> WorkflowFacts {
        WorkflowFacts {
            groups_total: 4,
            lightroom_current_groups: 4,
            ..WorkflowFacts::default()
        }
    }

    #[test]
    fn preparation_has_highest_attention_priority() {
        let mut value = facts();
        value.preparation_active = 2;
        value.cull_attention = 8;
        value.reference_attention_groups = 3;
        assert_eq!(
            derive_workflow_status(value).next_focus,
            WorkflowFocus::Prepare
        );
    }

    #[test]
    fn cull_precedes_reference_when_analysis_is_ready() {
        let mut value = facts();
        value.cull_attention = 5;
        value.reference_attention_groups = 2;
        assert_eq!(derive_workflow_status(value).next_focus, WorkflowFocus::Cull);
    }

    #[test]
    fn reference_precedes_recipe_review() {
        let mut value = facts();
        value.reference_attention_groups = 2;
        value.review_attention = 9;
        assert_eq!(
            derive_workflow_status(value).next_focus,
            WorkflowFocus::Reference
        );
    }

    #[test]
    fn recipe_review_precedes_delivery() {
        let mut value = facts();
        value.review_attention = 3;
        value.lightroom_missing_sidecars = 20;
        assert_eq!(
            derive_workflow_status(value).next_focus,
            WorkflowFocus::Review
        );
    }

    #[test]
    fn delivery_is_next_when_decisions_are_clear() {
        let mut value = facts();
        value.lightroom_current_groups = 1;
        value.lightroom_missing_sidecars = 7;
        assert_eq!(
            derive_workflow_status(value).next_focus,
            WorkflowFocus::Lightroom
        );
    }

    #[test]
    fn complete_requires_no_remaining_attention() {
        assert_eq!(
            derive_workflow_status(facts()).next_focus,
            WorkflowFocus::Complete
        );
    }

    #[test]
    fn recipe_attention_prefers_photographer_decisions_over_ai() {
        let mut signal = RecipeReviewSignal {
            ai_decision: Some(CullingDecision::RejectSuggestion),
            ..RecipeReviewSignal::default()
        };
        assert!(recipe_review_requires_attention(&signal));
        signal.user_decision = Some(CullingUserDecision::Keep);
        assert!(!recipe_review_requires_attention(&signal));
        signal.user_decision = Some(CullingUserDecision::Review);
        assert!(recipe_review_requires_attention(&signal));
    }

    #[test]
    fn saved_exception_requires_attention_until_current_recipe_is_confirmed() {
        let mut signal = RecipeReviewSignal {
            has_exception: true,
            ..RecipeReviewSignal::default()
        };
        assert!(recipe_review_requires_attention(&signal));
        signal.confirmed = true;
        assert!(!recipe_review_requires_attention(&signal));
    }

    #[test]
    fn clear_group_confirmation_requires_complete_non_attention_recipes() {
        let clear = RecipeReviewSignal {
            ai_decision: Some(CullingDecision::Keep),
            ..RecipeReviewSignal::default()
        };
        assert!(recipe_review_group_can_confirm(&[clear]));
        assert!(!recipe_review_group_can_confirm(&[
            clear,
            RecipeReviewSignal { evidence_pending: true, ..clear }
        ]));
        assert!(!recipe_review_group_can_confirm(&[
            clear,
            RecipeReviewSignal {
                ai_decision: Some(CullingDecision::Review),
                ..clear
            }
        ]));
    }

    #[test]
    fn already_confirmed_group_is_not_offered_as_new_batch_work() {
        let confirmed = RecipeReviewSignal {
            confirmed: true,
            has_exception: true,
            ai_decision: Some(CullingDecision::RejectSuggestion),
            evidence_pending: true,
            ..RecipeReviewSignal::default()
        };
        assert!(!recipe_review_group_can_confirm(&[confirmed]));
    }
}
