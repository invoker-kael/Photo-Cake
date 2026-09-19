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
    #[serde(default)]
    pub target_temperature_k: Option<f32>,
    #[serde(default)]
    pub target_tint: Option<f32>,
    pub contrast: f32,
    pub saturation: f32,
    pub semantic: Vec<SemanticColorIntent>,
}

impl GroupColorIntent {
    pub fn white_balance(&self) -> Option<(f32, f32)> {
        self.target_temperature_k.zip(self.target_tint)
    }
}

impl Default for GroupColorIntent {
    fn default() -> Self {
        Self {
            name: "Balanced".to_string(),
            target_exposure_ev: 0.0,
            target_temperature_k: None,
            target_tint: None,
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
    #[serde(default)]
    pub temperature_k: Option<f32>,
    #[serde(default)]
    pub tint: Option<f32>,
    pub confidence: f32,
}

impl PhotoColorAnalysis {
    pub fn white_balance(&self) -> Option<(f32, f32)> {
        self.temperature_k.zip(self.tint)
    }
}

/// Preview-derived exposure evidence used for relative photographic matching.
///
/// Percentiles and clipping ratios come from the embedded RAW preview and are
/// deliberately treated as relative tone evidence, not as sensor-linear RAW data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhotoExposureAnalysis {
    pub asset_id: Uuid,
    pub exposure_ev: f32,
    #[serde(default)]
    pub temperature_k: Option<f32>,
    #[serde(default)]
    pub tint: Option<f32>,
    pub confidence: f32,
    #[serde(default)]
    pub luminance_p02: Option<f32>,
    #[serde(default)]
    pub luminance_p10: Option<f32>,
    #[serde(default)]
    pub luminance_p50: Option<f32>,
    #[serde(default)]
    pub luminance_p90: Option<f32>,
    #[serde(default)]
    pub luminance_p98: Option<f32>,
    #[serde(default)]
    pub shadow_clip_ratio: Option<f32>,
    #[serde(default)]
    pub highlight_clip_ratio: Option<f32>,
    #[serde(default)]
    pub colorfulness: Option<f32>,
    #[serde(default)]
    pub colorfulness_p25: Option<f32>,
    #[serde(default)]
    pub colorfulness_p75: Option<f32>,
}

impl PhotoExposureAnalysis {
    pub fn color_analysis(&self) -> PhotoColorAnalysis {
        PhotoColorAnalysis {
            asset_id: self.asset_id,
            exposure_ev: self.exposure_ev,
            temperature_k: self.temperature_k,
            tint: self.tint,
            confidence: self.confidence,
        }
    }

    pub fn tone_percentiles(&self) -> Option<(f32, f32)> {
        self.luminance_p10.zip(self.luminance_p90)
    }

    pub fn endpoint_percentiles(&self) -> Option<(f32, f32)> {
        self.luminance_p02.zip(self.luminance_p98)
    }
}

/// Resolve highlight/shadow corrections after the normal per-photo exposure
/// correction so the target follows the photographer-selected Reference's
/// tonal distribution without blindly copying numeric settings.
pub fn reference_relative_tone_adjustments(
    reference: &PhotoExposureAnalysis,
    target: &PhotoExposureAnalysis,
    exposure_delta_ev: f32,
) -> Option<(f32, f32)> {
    let (reference_shadow, reference_highlight) = reference.tone_percentiles()?;
    let (target_shadow, target_highlight) = target.tone_percentiles()?;
    let exposure_factor = 2.0_f32.powf(exposure_delta_ev.clamp(-5.0, 5.0));
    let projected_shadow = (target_shadow * exposure_factor).clamp(0.0, 1.0);
    let projected_highlight = (target_highlight * exposure_factor).clamp(0.0, 1.0);
    let confidence = reference
        .confidence
        .min(target.confidence)
        .clamp(0.2, 1.0);

    let reference_shadow_clip = reference.shadow_clip_ratio.unwrap_or(0.0);
    let target_shadow_clip = target.shadow_clip_ratio.unwrap_or(0.0);
    let reference_highlight_clip = reference.highlight_clip_ratio.unwrap_or(0.0);
    let target_highlight_clip = target.highlight_clip_ratio.unwrap_or(0.0);

    let excess_shadow_clip = (target_shadow_clip - reference_shadow_clip).max(0.0);
    let excess_highlight_clip = (target_highlight_clip - reference_highlight_clip).max(0.0);

    let mut shadows =
        (reference_shadow - projected_shadow) * 180.0 * confidence
        + excess_shadow_clip * 400.0 * confidence;
    let mut highlights =
        (reference_highlight - projected_highlight) * 180.0 * confidence
        - excess_highlight_clip * 400.0 * confidence;

    // Opening extremely dark preview shadows aggressively is a poor default:
    // embedded JPEGs do not tell us how much clean RAW shadow detail is really
    // recoverable. Keep the direction of the Reference match, but preserve a
    // stronger black anchor as projected P10 approaches black or clipping rises.
    if shadows > 0.0 {
        let dark_shadow_guard =
            (0.55 + ((projected_shadow - 0.02) / 0.10).clamp(0.0, 1.0) * 0.45)
                .clamp(0.55, 1.0);
        let clip_guard = (1.0 - target_shadow_clip * 8.0).clamp(0.55, 1.0);
        shadows *= dark_shadow_guard.min(clip_guard);
    }

    // Preserve naturally wide dynamic range. Simultaneously lifting shadows and
    // pulling highlights is useful up to a point, but excessive opposing moves
    // create the flat "HDR" look that hurts night, landscape and architecture.
    let target_span = (projected_highlight - projected_shadow).max(0.0);
    let opposing_compression = shadows.max(0.0) + (-highlights).max(0.0);
    if target_span > 0.70 && opposing_compression > 0.0 {
        let wide_range = ((target_span - 0.70) / 0.22).clamp(0.0, 1.0);
        let max_compression = 85.0 - wide_range * 20.0;
        if opposing_compression > max_compression {
            let scale = max_compression / opposing_compression;
            shadows *= scale;
            highlights *= scale;
        }
    }

    if shadows.abs() < 2.0 {
        shadows = 0.0;
    }
    if highlights.abs() < 2.0 {
        highlights = 0.0;
    }

    Some((
        highlights.clamp(-70.0, 70.0),
        shadows.clamp(-70.0, 70.0),
    ))
}

/// Resolve white/black endpoint corrections independently from the broader
/// Highlights/Shadows match. P02/P98 target the tonal endpoints while P10/P90
/// remain responsible for the wider shadow/highlight regions.
pub fn reference_relative_endpoint_adjustments(
    reference: &PhotoExposureAnalysis,
    target: &PhotoExposureAnalysis,
    exposure_delta_ev: f32,
) -> Option<(f32, f32)> {
    let (reference_black, reference_white) = reference.endpoint_percentiles()?;
    let (target_black, target_white) = target.endpoint_percentiles()?;
    let exposure_factor = 2.0_f32.powf(exposure_delta_ev.clamp(-5.0, 5.0));
    let projected_black = (target_black * exposure_factor).clamp(0.0, 1.0);
    let projected_white = (target_white * exposure_factor).clamp(0.0, 1.0);
    let confidence = reference
        .confidence
        .min(target.confidence)
        .clamp(0.2, 1.0);

    let excess_shadow_clip = (
        target.shadow_clip_ratio.unwrap_or(0.0)
            - reference.shadow_clip_ratio.unwrap_or(0.0)
    )
    .max(0.0);
    let excess_highlight_clip = (
        target.highlight_clip_ratio.unwrap_or(0.0)
            - reference.highlight_clip_ratio.unwrap_or(0.0)
    )
    .max(0.0);

    let mut blacks =
        (reference_black - projected_black) * 220.0 * confidence
        + excess_shadow_clip * 350.0 * confidence;
    let mut whites =
        (reference_white - projected_white) * 220.0 * confidence
        - excess_highlight_clip * 350.0 * confidence;

    // Positive Blacks can quickly wash out night scenes. Retain more of the
    // target's deep-black anchor when P10/P02 are already near black.
    if blacks > 0.0 {
        let projected_p10 = target
            .luminance_p10
            .map(|value| (value * exposure_factor).clamp(0.0, 1.0))
            .unwrap_or(projected_black);
        let black_anchor_guard =
            (0.45 + ((projected_p10 - 0.015) / 0.08).clamp(0.0, 1.0) * 0.55)
                .clamp(0.45, 1.0);
        let clip_guard =
            (1.0 - target.shadow_clip_ratio.unwrap_or(0.0) * 10.0).clamp(0.45, 1.0);
        blacks *= black_anchor_guard.min(clip_guard);
    }

    // Do not push Whites hard when the target is already near its preview
    // endpoint. This protects bright clouds, snow, speculars and saturated skies
    // from unnecessary additional endpoint pressure.
    if whites > 0.0 {
        let endpoint_headroom = (1.0 - projected_white).clamp(0.0, 1.0);
        let headroom_guard = (endpoint_headroom / 0.08).clamp(0.35, 1.0);
        let clip_guard =
            (1.0 - target.highlight_clip_ratio.unwrap_or(0.0) * 12.0).clamp(0.35, 1.0);
        whites *= headroom_guard.min(clip_guard);
    }

    if blacks.abs() < 1.5 {
        blacks = 0.0;
    }
    if whites.abs() < 1.5 {
        whites = 0.0;
    }

    Some((whites.clamp(-60.0, 60.0), blacks.clamp(-60.0, 60.0)))
}

/// Refine the base preview-relative exposure match with median luminance while
/// respecting highlight headroom. The base trimmed-mean signal remains the
/// primary exposure estimate; this function only adds a bounded correction.
pub fn reference_relative_exposure_correction(
    reference: &PhotoExposureAnalysis,
    target: &PhotoExposureAnalysis,
    current_delta_ev: f32,
    style_bias_ev: f32,
) -> Option<f32> {
    let reference_median = reference.luminance_p50?;
    let target_median = target.luminance_p50?;
    if reference_median <= 0.0 || target_median <= 0.0 {
        return None;
    }

    let confidence = reference
        .confidence
        .min(target.confidence)
        .clamp(0.2, 1.0);
    let current_factor = 2.0_f32.powf(current_delta_ev.clamp(-5.0, 5.0));
    let style_factor = 2.0_f32.powf(style_bias_ev.clamp(-2.0, 2.0));
    let projected_median = (target_median * current_factor).max(1.0 / 255.0);
    let desired_median = (reference_median * style_factor).max(1.0 / 255.0);

    let mut correction =
        (desired_median / projected_median).log2() * 0.35 * confidence;

    if correction > 0.0 {
        let reference_clip = reference.highlight_clip_ratio.unwrap_or(0.0);
        let target_clip = target.highlight_clip_ratio.unwrap_or(0.0);
        let excess_clip = (target_clip - reference_clip).max(0.0);
        correction *= (1.0 - excess_clip * 20.0).clamp(0.0, 1.0);

        if let (Some(reference_highlight), Some(target_highlight)) =
            (reference.luminance_p90, target.luminance_p90)
        {
            let projected_highlight = target_highlight * current_factor;
            let allowed_highlight = (reference_highlight * style_factor).clamp(0.80, 0.98);
            if projected_highlight >= allowed_highlight {
                correction = 0.0;
            } else if projected_highlight > 0.0 {
                let headroom_ev = (allowed_highlight / projected_highlight).log2().max(0.0);
                correction = correction.min(headroom_ev);
            }
        }
    }

    if correction.abs() < 0.03 {
        correction = 0.0;
    }
    Some(correction.clamp(-0.60, 0.60))
}

/// Match the Reference's overall tonal separation after per-photo exposure
/// alignment. Positive contrast backs off as clipping increases.
pub fn reference_relative_contrast_adjustment(
    reference: &PhotoExposureAnalysis,
    target: &PhotoExposureAnalysis,
    exposure_delta_ev: f32,
) -> Option<f32> {
    let (reference_shadow, reference_highlight) = reference.tone_percentiles()?;
    let (target_shadow, target_highlight) = target.tone_percentiles()?;
    let exposure_factor = 2.0_f32.powf(exposure_delta_ev.clamp(-5.0, 5.0));
    let projected_shadow = (target_shadow * exposure_factor).clamp(0.0, 1.0);
    let projected_highlight = (target_highlight * exposure_factor).clamp(0.0, 1.0);
    let reference_span = (reference_highlight - reference_shadow).max(0.0);
    let target_span = (projected_highlight - projected_shadow).max(0.0);

    let confidence = reference
        .confidence
        .min(target.confidence)
        .clamp(0.2, 1.0);
    let clipping = target
        .shadow_clip_ratio
        .unwrap_or(0.0)
        .max(target.highlight_clip_ratio.unwrap_or(0.0))
        .clamp(0.0, 1.0);
    let positive_headroom = (1.0 - clipping * 20.0).clamp(0.0, 1.0);

    let mut correction = (reference_span - target_span) * 80.0 * confidence;
    if correction > 0.0 {
        correction *= positive_headroom;
    }
    if correction.abs() < 1.0 {
        correction = 0.0;
    }
    Some(correction.clamp(-25.0, 25.0))
}

/// Reduce global saturation when a target is materially more colorful than
/// the selected Reference. Positive adaptive color lift is intentionally left
/// to Vibrance so already-saturated colors are protected.
pub fn reference_relative_saturation_adjustment(
    reference: &PhotoExposureAnalysis,
    target: &PhotoExposureAnalysis,
) -> Option<f32> {
    let reference_colorfulness = reference.colorfulness?;
    let target_colorfulness = target.colorfulness?;
    let confidence = reference
        .confidence
        .min(target.confidence)
        .clamp(0.2, 1.0);

    let mut correction =
        ((reference_colorfulness - target_colorfulness) * 70.0 * confidence).min(0.0);
    if correction.abs() < 1.0 {
        correction = 0.0;
    }
    Some(correction.clamp(-20.0, 0.0))
}

/// Lift muted colors toward the selected Reference without applying the same
/// gain to already-saturated pixels. P25 drives muted-color recovery while P75
/// limits the correction when the target already has a strong saturated tail.
pub fn reference_relative_vibrance_adjustment(
    reference: &PhotoExposureAnalysis,
    target: &PhotoExposureAnalysis,
) -> Option<f32> {
    let reference_mean = reference.colorfulness?;
    let target_mean = target.colorfulness?;
    let reference_p25 = reference.colorfulness_p25?;
    let target_p25 = target.colorfulness_p25?;
    let reference_p75 = reference.colorfulness_p75?;
    let target_p75 = target.colorfulness_p75?;

    let mean_deficit = (reference_mean - target_mean).max(0.0);
    let muted_deficit = (reference_p25 - target_p25).max(0.0);
    if mean_deficit <= 0.0 && muted_deficit <= 0.0 {
        return Some(0.0);
    }

    let confidence = reference
        .confidence
        .min(target.confidence)
        .clamp(0.2, 1.0);
    let saturated_tail_excess = (target_p75 - reference_p75).max(0.0);
    let saturated_headroom = (1.0 - saturated_tail_excess * 8.0).clamp(0.0, 1.0);

    let mut correction =
        (mean_deficit * 45.0 + muted_deficit * 65.0) * confidence * saturated_headroom;
    if correction.abs() < 1.0 {
        correction = 0.0;
    }
    Some(correction.clamp(0.0, 25.0))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedColorEdit {
    pub asset_id: Uuid,
    pub exposure_delta_ev: f32,
    #[serde(default)]
    pub temperature_delta_k: Option<f32>,
    #[serde(default)]
    pub tint_delta: Option<f32>,
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
    let measured_white_balance = analyses
        .iter()
        .filter_map(PhotoColorAnalysis::white_balance)
        .collect::<Vec<_>>();
    let (target_temperature_k, target_tint) = if measured_white_balance.is_empty() {
        (None, None)
    } else {
        (
            Some(median(measured_white_balance.iter().map(|value| value.0))),
            Some(median(measured_white_balance.iter().map(|value| value.1))),
        )
    };

    Ok(GroupColorIntent {
        name: "Auto Balanced".to_string(),
        target_exposure_ev: median(analyses.iter().map(|a| a.exposure_ev)),
        target_temperature_k,
        target_tint,
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
                let (temperature_delta_k, tint_delta) = source
                    .temperature_delta_k
                    .zip(source.tint_delta)
                    .map(|value| (Some(value.0), Some(value.1)))
                    .unwrap_or((None, None));
                ResolvedColorEdit {
                    asset_id: *asset_id,
                    exposure_delta_ev: source.exposure_delta_ev,
                    temperature_delta_k,
                    tint_delta,
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
    let (temperature_delta_k, tint_delta) = match (intent.white_balance(), analysis.white_balance()) {
        (Some((target_temperature, target_tint)), Some((measured_temperature, measured_tint))) => (
            Some(clamp(target_temperature - measured_temperature, -4000.0, 4000.0)),
            Some(clamp(target_tint - measured_tint, -150.0, 150.0)),
        ),
        _ => (None, None),
    };

    ResolvedColorEdit {
        asset_id: analysis.asset_id,
        exposure_delta_ev: clamp(intent.target_exposure_ev - analysis.exposure_ev, -4.0, 4.0),
        temperature_delta_k,
        tint_delta,
        contrast: intent.contrast,
        saturation: intent.saturation,
        semantic: intent.semantic.clone(),
    }
}

fn reference_score(analysis: &PhotoColorAnalysis, intent: &GroupColorIntent) -> f32 {
    let mut score = (analysis.exposure_ev - intent.target_exposure_ev).abs() * 2.0;
    if let (Some((temperature, tint)), Some((target_temperature, target_tint))) =
        (analysis.white_balance(), intent.white_balance())
    {
        score += (temperature - target_temperature).abs() / 2000.0;
        score += (tint - target_tint).abs() / 50.0;
    }
    score + (1.0 - analysis.confidence.clamp(0.0, 1.0)) * 0.5
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

    fn analysis(
        asset_id: Uuid,
        exposure_ev: f32,
        temperature_k: Option<f32>,
    ) -> PhotoColorAnalysis {
        PhotoColorAnalysis {
            asset_id,
            exposure_ev,
            temperature_k,
            tint: temperature_k.map(|_| 0.0),
            confidence: 1.0,
        }
    }

    #[test]
    fn exposure_only_auto_mode_does_not_invent_white_balance() {
        let first = Uuid::new_v4();
        let middle = Uuid::new_v4();
        let last = Uuid::new_v4();
        let analyses = vec![
            analysis(first, -1.0, None),
            analysis(middle, 0.1, None),
            analysis(last, 1.0, None),
        ];
        let plan =
            build_auto_group_plan(Uuid::new_v4(), &[first, middle, last], &analyses, 1).unwrap();

        assert_eq!(plan.reference_asset_id, Some(middle));
        assert_eq!(plan.intent.target_exposure_ev, 0.1);
        assert_eq!(plan.intent.target_temperature_k, None);
        assert_eq!(plan.intent.target_tint, None);
        assert!(plan.resolved.iter().all(|edit| edit.temperature_delta_k.is_none()));
        assert!(plan.resolved.iter().all(|edit| edit.tint_delta.is_none()));
    }

    #[test]
    fn partial_white_balance_is_treated_as_unknown() {
        let asset_id = Uuid::new_v4();
        let analyses = vec![PhotoColorAnalysis {
            asset_id,
            exposure_ev: 0.0,
            temperature_k: Some(5600.0),
            tint: None,
            confidence: 1.0,
        }];

        let plan = build_auto_group_plan(Uuid::new_v4(), &[asset_id], &analyses, 1).unwrap();
        assert_eq!(plan.intent.target_temperature_k, None);
        assert_eq!(plan.intent.target_tint, None);
        assert_eq!(plan.resolved[0].temperature_delta_k, None);
        assert_eq!(plan.resolved[0].tint_delta, None);
    }

    #[test]
    fn measured_white_balance_still_derives_group_target() {
        let first = Uuid::new_v4();
        let middle = Uuid::new_v4();
        let last = Uuid::new_v4();
        let analyses = vec![
            analysis(first, -1.0, Some(5000.0)),
            analysis(middle, 0.1, Some(5500.0)),
            analysis(last, 1.0, Some(6200.0)),
        ];
        let plan =
            build_auto_group_plan(Uuid::new_v4(), &[first, middle, last], &analyses, 1).unwrap();

        assert_eq!(plan.intent.target_temperature_k, Some(5500.0));
        assert_eq!(plan.intent.target_tint, Some(0.0));
    }

    #[test]
    fn adaptive_sync_resolves_different_exposure_without_fake_white_balance() {
        let group_id = Uuid::new_v4();
        let dark = Uuid::new_v4();
        let bright = Uuid::new_v4();
        let intent = GroupColorIntent {
            target_exposure_ev: 0.25,
            ..GroupColorIntent::default()
        };
        let plan = build_adaptive_group_plan(
            group_id,
            &[dark, bright],
            GroupSyncMode::AutoGroup,
            None,
            intent,
            &[analysis(dark, -1.0, None), analysis(bright, 0.8, None)],
            None,
            1,
        )
        .unwrap();

        assert_ne!(
            plan.resolved[0].exposure_delta_ev,
            plan.resolved[1].exposure_delta_ev
        );
        assert_eq!(plan.resolved[0].temperature_delta_k, None);
        assert_eq!(plan.resolved[1].temperature_delta_k, None);
    }

    #[test]
    fn external_reference_can_drive_another_group_with_measured_wb() {
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
                target_temperature_k: Some(5800.0),
                target_tint: Some(3.0),
                ..GroupColorIntent::default()
            },
            &[PhotoColorAnalysis {
                asset_id: target,
                exposure_ev: -0.5,
                temperature_k: Some(5200.0),
                tint: Some(1.0),
                confidence: 1.0,
            }],
            None,
            1,
        )
        .unwrap();

        assert!((plan.resolved[0].exposure_delta_ev - 0.7).abs() < 1e-6);
        assert_eq!(plan.resolved[0].temperature_delta_k, Some(600.0));
        assert_eq!(plan.resolved[0].tint_delta, Some(2.0));
    }

    #[test]
    fn deep_shadow_guard_avoids_aggressive_default_lift() {
        let reference = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: Some(0.02),
            luminance_p10: Some(0.20),
            luminance_p50: Some(0.45),
            luminance_p90: Some(0.75),
            luminance_p98: Some(0.95),
            shadow_clip_ratio: Some(0.0),
            highlight_clip_ratio: Some(0.0),
            colorfulness: None,
            colorfulness_p25: None,
            colorfulness_p75: None,
        };
        let target = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: Some(0.005),
            luminance_p10: Some(0.02),
            luminance_p50: Some(0.40),
            luminance_p90: Some(0.74),
            luminance_p98: Some(0.94),
            shadow_clip_ratio: Some(0.0),
            highlight_clip_ratio: Some(0.0),
            colorfulness: None,
            colorfulness_p25: None,
            colorfulness_p75: None,
        };

        let (_, shadows) =
            reference_relative_tone_adjustments(&reference, &target, 0.0).unwrap();
        assert!(shadows > 0.0);
        assert!(shadows < 25.0);
    }

    #[test]
    fn wide_dynamic_range_caps_opposing_tone_compression() {
        let reference = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: Some(0.03),
            luminance_p10: Some(0.30),
            luminance_p50: Some(0.48),
            luminance_p90: Some(0.65),
            luminance_p98: Some(0.96),
            shadow_clip_ratio: Some(0.0),
            highlight_clip_ratio: Some(0.0),
            colorfulness: None,
            colorfulness_p25: None,
            colorfulness_p75: None,
        };
        let target = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: Some(0.005),
            luminance_p10: Some(0.03),
            luminance_p50: Some(0.48),
            luminance_p90: Some(0.98),
            luminance_p98: Some(1.0),
            shadow_clip_ratio: Some(0.0),
            highlight_clip_ratio: Some(0.0),
            colorfulness: None,
            colorfulness_p25: None,
            colorfulness_p75: None,
        };

        let (highlights, shadows) =
            reference_relative_tone_adjustments(&reference, &target, 0.0).unwrap();
        assert!(highlights < 0.0);
        assert!(shadows > 0.0);
        assert!(shadows + (-highlights) <= 66.0);
    }

    #[test]
    fn deep_black_anchor_damps_positive_blacks() {
        let reference = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: Some(0.12),
            luminance_p10: Some(0.18),
            luminance_p50: Some(0.50),
            luminance_p90: Some(0.80),
            luminance_p98: Some(0.95),
            shadow_clip_ratio: Some(0.0),
            highlight_clip_ratio: Some(0.0),
            colorfulness: None,
            colorfulness_p25: None,
            colorfulness_p75: None,
        };
        let target = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: Some(0.01),
            luminance_p10: Some(0.02),
            luminance_p50: Some(0.50),
            luminance_p90: Some(0.80),
            luminance_p98: Some(0.95),
            shadow_clip_ratio: Some(0.0),
            highlight_clip_ratio: Some(0.0),
            colorfulness: None,
            colorfulness_p25: None,
            colorfulness_p75: None,
        };

        let (_, blacks) =
            reference_relative_endpoint_adjustments(&reference, &target, 0.0).unwrap();
        assert!(blacks > 0.0);
        assert!(blacks < 15.0);
    }

    #[test]
    fn near_white_endpoint_damps_positive_whites() {
        let reference = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: Some(0.03),
            luminance_p10: Some(0.15),
            luminance_p50: Some(0.50),
            luminance_p90: Some(0.82),
            luminance_p98: Some(1.0),
            shadow_clip_ratio: Some(0.0),
            highlight_clip_ratio: Some(0.0),
            colorfulness: None,
            colorfulness_p25: None,
            colorfulness_p75: None,
        };
        let target = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: Some(0.03),
            luminance_p10: Some(0.15),
            luminance_p50: Some(0.50),
            luminance_p90: Some(0.82),
            luminance_p98: Some(0.94),
            shadow_clip_ratio: Some(0.0),
            highlight_clip_ratio: Some(0.0),
            colorfulness: None,
            colorfulness_p25: None,
            colorfulness_p75: None,
        };

        let (whites, _) =
            reference_relative_endpoint_adjustments(&reference, &target, 0.0).unwrap();
        assert!(whites > 0.0);
        assert!(whites < 12.0);
    }

    #[test]
    fn reference_tone_matching_recovers_hot_highlights_and_deep_shadows() {
        let reference = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: None,
            luminance_p10: Some(0.18),
            luminance_p50: Some(0.48),
            luminance_p90: Some(0.78),
            luminance_p98: None,
            shadow_clip_ratio: Some(0.0),
            highlight_clip_ratio: Some(0.0),
            colorfulness: None,
            colorfulness_p25: None,
            colorfulness_p75: None,
        };
        let target = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: None,
            luminance_p10: Some(0.05),
            luminance_p50: Some(0.50),
            luminance_p90: Some(0.96),
            luminance_p98: None,
            shadow_clip_ratio: Some(0.04),
            highlight_clip_ratio: Some(0.05),
            colorfulness: None,
            colorfulness_p25: None,
            colorfulness_p75: None,
        };

        let (highlights, shadows) =
            reference_relative_tone_adjustments(&reference, &target, 0.0).unwrap();
        assert!(highlights < -20.0);
        assert!(shadows > 20.0);
    }

    #[test]
    fn tone_matching_accounts_for_exposure_before_tonal_recovery() {
        let reference = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: None,
            luminance_p10: Some(0.20),
            luminance_p50: Some(0.50),
            luminance_p90: Some(0.80),
            luminance_p98: None,
            shadow_clip_ratio: None,
            highlight_clip_ratio: None,
            colorfulness: None,
            colorfulness_p25: None,
            colorfulness_p75: None,
        };
        let target = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: -1.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: None,
            luminance_p10: Some(0.10),
            luminance_p50: Some(0.25),
            luminance_p90: Some(0.40),
            luminance_p98: None,
            shadow_clip_ratio: None,
            highlight_clip_ratio: None,
            colorfulness: None,
            colorfulness_p25: None,
            colorfulness_p75: None,
        };

        let (highlights, shadows) =
            reference_relative_tone_adjustments(&reference, &target, 1.0).unwrap();
        assert_eq!(highlights, 0.0);
        assert_eq!(shadows, 0.0);
    }

    #[test]
    fn endpoint_matching_deepens_raised_blacks_and_recovers_hot_whites() {
        let reference = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: Some(0.03),
            luminance_p10: Some(0.15),
            luminance_p50: Some(0.50),
            luminance_p90: Some(0.82),
            luminance_p98: Some(0.96),
            shadow_clip_ratio: Some(0.0),
            highlight_clip_ratio: Some(0.0),
            colorfulness: None,
            colorfulness_p25: None,
            colorfulness_p75: None,
        };
        let target = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: Some(0.12),
            luminance_p10: Some(0.18),
            luminance_p50: Some(0.50),
            luminance_p90: Some(0.85),
            luminance_p98: Some(1.0),
            shadow_clip_ratio: Some(0.0),
            highlight_clip_ratio: Some(0.02),
            colorfulness: None,
            colorfulness_p25: None,
            colorfulness_p75: None,
        };

        let (whites, blacks) =
            reference_relative_endpoint_adjustments(&reference, &target, 0.0).unwrap();
        assert!(whites < -10.0);
        assert!(blacks < -10.0);
    }

    #[test]
    fn old_endpoint_evidence_skips_whites_blacks_matching() {
        let evidence = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: None,
            luminance_p10: Some(0.15),
            luminance_p50: Some(0.50),
            luminance_p90: Some(0.85),
            luminance_p98: None,
            shadow_clip_ratio: None,
            highlight_clip_ratio: None,
            colorfulness: None,
            colorfulness_p25: None,
            colorfulness_p75: None,
        };
        assert!(
            reference_relative_endpoint_adjustments(&evidence, &evidence, 0.0).is_none()
        );
    }

    #[test]
    fn channel_clip_excess_pushes_highlights_down_even_with_similar_luma_percentiles() {
        let reference = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: None,
            luminance_p10: Some(0.15),
            luminance_p50: Some(0.50),
            luminance_p90: Some(0.80),
            luminance_p98: None,
            shadow_clip_ratio: Some(0.0),
            highlight_clip_ratio: Some(0.0),
            colorfulness: Some(0.2),
            colorfulness_p25: None,
            colorfulness_p75: None,
        };
        let target = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: None,
            luminance_p10: Some(0.15),
            luminance_p50: Some(0.50),
            luminance_p90: Some(0.80),
            luminance_p98: None,
            shadow_clip_ratio: Some(0.0),
            highlight_clip_ratio: Some(0.05),
            colorfulness: Some(0.2),
            colorfulness_p25: None,
            colorfulness_p75: None,
        };

        let (highlights, shadows) =
            reference_relative_tone_adjustments(&reference, &target, 0.0).unwrap();
        assert!(highlights <= -19.0);
        assert_eq!(shadows, 0.0);
    }

    #[test]
    fn excess_channel_clipping_blocks_median_brightening() {
        let reference = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: None,
            luminance_p10: Some(0.15),
            luminance_p50: Some(0.50),
            luminance_p90: Some(0.90),
            luminance_p98: None,
            shadow_clip_ratio: Some(0.0),
            highlight_clip_ratio: Some(0.0),
            colorfulness: Some(0.2),
            colorfulness_p25: None,
            colorfulness_p75: None,
        };
        let target = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: None,
            luminance_p10: Some(0.08),
            luminance_p50: Some(0.25),
            luminance_p90: Some(0.45),
            luminance_p98: None,
            shadow_clip_ratio: Some(0.0),
            highlight_clip_ratio: Some(0.05),
            colorfulness: Some(0.2),
            colorfulness_p25: None,
            colorfulness_p75: None,
        };

        let correction =
            reference_relative_exposure_correction(&reference, &target, 0.0, 0.0).unwrap();
        assert_eq!(correction, 0.0);
    }

    #[test]
    fn median_refinement_nudges_underexposed_target_without_replacing_base_signal() {
        let reference = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: None,
            luminance_p10: Some(0.15),
            luminance_p50: Some(0.50),
            luminance_p90: Some(0.85),
            luminance_p98: None,
            shadow_clip_ratio: Some(0.0),
            highlight_clip_ratio: Some(0.0),
            colorfulness: None,
            colorfulness_p25: None,
            colorfulness_p75: None,
        };
        let target = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: None,
            luminance_p10: Some(0.08),
            luminance_p50: Some(0.25),
            luminance_p90: Some(0.45),
            luminance_p98: None,
            shadow_clip_ratio: Some(0.0),
            highlight_clip_ratio: Some(0.0),
            colorfulness: None,
            colorfulness_p25: None,
            colorfulness_p75: None,
        };

        let correction =
            reference_relative_exposure_correction(&reference, &target, 0.0, 0.0).unwrap();
        assert!(correction > 0.30);
        assert!(correction <= 0.60);
    }

    #[test]
    fn exposure_refinement_refuses_to_brighten_when_highlights_have_no_headroom() {
        let reference = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: None,
            luminance_p10: Some(0.15),
            luminance_p50: Some(0.50),
            luminance_p90: Some(0.85),
            luminance_p98: None,
            shadow_clip_ratio: Some(0.0),
            highlight_clip_ratio: Some(0.0),
            colorfulness: None,
            colorfulness_p25: None,
            colorfulness_p75: None,
        };
        let target = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: None,
            luminance_p10: Some(0.05),
            luminance_p50: Some(0.25),
            luminance_p90: Some(0.95),
            luminance_p98: None,
            shadow_clip_ratio: Some(0.0),
            highlight_clip_ratio: Some(0.0),
            colorfulness: None,
            colorfulness_p25: None,
            colorfulness_p75: None,
        };

        let correction =
            reference_relative_exposure_correction(&reference, &target, 0.0, 0.0).unwrap();
        assert_eq!(correction, 0.0);
    }

    #[test]
    fn exposure_refinement_preserves_style_bias_target() {
        let reference = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: None,
            luminance_p10: Some(0.15),
            luminance_p50: Some(0.40),
            luminance_p90: Some(0.70),
            luminance_p98: None,
            shadow_clip_ratio: Some(0.0),
            highlight_clip_ratio: Some(0.0),
            colorfulness: None,
            colorfulness_p25: None,
            colorfulness_p75: None,
        };
        let target = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: None,
            luminance_p10: Some(0.15),
            luminance_p50: Some(0.40),
            luminance_p90: Some(0.70),
            luminance_p98: None,
            shadow_clip_ratio: Some(0.0),
            highlight_clip_ratio: Some(0.0),
            colorfulness: None,
            colorfulness_p25: None,
            colorfulness_p75: None,
        };

        // A +0.5 EV style bias is already represented in current_delta_ev, so
        // the median refinement should not pull it back toward the raw Reference.
        let correction =
            reference_relative_exposure_correction(&reference, &target, 0.5, 0.5).unwrap();
        assert_eq!(correction, 0.0);
    }

    #[test]
    fn flat_target_gets_modest_positive_contrast() {
        let reference = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: None,
            luminance_p10: Some(0.15),
            luminance_p50: Some(0.50),
            luminance_p90: Some(0.85),
            luminance_p98: None,
            shadow_clip_ratio: Some(0.0),
            highlight_clip_ratio: Some(0.0),
            colorfulness: None,
            colorfulness_p25: None,
            colorfulness_p75: None,
        };
        let target = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: None,
            luminance_p10: Some(0.30),
            luminance_p50: Some(0.50),
            luminance_p90: Some(0.70),
            luminance_p98: None,
            shadow_clip_ratio: Some(0.0),
            highlight_clip_ratio: Some(0.0),
            colorfulness: None,
            colorfulness_p25: None,
            colorfulness_p75: None,
        };
        let contrast =
            reference_relative_contrast_adjustment(&reference, &target, 0.0).unwrap();
        assert!(contrast > 10.0);
        assert!(contrast <= 25.0);
    }

    #[test]
    fn clipped_target_does_not_get_aggressive_positive_contrast() {
        let reference = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: None,
            luminance_p10: Some(0.10),
            luminance_p50: Some(0.50),
            luminance_p90: Some(0.90),
            luminance_p98: None,
            shadow_clip_ratio: Some(0.0),
            highlight_clip_ratio: Some(0.0),
            colorfulness: None,
            colorfulness_p25: None,
            colorfulness_p75: None,
        };
        let target = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: None,
            luminance_p10: Some(0.30),
            luminance_p50: Some(0.50),
            luminance_p90: Some(0.70),
            luminance_p98: None,
            shadow_clip_ratio: Some(0.04),
            highlight_clip_ratio: Some(0.0),
            colorfulness: None,
            colorfulness_p25: None,
            colorfulness_p75: None,
        };
        let contrast =
            reference_relative_contrast_adjustment(&reference, &target, 0.0).unwrap();
        assert!(contrast >= 0.0);
        assert!(contrast < 7.0);
    }

    #[test]
    fn overly_hard_target_gets_negative_contrast() {
        let reference = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: None,
            luminance_p10: Some(0.20),
            luminance_p50: Some(0.50),
            luminance_p90: Some(0.80),
            luminance_p98: None,
            shadow_clip_ratio: Some(0.0),
            highlight_clip_ratio: Some(0.0),
            colorfulness: None,
            colorfulness_p25: None,
            colorfulness_p75: None,
        };
        let target = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: None,
            luminance_p10: Some(0.02),
            luminance_p50: Some(0.50),
            luminance_p90: Some(0.98),
            luminance_p98: None,
            shadow_clip_ratio: Some(0.0),
            highlight_clip_ratio: Some(0.0),
            colorfulness: None,
            colorfulness_p25: None,
            colorfulness_p75: None,
        };
        let contrast =
            reference_relative_contrast_adjustment(&reference, &target, 0.0).unwrap();
        assert!(contrast < -10.0);
    }

    #[test]
    fn muted_target_uses_vibrance_instead_of_global_saturation() {
        let reference = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: None,
            luminance_p10: None,
            luminance_p50: None,
            luminance_p90: None,
            luminance_p98: None,
            shadow_clip_ratio: None,
            highlight_clip_ratio: None,
            colorfulness: Some(0.35),
            colorfulness_p25: Some(0.20),
            colorfulness_p75: Some(0.50),
        };
        let target = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: None,
            luminance_p10: None,
            luminance_p50: None,
            luminance_p90: None,
            luminance_p98: None,
            shadow_clip_ratio: None,
            highlight_clip_ratio: None,
            colorfulness: Some(0.15),
            colorfulness_p25: Some(0.05),
            colorfulness_p75: Some(0.30),
        };

        assert_eq!(
            reference_relative_saturation_adjustment(&reference, &target),
            Some(0.0)
        );
        let vibrance =
            reference_relative_vibrance_adjustment(&reference, &target).unwrap();
        assert!(vibrance > 10.0);
        assert!(vibrance <= 25.0);
    }

    #[test]
    fn saturated_tail_blocks_aggressive_vibrance_lift() {
        let reference = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: None,
            luminance_p10: None,
            luminance_p50: None,
            luminance_p90: None,
            luminance_p98: None,
            shadow_clip_ratio: None,
            highlight_clip_ratio: None,
            colorfulness: Some(0.35),
            colorfulness_p25: Some(0.20),
            colorfulness_p75: Some(0.50),
        };
        let target = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: None,
            luminance_p10: None,
            luminance_p50: None,
            luminance_p90: None,
            luminance_p98: None,
            shadow_clip_ratio: None,
            highlight_clip_ratio: None,
            colorfulness: Some(0.25),
            colorfulness_p25: Some(0.05),
            colorfulness_p75: Some(0.65),
        };

        let vibrance =
            reference_relative_vibrance_adjustment(&reference, &target).unwrap();
        assert!(vibrance < 3.0);
    }

    #[test]
    fn overly_colorful_target_gets_negative_global_saturation() {
        let reference = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: None,
            luminance_p10: None,
            luminance_p50: None,
            luminance_p90: None,
            luminance_p98: None,
            shadow_clip_ratio: None,
            highlight_clip_ratio: None,
            colorfulness: Some(0.20),
            colorfulness_p25: Some(0.10),
            colorfulness_p75: Some(0.35),
        };
        let target = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: None,
            luminance_p10: None,
            luminance_p50: None,
            luminance_p90: None,
            luminance_p98: None,
            shadow_clip_ratio: None,
            highlight_clip_ratio: None,
            colorfulness: Some(0.45),
            colorfulness_p25: Some(0.25),
            colorfulness_p75: Some(0.65),
        };

        let saturation =
            reference_relative_saturation_adjustment(&reference, &target).unwrap();
        assert!(saturation < -10.0);
        assert_eq!(
            reference_relative_vibrance_adjustment(&reference, &target),
            Some(0.0)
        );
    }

    #[test]
    fn old_color_distribution_skips_vibrance_matching() {
        let evidence = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: None,
            luminance_p10: None,
            luminance_p50: None,
            luminance_p90: None,
            luminance_p98: None,
            shadow_clip_ratio: None,
            highlight_clip_ratio: None,
            colorfulness: Some(0.2),
            colorfulness_p25: None,
            colorfulness_p75: None,
        };
        assert!(
            reference_relative_vibrance_adjustment(&evidence, &evidence).is_none()
        );
    }

    #[test]
    fn old_color_evidence_skips_saturation_matching() {
        let evidence = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: None,
            luminance_p10: None,
            luminance_p50: None,
            luminance_p90: None,
            luminance_p98: None,
            shadow_clip_ratio: None,
            highlight_clip_ratio: None,
            colorfulness: None,
            colorfulness_p25: None,
            colorfulness_p75: None,
        };
        assert!(reference_relative_saturation_adjustment(&evidence, &evidence).is_none());
    }

    #[test]
    fn old_exposure_evidence_without_percentiles_skips_tone_matching() {
        let reference = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: None,
            luminance_p10: None,
            luminance_p50: None,
            luminance_p90: None,
            luminance_p98: None,
            shadow_clip_ratio: None,
            highlight_clip_ratio: None,
            colorfulness: None,
            colorfulness_p25: None,
            colorfulness_p75: None,
        };
        assert!(reference_relative_tone_adjustments(&reference, &reference, 0.0).is_none());
    }

    #[test]
    fn manual_copy_preserves_missing_white_balance() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let source = ResolvedColorEdit {
            asset_id: first,
            exposure_delta_ev: 0.4,
            temperature_delta_k: None,
            tint_delta: None,
            contrast: 10.0,
            saturation: 4.0,
            semantic: Vec::new(),
        };
        let plan = build_adaptive_group_plan(
            Uuid::new_v4(),
            &[first, second],
            GroupSyncMode::ManualCopy,
            Some(first),
            GroupColorIntent::default(),
            &[],
            Some(&source),
            1,
        )
        .unwrap();

        assert_eq!(plan.resolved[0].temperature_delta_k, None);
        assert_eq!(plan.resolved[1].temperature_delta_k, None);
    }
}
