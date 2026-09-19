use crate::culling::CullingDecision;
use crate::culling_store::CullingUserDecision;
use crate::classification::SceneTag;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
    pub lightroom_hdr_merge_groups: usize,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RecipeReviewAttentionReason {
    SavedException,
    PhotographerReview,
    AiRejectSuggestion,
    AiReview,
    EvidencePending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RecipeReviewDisposition {
    Confirmed,
    NeedsReview,
    Clear,
    Pending,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipeReviewAssetPreflight {
    pub asset_id: Uuid,
    pub disposition: RecipeReviewDisposition,
    pub reason: Option<RecipeReviewAttentionReason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipeReviewGroupPreflight {
    pub group_id: Uuid,
    #[serde(default)]
    pub assets: Vec<RecipeReviewAssetPreflight>,
    #[serde(default)]
    pub attention_asset_ids: Vec<Uuid>,
    #[serde(default)]
    pub clear_asset_ids: Vec<Uuid>,
    #[serde(default)]
    pub confirmed_asset_ids: Vec<Uuid>,
    #[serde(default)]
    pub pending_asset_ids: Vec<Uuid>,
    pub can_confirm_clear_group: bool,
    pub contains_people: bool,
    #[serde(default)]
    pub scene_tags: Vec<SceneTag>,
    #[serde(default)]
    pub hdr_source_asset_ids: Vec<Uuid>,
}

pub fn recipe_review_attention_reason(
    signal: &RecipeReviewSignal,
) -> Option<RecipeReviewAttentionReason> {
    if signal.confirmed {
        return None;
    }
    if signal.has_exception {
        return Some(RecipeReviewAttentionReason::SavedException);
    }
    match signal.user_decision {
        Some(CullingUserDecision::Review) => {
            return Some(RecipeReviewAttentionReason::PhotographerReview)
        }
        Some(CullingUserDecision::Keep | CullingUserDecision::Reject) => return None,
        None => {}
    }
    if signal.evidence_pending {
        return Some(RecipeReviewAttentionReason::EvidencePending);
    }
    match signal.ai_decision {
        Some(CullingDecision::RejectSuggestion) => {
            Some(RecipeReviewAttentionReason::AiRejectSuggestion)
        }
        Some(CullingDecision::Review) => Some(RecipeReviewAttentionReason::AiReview),
        _ => None,
    }
}

pub fn build_recipe_review_group_preflight(
    group_id: Uuid,
    signals: &[(Uuid, RecipeReviewSignal)],
    contains_people: bool,
    scene_tags: Vec<SceneTag>,
    hdr_source_asset_ids: Vec<Uuid>,
) -> RecipeReviewGroupPreflight {
    let mut assets = Vec::with_capacity(signals.len());
    let mut attention_asset_ids = Vec::new();
    let mut clear_asset_ids = Vec::new();
    let mut confirmed_asset_ids = Vec::new();
    let mut pending_asset_ids = Vec::new();

    for (asset_id, signal) in signals {
        let reason = recipe_review_attention_reason(signal);
        let disposition = if signal.confirmed {
            confirmed_asset_ids.push(*asset_id);
            RecipeReviewDisposition::Confirmed
        } else if matches!(reason, Some(RecipeReviewAttentionReason::EvidencePending)) {
            pending_asset_ids.push(*asset_id);
            RecipeReviewDisposition::Pending
        } else if reason.is_some() {
            attention_asset_ids.push(*asset_id);
            RecipeReviewDisposition::NeedsReview
        } else {
            clear_asset_ids.push(*asset_id);
            RecipeReviewDisposition::Clear
        };
        assets.push(RecipeReviewAssetPreflight {
            asset_id: *asset_id,
            disposition,
            reason,
        });
    }

    let can_confirm_clear_group = !clear_asset_ids.is_empty()
        && attention_asset_ids.is_empty()
        && pending_asset_ids.is_empty();

    RecipeReviewGroupPreflight {
        group_id,
        assets,
        attention_asset_ids,
        clear_asset_ids,
        confirmed_asset_ids,
        pending_asset_ids,
        can_confirm_clear_group,
        contains_people,
        scene_tags,
        hdr_source_asset_ids,
    }
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
        || facts.lightroom_hdr_merge_groups > 0
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
    fn hdr_merge_routes_to_lightroom_without_fake_recipe_completion() {
        let mut value = facts();
        value.lightroom_current_groups = 3;
        value.lightroom_hdr_merge_groups = 1;
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
    fn recipe_preflight_prioritizes_exception_and_blocks_clear_group() {
        let exception_id = Uuid::new_v4();
        let clear_id = Uuid::new_v4();
        let plan = build_recipe_review_group_preflight(
            Uuid::new_v4(),
            &[
                (
                    exception_id,
                    RecipeReviewSignal {
                        has_exception: true,
                        ai_decision: Some(CullingDecision::Keep),
                        ..RecipeReviewSignal::default()
                    },
                ),
                (
                    clear_id,
                    RecipeReviewSignal {
                        ai_decision: Some(CullingDecision::Keep),
                        ..RecipeReviewSignal::default()
                    },
                ),
            ],
            false,
            vec![SceneTag::Landscape],
            Vec::new(),
        );
        assert_eq!(plan.attention_asset_ids, vec![exception_id]);
        assert_eq!(plan.clear_asset_ids, vec![clear_id]);
        assert!(!plan.can_confirm_clear_group);
        assert_eq!(plan.scene_tags, vec![SceneTag::Landscape]);
    }

    #[test]
    fn pending_evidence_is_not_silently_batch_confirmed() {
        let pending_id = Uuid::new_v4();
        let plan = build_recipe_review_group_preflight(
            Uuid::new_v4(),
            &[(
                pending_id,
                RecipeReviewSignal {
                    evidence_pending: true,
                    ..RecipeReviewSignal::default()
                },
            )],
            false,
            Vec::new(),
            Vec::new(),
        );
        assert_eq!(plan.pending_asset_ids, vec![pending_id]);
        assert!(!plan.can_confirm_clear_group);
        assert_eq!(
            plan.assets[0].reason,
            Some(RecipeReviewAttentionReason::EvidencePending)
        );
    }

    #[test]
    fn fully_clear_group_is_batch_confirmable_and_keeps_hdr_context_separate() {
        let clear_id = Uuid::new_v4();
        let hdr_id = Uuid::new_v4();
        let plan = build_recipe_review_group_preflight(
            Uuid::new_v4(),
            &[(
                clear_id,
                RecipeReviewSignal {
                    ai_decision: Some(CullingDecision::Keep),
                    ..RecipeReviewSignal::default()
                },
            )],
            true,
            vec![SceneTag::Night],
            vec![hdr_id],
        );
        assert!(plan.can_confirm_clear_group);
        assert_eq!(plan.clear_asset_ids, vec![clear_id]);
        assert_eq!(plan.hdr_source_asset_ids, vec![hdr_id]);
        assert!(plan.contains_people);
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
