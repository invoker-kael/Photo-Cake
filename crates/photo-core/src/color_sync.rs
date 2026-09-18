use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GroupSyncMode {
    AutoGroup,
    ReferenceDriven,
    ManualCopy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SemanticRegion {
    Person,
    FaceSkin,
    Background,
    Sky,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticColorIntent {
    pub region: SemanticRegion,
    pub exposure_delta_ev: f32,
    pub saturation_delta: f32,
    pub warmth_delta: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupColorIntent {
    pub name: String,
    pub target_exposure_ev: f32,
    pub target_temperature_k: f32,
    pub target_tint: f32,
    pub contrast: f32,
    pub saturation: f32,
    pub semantic: Vec<SemanticColorIntent>,
}

impl Default for GroupColorIntent {
    fn default() -> Self {
        Self {
            name: "Balanced".to_string(),
            target_exposure_ev: 0.0,
            target_temperature_k: 5500.0,
            target_tint: 0.0,
            contrast: 0.0,
            saturation: 0.0,
            semantic: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhotoColorAnalysis {
    pub asset_id: Uuid,
    pub exposure_ev: f32,
    pub temperature_k: f32,
    pub tint: f32,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedColorEdit {
    pub asset_id: Uuid,
    pub exposure_delta_ev: f32,
    pub temperature_delta_k: f32,
    pub tint_delta: f32,
    pub contrast: f32,
    pub saturation: f32,
    pub semantic: Vec<SemanticColorIntent>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupColorSyncPlan {
    pub group_id: Uuid,
    pub mode: GroupSyncMode,
    pub reference_asset_id: Option<Uuid>,
    pub intent: GroupColorIntent,
    pub revision: u64,
    pub resolved: Vec<ResolvedColorEdit>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GroupColorInvalidationScope {
    GroupColorOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupColorInvalidation {
    pub group_id: Uuid,
    pub scope: GroupColorInvalidationScope,
    pub previous_revision: u64,
    pub next_revision: u64,
}

#[derive(Debug, Error)]
pub enum ColorSyncError {
    #[error("reference asset is not part of the group")]
    ReferenceOutsideGroup,
    #[error("reference-driven sync requires a reference asset")]
    MissingReference,
    #[error("manual copy mode requires a source edit")]
    MissingManualCopyEdit,
    #[error("analysis missing for group asset {0}")]
    MissingAnalysis(Uuid),
    #[error("cannot derive automatic group intent without analyses")]
    EmptyAnalysis,
}

pub fn derive_auto_group_intent(
    analyses: &[PhotoColorAnalysis],
) -> Result<GroupColorIntent, ColorSyncError> {
    if analyses.is_empty() {
        return Err(ColorSyncError::EmptyAnalysis);
    }
    Ok(GroupColorIntent {
        name: "Auto Balanced".to_string(),
        target_exposure_ev: median(analyses.iter().map(|a| a.exposure_ev)),
        target_temperature_k: median(analyses.iter().map(|a| a.temperature_k)),
        target_tint: median(analyses.iter().map(|a| a.tint)),
        contrast: 0.0,
        saturation: 0.0,
        semantic: Vec::new(),
    })
}

pub fn choose_reference_candidate(
    analyses: &[PhotoColorAnalysis],
    intent: &GroupColorIntent,
) -> Option<Uuid> {
    analyses
        .iter()
        .min_by(|left, right| {
            reference_score(left, intent).total_cmp(&reference_score(right, intent))
        })
        .map(|analysis| analysis.asset_id)
}

pub fn build_auto_group_plan(
    group_id: Uuid,
    asset_ids: &[Uuid],
    analyses: &[PhotoColorAnalysis],
    revision: u64,
) -> Result<GroupColorSyncPlan, ColorSyncError> {
    let intent = derive_auto_group_intent(analyses)?;
    let reference_asset_id = choose_reference_candidate(analyses, &intent);
    build_adaptive_group_plan(
        group_id,
        asset_ids,
        GroupSyncMode::AutoGroup,
        reference_asset_id,
        intent,
        analyses,
        None,
        revision,
    )
}

pub fn build_adaptive_group_plan(
    group_id: Uuid,
    asset_ids: &[Uuid],
    mode: GroupSyncMode,
    reference_asset_id: Option<Uuid>,
    intent: GroupColorIntent,
    analyses: &[PhotoColorAnalysis],
    manual_copy_edit: Option<&ResolvedColorEdit>,
    revision: u64,
) -> Result<GroupColorSyncPlan, ColorSyncError> {
    // Reference-driven style may come from an external edited photo or another
    // group. Only an explicit in-group promotion requires membership.
    if mode == GroupSyncMode::ReferenceDriven && reference_asset_id.is_none() {
        return Err(ColorSyncError::MissingReference);
    }
    if mode == GroupSyncMode::ManualCopy && manual_copy_edit.is_none() {
        return Err(ColorSyncError::MissingManualCopyEdit);
    }

    let mut resolved = Vec::with_capacity(asset_ids.len());
    for asset_id in asset_ids {
        let edit = match mode {
            GroupSyncMode::ManualCopy => {
                let source = manual_copy_edit.expect("validated above");
                ResolvedColorEdit {
                    asset_id: *asset_id,
                    exposure_delta_ev: source.exposure_delta_ev,
                    temperature_delta_k: source.temperature_delta_k,
                    tint_delta: source.tint_delta,
                    contrast: source.contrast,
                    saturation: source.saturation,
                    semantic: source.semantic.clone(),
                }
            }
            GroupSyncMode::AutoGroup | GroupSyncMode::ReferenceDriven => {
                let analysis = analyses
                    .iter()
                    .find(|analysis| analysis.asset_id == *asset_id)
                    .ok_or(ColorSyncError::MissingAnalysis(*asset_id))?;
                resolve_adaptive_edit(analysis, &intent)
            }
        };
        resolved.push(edit);
    }

    Ok(GroupColorSyncPlan {
        group_id,
        mode,
        reference_asset_id,
        intent,
        revision,
        resolved,
    })
}

pub fn promote_group_reference(
    plan: &mut GroupColorSyncPlan,
    asset_ids: &[Uuid],
    reference_asset_id: Uuid,
    reference_target: GroupColorIntent,
    analyses: &[PhotoColorAnalysis],
) -> Result<GroupColorInvalidation, ColorSyncError> {
    if !asset_ids.contains(&reference_asset_id) {
        return Err(ColorSyncError::ReferenceOutsideGroup);
    }

    let previous_revision = plan.revision;
    let next = build_adaptive_group_plan(
        plan.group_id,
        asset_ids,
        GroupSyncMode::ReferenceDriven,
        Some(reference_asset_id),
        reference_target,
        analyses,
        None,
        previous_revision.saturating_add(1),
    )?;
    *plan = next;

    Ok(GroupColorInvalidation {
        group_id: plan.group_id,
        scope: GroupColorInvalidationScope::GroupColorOnly,
        previous_revision,
        next_revision: plan.revision,
    })
}

fn resolve_adaptive_edit(
    analysis: &PhotoColorAnalysis,
    intent: &GroupColorIntent,
) -> ResolvedColorEdit {
    ResolvedColorEdit {
        asset_id: analysis.asset_id,
        exposure_delta_ev: clamp(intent.target_exposure_ev - analysis.exposure_ev, -4.0, 4.0),
        temperature_delta_k: clamp(
            intent.target_temperature_k - analysis.temperature_k,
            -4000.0,
            4000.0,
        ),
        tint_delta: clamp(intent.target_tint - analysis.tint, -150.0, 150.0),
        contrast: intent.contrast,
        saturation: intent.saturation,
        semantic: intent.semantic.clone(),
    }
}

fn reference_score(analysis: &PhotoColorAnalysis, intent: &GroupColorIntent) -> f32 {
    (analysis.exposure_ev - intent.target_exposure_ev).abs() * 2.0
        + (analysis.temperature_k - intent.target_temperature_k).abs() / 2000.0
        + (analysis.tint - intent.target_tint).abs() / 50.0
        + (1.0 - analysis.confidence.clamp(0.0, 1.0)) * 0.5
}

fn median(values: impl Iterator<Item = f32>) -> f32 {
    let mut values = values.collect::<Vec<_>>();
    values.sort_by(|left, right| left.total_cmp(right));
    let middle = values.len() / 2;
    if values.len() % 2 == 0 {
        (values[middle - 1] + values[middle]) / 2.0
    } else {
        values[middle]
    }
}

fn clamp(value: f32, min: f32, max: f32) -> f32 {
    value.max(min).min(max)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn analysis(asset_id: Uuid, exposure_ev: f32, temperature_k: f32) -> PhotoColorAnalysis {
        PhotoColorAnalysis {
            asset_id,
            exposure_ev,
            temperature_k,
            tint: 0.0,
            confidence: 1.0,
        }
    }

    #[test]
    fn auto_mode_derives_target_and_reference_without_manual_grade() {
        let first = Uuid::new_v4();
        let middle = Uuid::new_v4();
        let last = Uuid::new_v4();
        let analyses = vec![
            analysis(first, -1.0, 5000.0),
            analysis(middle, 0.1, 5500.0),
            analysis(last, 1.0, 6200.0),
        ];
        let plan = build_auto_group_plan(
            Uuid::new_v4(),
            &[first, middle, last],
            &analyses,
            1,
        )
        .unwrap();
        assert_eq!(plan.mode, GroupSyncMode::AutoGroup);
        assert_eq!(plan.reference_asset_id, Some(middle));
        assert_eq!(plan.intent.target_exposure_ev, 0.1);
        assert_eq!(plan.intent.target_temperature_k, 5500.0);
    }

    #[test]
    fn adaptive_sync_resolves_different_parameters_for_each_photo() {
        let group_id = Uuid::new_v4();
        let dark = Uuid::new_v4();
        let bright = Uuid::new_v4();
        let intent = GroupColorIntent {
            target_exposure_ev: 0.25,
            target_temperature_k: 5600.0,
            ..GroupColorIntent::default()
        };
        let plan = build_adaptive_group_plan(
            group_id,
            &[dark, bright],
            GroupSyncMode::AutoGroup,
            None,
            intent,
            &[
                analysis(dark, -1.0, 5000.0),
                analysis(bright, 0.8, 6000.0),
            ],
            None,
            1,
        )
        .unwrap();

        assert_eq!(plan.resolved.len(), 2);
        assert_ne!(
            plan.resolved[0].exposure_delta_ev,
            plan.resolved[1].exposure_delta_ev
        );
        assert_ne!(
            plan.resolved[0].temperature_delta_k,
            plan.resolved[1].temperature_delta_k
        );
    }

    #[test]
    fn external_reference_can_drive_another_group() {
        let external_reference = Uuid::new_v4();
        let target = Uuid::new_v4();
        let plan = build_adaptive_group_plan(
            Uuid::new_v4(),
            &[target],
            GroupSyncMode::ReferenceDriven,
            Some(external_reference),
            GroupColorIntent {
                name: "External look".into(),
                target_exposure_ev: 0.2,
                target_temperature_k: 5800.0,
                ..GroupColorIntent::default()
            },
            &[analysis(target, -0.5, 5200.0)],
            None,
            1,
        )
        .unwrap();

        assert_eq!(plan.reference_asset_id, Some(external_reference));
        assert_eq!(plan.resolved[0].asset_id, target);
        assert!((plan.resolved[0].exposure_delta_ev - 0.7).abs() < 1e-6);
    }

    #[test]
    fn promoting_reference_only_invalidates_group_color_revision() {
        let group_id = Uuid::new_v4();
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let analyses = vec![analysis(first, -0.4, 5200.0), analysis(second, 0.5, 5900.0)];
        let mut plan = build_adaptive_group_plan(
            group_id,
            &[first, second],
            GroupSyncMode::AutoGroup,
            None,
            GroupColorIntent::default(),
            &analyses,
            None,
            3,
        )
        .unwrap();

        let invalidation = promote_group_reference(
            &mut plan,
            &[first, second],
            second,
            GroupColorIntent {
                name: "My reference".to_string(),
                target_exposure_ev: 0.2,
                target_temperature_k: 5750.0,
                ..GroupColorIntent::default()
            },
            &analyses,
        )
        .unwrap();

        assert_eq!(plan.mode, GroupSyncMode::ReferenceDriven);
        assert_eq!(plan.reference_asset_id, Some(second));
        assert_eq!(invalidation.scope, GroupColorInvalidationScope::GroupColorOnly);
        assert_eq!(invalidation.previous_revision, 3);
        assert_eq!(invalidation.next_revision, 4);
    }

    #[test]
    fn manual_copy_is_explicit_and_copies_exact_parameters() {
        let group_id = Uuid::new_v4();
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let source = ResolvedColorEdit {
            asset_id: first,
            exposure_delta_ev: 0.4,
            temperature_delta_k: -200.0,
            tint_delta: 2.0,
            contrast: 10.0,
            saturation: 4.0,
            semantic: Vec::new(),
        };
        let plan = build_adaptive_group_plan(
            group_id,
            &[first, second],
            GroupSyncMode::ManualCopy,
            Some(first),
            GroupColorIntent::default(),
            &[],
            Some(&source),
            1,
        )
        .unwrap();

        assert_eq!(plan.resolved[0].exposure_delta_ev, 0.4);
        assert_eq!(plan.resolved[1].exposure_delta_ev, 0.4);
        assert_eq!(plan.resolved[0].temperature_delta_k, -200.0);
        assert_eq!(plan.resolved[1].temperature_delta_k, -200.0);
    }
}
