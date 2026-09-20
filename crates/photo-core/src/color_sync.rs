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

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct HueColorBin {
    pub coverage: f32,
    pub saturation: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct HueColorDistribution {
    pub red: HueColorBin,
    pub orange: HueColorBin,
    pub yellow: HueColorBin,
    pub green: HueColorBin,
    pub aqua: HueColorBin,
    pub blue: HueColorBin,
    pub purple: HueColorBin,
    pub magenta: HueColorBin,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ColorMixerSaturation {
    pub red: Option<f32>,
    pub orange: Option<f32>,
    pub yellow: Option<f32>,
    pub green: Option<f32>,
    pub aqua: Option<f32>,
    pub blue: Option<f32>,
    pub purple: Option<f32>,
    pub magenta: Option<f32>,
}

impl ColorMixerSaturation {
    pub fn is_empty(&self) -> bool {
        [
            self.red, self.orange, self.yellow, self.green,
            self.aqua, self.blue, self.purple, self.magenta,
        ]
        .into_iter()
        .all(|value| value.is_none())
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
    #[serde(default)]
    pub hue_color_distribution: Option<HueColorDistribution>,
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

pub fn reference_relative_match_strength(
    reference: &PhotoExposureAnalysis,
    target: &PhotoExposureAnalysis,
) -> Option<f32> {
    let reference_mid = reference.luminance_p50?;
    let target_mid = target.luminance_p50?;
    let (reference_shadow, reference_highlight) = reference.tone_percentiles()?;
    let (target_shadow, target_highlight) = target.tone_percentiles()?;
    if reference_mid <= 0.0 || target_mid <= 0.0 {
        return None;
    }

    let safe_ratio = |value: f32, midpoint: f32| {
        (value.max(1.0 / 255.0) / midpoint.max(1.0 / 255.0))
            .max(1.0 / 255.0)
            .log2()
    };
    let tone_shape_distance = (
        (safe_ratio(reference_shadow, reference_mid) - safe_ratio(target_shadow, target_mid)).abs()
            + (safe_ratio(reference_highlight, reference_mid)
                - safe_ratio(target_highlight, target_mid))
            .abs()
    ) * 0.5;

    let color_distance = match (reference.colorfulness, target.colorfulness) {
        (Some(reference_mean), Some(target_mean)) => {
            let muted = match (reference.colorfulness_p25, target.colorfulness_p25) {
                (Some(reference_muted), Some(target_muted)) => {
                    (reference_muted - target_muted).abs() * 0.35
                }
                _ => 0.0,
            };
            let tail = match (reference.colorfulness_p75, target.colorfulness_p75) {
                (Some(reference_tail), Some(target_tail)) => {
                    (reference_tail - target_tail).abs() * 0.5
                }
                _ => 0.0,
            };
            (reference_mean - target_mean).abs() + muted + tail
        }
        _ => 0.0,
    };

    let clipping_distance =
        (reference.shadow_clip_ratio.unwrap_or(0.0) - target.shadow_clip_ratio.unwrap_or(0.0))
            .abs()
            + (reference.highlight_clip_ratio.unwrap_or(0.0)
                - target.highlight_clip_ratio.unwrap_or(0.0))
            .abs();
    let confidence = reference.confidence.min(target.confidence).clamp(0.0, 1.0);
    let penalty = tone_shape_distance * 0.20
        + color_distance * 0.30
        + clipping_distance * 2.0
        + (1.0 - confidence) * 0.10;
    Some((1.0 - penalty).clamp(0.45, 1.0))
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

fn midtone_log_separation(
    shadow: f32,
    median: f32,
    highlight: f32,
) -> Option<(f32, f32)> {
    let floor = 1.0 / 255.0;
    if median <= floor || shadow < 0.0 || highlight <= 0.0 {
        return None;
    }
    let lower = (median / shadow.max(floor)).log2().max(0.0);
    let upper = (highlight.max(floor) / median.max(floor)).log2().max(0.0);
    Some((lower, upper))
}

/// Match the Reference's tonal separation around the midtone after per-photo
/// exposure alignment.
///
/// Contrast is a global control, so it should react strongly only when both
/// shadow-to-mid and mid-to-highlight structure support the same direction.
/// If one side is flatter while the other is already harder than the
/// Reference, Highlights/Shadows are the safer controls and global Contrast is
/// deliberately damped. Log-space ratios also make the comparison stable for
/// ordinary exposure-only differences.
pub fn reference_relative_contrast_adjustment(
    reference: &PhotoExposureAnalysis,
    target: &PhotoExposureAnalysis,
    _exposure_delta_ev: f32,
) -> Option<f32> {
    let reference_median = reference.luminance_p50?;
    let target_median = target.luminance_p50?;
    let (reference_shadow, reference_highlight) = reference.tone_percentiles()?;
    let (target_shadow, target_highlight) = target.tone_percentiles()?;
    let (reference_lower, reference_upper) =
        midtone_log_separation(reference_shadow, reference_median, reference_highlight)?;
    let (target_lower, target_upper) =
        midtone_log_separation(target_shadow, target_median, target_highlight)?;

    let lower_deficit = reference_lower - target_lower;
    let upper_deficit = reference_upper - target_upper;
    let confidence = reference
        .confidence
        .min(target.confidence)
        .clamp(0.2, 1.0);

    // Both sides agreeing means the entire frame is genuinely flatter/harder.
    // Opposing signs describe asymmetric tone structure, where a global
    // Contrast move would improve one side while worsening the other.
    let direction_agreement = if lower_deficit * upper_deficit < 0.0 {
        0.35
    } else {
        1.0
    };
    let mut correction =
        (lower_deficit + upper_deficit) * 11.0 * confidence * direction_agreement;

    if correction > 0.0 {
        let clipping = target
            .shadow_clip_ratio
            .unwrap_or(0.0)
            .max(target.highlight_clip_ratio.unwrap_or(0.0))
            .clamp(0.0, 1.0);
        let positive_headroom = (1.0 - clipping * 20.0).clamp(0.0, 1.0);
        correction *= positive_headroom;

        // Strong positive contrast is also a poor default when either endpoint
        // is already crowded against the preview boundary.
        if let Some((black, white)) = target.endpoint_percentiles() {
            let endpoint_headroom =
                ((black / 0.03).clamp(0.0, 1.0) * ((1.0 - white) / 0.04).clamp(0.0, 1.0))
                    .sqrt();
            correction *= (0.45 + endpoint_headroom * 0.55).clamp(0.45, 1.0);
        }
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
    let mean_excess = (target_colorfulness - reference_colorfulness).max(0.0);
    if mean_excess <= 0.0 {
        return Some(0.0);
    }

    let confidence = reference
        .confidence
        .min(target.confidence)
        .clamp(0.2, 1.0);

    // Mean colorfulness alone cannot tell whether the whole frame is too
    // colorful or whether a small neon/flower/sunset region creates a strong
    // saturated tail. Use P25 as evidence that the broader image really is
    // more colorful before committing to a strong global desaturation.
    let broad_excess = match (reference.colorfulness_p25, target.colorfulness_p25) {
        (Some(reference_p25), Some(target_p25)) => (target_p25 - reference_p25).max(0.0),
        _ => {
            let mut legacy = -(mean_excess * 70.0 * confidence);
            if legacy.abs() < 1.0 {
                legacy = 0.0;
            }
            return Some(legacy.clamp(-20.0, 0.0));
        }
    };
    let tail_excess = match (reference.colorfulness_p75, target.colorfulness_p75) {
        (Some(reference_p75), Some(target_p75)) => (target_p75 - reference_p75).max(0.0),
        _ => broad_excess,
    };

    let mut magnitude = (mean_excess * 45.0 + broad_excess * 55.0) * confidence;
    let localized_tail = broad_excess < 0.045 && tail_excess > broad_excess + 0.10;
    if localized_tail {
        magnitude *= 0.35;
    } else if broad_excess < 0.08 && tail_excess > broad_excess + 0.06 {
        magnitude *= 0.65;
    }

    if magnitude < 1.0 {
        magnitude = 0.0;
    }
    Some((-magnitude).clamp(-20.0, 0.0))
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

    // A dark embedded preview cannot prove that lifted chroma is clean RAW
    // detail rather than low-light color noise. Preserve muted-color recovery,
    // but taper positive Vibrance when the frame is genuinely low-key.
    let low_light_mid_guard = target
        .luminance_p50
        .map(|mid| 0.35 + ((mid - 0.10) / 0.20).clamp(0.0, 1.0) * 0.65)
        .unwrap_or(1.0);
    let low_light_shadow_guard = target
        .luminance_p10
        .map(|shadow| 0.45 + ((shadow - 0.02) / 0.08).clamp(0.0, 1.0) * 0.55)
        .unwrap_or(1.0);
    let shadow_clip_guard =
        (1.0 - target.shadow_clip_ratio.unwrap_or(0.0) * 10.0).clamp(0.45, 1.0);
    let low_light_guard = low_light_mid_guard
        .min(low_light_shadow_guard)
        .min(shadow_clip_guard);

    // Absolute saturated-tail pressure matters even when the Reference itself
    // is colorful (sunset, neon, stage lighting). Moderate channel clipping
    // further reduces positive Vibrance so hue/chroma does not pile up at the
    // highlight boundary.
    let absolute_tail_pressure = ((target_p75 - 0.72) / 0.23).clamp(0.0, 1.0);
    let saturated_tail_guard = (1.0 - absolute_tail_pressure * 0.55).clamp(0.45, 1.0);
    let highlight_clip_guard =
        (1.0 - target.highlight_clip_ratio.unwrap_or(0.0) * 25.0).clamp(0.35, 1.0);
    let highlight_color_guard = saturated_tail_guard.min(highlight_clip_guard);

    let mut correction = (mean_deficit * 45.0 + muted_deficit * 65.0)
        * confidence
        * saturated_headroom
        * low_light_guard
        * highlight_color_guard;
    if correction.abs() < 1.0 {
        correction = 0.0;
    }
    Some(correction.clamp(0.0, 25.0))
}

pub fn reference_relative_color_adjustments(
    reference: &PhotoExposureAnalysis,
    target: &PhotoExposureAnalysis,
) -> (Option<f32>, Option<f32>) {
    let mut saturation = reference_relative_saturation_adjustment(reference, target);
    let mut vibrance = reference_relative_vibrance_adjustment(reference, target);

    if let (Some(saturation_value), Some(vibrance_value)) = (saturation, vibrance) {
        if saturation_value < 0.0 && vibrance_value > 0.0 {
            let muted_deficit = reference
                .colorfulness_p25
                .zip(target.colorfulness_p25)
                .map(|(reference_p25, target_p25)| (reference_p25 - target_p25).max(0.0))
                .unwrap_or(0.0);

            // Negative global Saturation and positive Vibrance can partially
            // cancel each other while changing the distribution in opposite
            // directions. When the lower quartile is genuinely deficient,
            // keep the selective Vibrance path and avoid globally washing it
            // out. If the lower quartile is effectively already matched,
            // prefer the global desaturation and skip the redundant Vibrance.
            if muted_deficit >= 0.03 {
                saturation = Some(0.0);
                vibrance = Some(vibrance_value.min(12.0));
            } else {
                vibrance = Some(0.0);
            }
        }
    }

    (saturation, vibrance)
}

pub fn reference_relative_color_distribution_conflict(
    reference: &PhotoExposureAnalysis,
    target: &PhotoExposureAnalysis,
) -> bool {
    let Some((reference_p25, target_p25)) =
        reference.colorfulness_p25.zip(target.colorfulness_p25)
    else {
        return false;
    };
    let Some((reference_p75, target_p75)) =
        reference.colorfulness_p75.zip(target.colorfulness_p75)
    else {
        return false;
    };

    let muted_deficit = (reference_p25 - target_p25).max(0.0);
    let tail_excess = (target_p75 - reference_p75).max(0.0);
    let reference_spread = (reference_p75 - reference_p25).max(0.0);
    let target_spread = (target_p75 - target_p25).max(0.0);

    muted_deficit > 0.05
        && tail_excess > 0.08
        && target_spread > reference_spread + 0.16
        && target_spread > 0.42
}


fn selective_saturation_delta(
    reference: HueColorBin,
    target: HueColorBin,
    confidence: f32,
    max_negative_abs: f32,
    max_positive: f32,
) -> Option<f32> {
    let min_coverage = reference.coverage.min(target.coverage);
    let max_coverage = reference.coverage.max(target.coverage);
    if min_coverage < 0.015 {
        return None;
    }

    // A hue that occupies very different amounts of the two frames is more
    // likely a different object/scene composition than a style mismatch.
    let coverage_overlap = min_coverage / max_coverage.max(1e-6);
    if coverage_overlap < 0.25 {
        return None;
    }

    // Small-but-real hue regions remain usable, but they receive less authority
    // than broad, well-supported regions.
    let coverage_reliability = ((min_coverage - 0.015) / 0.08).clamp(0.0, 1.0);
    let strength = (0.35 + coverage_overlap.clamp(0.0, 1.0) * 0.65)
        * (0.55 + coverage_reliability * 0.45);
    let delta = (reference.saturation - target.saturation) * 55.0 * confidence * strength;
    if delta.abs() < 2.0 {
        None
    } else {
        Some(delta.clamp(-max_negative_abs, max_positive))
    }
}

/// Produce conservative Lightroom Color Mixer saturation corrections only for
/// frames whose lower- and upper-colorfulness quartiles already prove that
/// global Saturation/Vibrance cannot satisfy both muted and saturated regions.
///
/// Orange/red are capped more tightly because those bins frequently contain
/// skin. Missing hue evidence or non-conflict frames return None.
pub fn reference_relative_color_mixer_saturation(
    reference: &PhotoExposureAnalysis,
    target: &PhotoExposureAnalysis,
) -> Option<ColorMixerSaturation> {
    if !reference_relative_color_distribution_conflict(reference, target) {
        return None;
    }
    let reference_hues = reference.hue_color_distribution.as_ref()?;
    let target_hues = target.hue_color_distribution.as_ref()?;
    let confidence = reference.confidence.min(target.confidence).clamp(0.2, 1.0);

    let result = ColorMixerSaturation {
        // Positive warm-hue lift is deliberately tighter than desaturation:
        // red/orange frequently carry skin, wood and indoor warm light.
        red: selective_saturation_delta(reference_hues.red, target_hues.red, confidence, 10.0, 6.0),
        orange: selective_saturation_delta(reference_hues.orange, target_hues.orange, confidence, 8.0, 5.0),
        yellow: selective_saturation_delta(reference_hues.yellow, target_hues.yellow, confidence, 16.0, 12.0),
        green: selective_saturation_delta(reference_hues.green, target_hues.green, confidence, 16.0, 16.0),
        aqua: selective_saturation_delta(reference_hues.aqua, target_hues.aqua, confidence, 16.0, 16.0),
        blue: selective_saturation_delta(reference_hues.blue, target_hues.blue, confidence, 16.0, 16.0),
        purple: selective_saturation_delta(reference_hues.purple, target_hues.purple, confidence, 14.0, 14.0),
        magenta: selective_saturation_delta(reference_hues.magenta, target_hues.magenta, confidence, 12.0, 10.0),
    };
    (!result.is_empty()).then_some(result)
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
    fn reference_match_strength_keeps_exposure_only_variation_near_full_strength() {
        let reference = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(), exposure_ev: 0.0, temperature_k: None, tint: None,
            confidence: 1.0, luminance_p02: Some(0.04), luminance_p10: Some(0.10),
            luminance_p50: Some(0.20), luminance_p90: Some(0.40), luminance_p98: Some(0.50),
            shadow_clip_ratio: Some(0.0), highlight_clip_ratio: Some(0.0),
            colorfulness: Some(0.35), colorfulness_p25: Some(0.18), colorfulness_p75: Some(0.55),
        
            hue_color_distribution: None,
        };
        let target = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(), exposure_ev: 1.0, temperature_k: None, tint: None,
            confidence: 1.0, luminance_p02: Some(0.08), luminance_p10: Some(0.20),
            luminance_p50: Some(0.40), luminance_p90: Some(0.80), luminance_p98: Some(0.95),
            shadow_clip_ratio: Some(0.0), highlight_clip_ratio: Some(0.0),
            colorfulness: Some(0.35), colorfulness_p25: Some(0.18), colorfulness_p75: Some(0.55),
        
            hue_color_distribution: None,
        };
        assert!(reference_relative_match_strength(&reference, &target).unwrap() > 0.95);
    }

    #[test]
    fn reference_match_strength_backs_off_for_scene_outlier() {
        let reference = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(), exposure_ev: 0.0, temperature_k: None, tint: None,
            confidence: 1.0, luminance_p02: Some(0.05), luminance_p10: Some(0.20),
            luminance_p50: Some(0.50), luminance_p90: Some(0.78), luminance_p98: Some(0.95),
            shadow_clip_ratio: Some(0.0), highlight_clip_ratio: Some(0.0),
            colorfulness: Some(0.25), colorfulness_p25: Some(0.12), colorfulness_p75: Some(0.45),
        
            hue_color_distribution: None,
        };
        let target = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(), exposure_ev: 0.0, temperature_k: None, tint: None,
            confidence: 1.0, luminance_p02: Some(0.005), luminance_p10: Some(0.03),
            luminance_p50: Some(0.48), luminance_p90: Some(0.98), luminance_p98: Some(1.0),
            shadow_clip_ratio: Some(0.02), highlight_clip_ratio: Some(0.02),
            colorfulness: Some(0.65), colorfulness_p25: Some(0.30), colorfulness_p75: Some(0.90),
        
            hue_color_distribution: None,
        };
        let strength = reference_relative_match_strength(&reference, &target).unwrap();
        assert!(strength < 0.62);
        assert!(strength >= 0.45);
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
            hue_color_distribution: None,
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
            hue_color_distribution: None,
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
            hue_color_distribution: None,
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
            hue_color_distribution: None,
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
            hue_color_distribution: None,
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
            hue_color_distribution: None,
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
            hue_color_distribution: None,
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
            hue_color_distribution: None,
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
            hue_color_distribution: None,
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
            hue_color_distribution: None,
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
            hue_color_distribution: None,
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
            hue_color_distribution: None,
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
            hue_color_distribution: None,
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
            hue_color_distribution: None,
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
            hue_color_distribution: None,
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
            hue_color_distribution: None,
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
            hue_color_distribution: None,
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
            hue_color_distribution: None,
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
            hue_color_distribution: None,
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
            hue_color_distribution: None,
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
            hue_color_distribution: None,
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
            hue_color_distribution: None,
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
            hue_color_distribution: None,
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
            hue_color_distribution: None,
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
            hue_color_distribution: None,
        };

        // A +0.5 EV style bias is already represented in current_delta_ev, so
        // the median refinement should not pull it back toward the raw Reference.
        let correction =
            reference_relative_exposure_correction(&reference, &target, 0.5, 0.5).unwrap();
        assert_eq!(correction, 0.0);
    }

    #[test]
    fn exposure_only_tone_shift_does_not_create_false_contrast_delta() {
        let reference = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: Some(0.03),
            luminance_p10: Some(0.15),
            luminance_p50: Some(0.50),
            luminance_p90: Some(0.85),
            luminance_p98: Some(0.96),
            shadow_clip_ratio: Some(0.0),
            highlight_clip_ratio: Some(0.0),
            colorfulness: None,
            colorfulness_p25: None,
            colorfulness_p75: None,
            hue_color_distribution: None,
        };
        let target = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: -1.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: Some(0.015),
            luminance_p10: Some(0.075),
            luminance_p50: Some(0.25),
            luminance_p90: Some(0.425),
            luminance_p98: Some(0.48),
            shadow_clip_ratio: Some(0.0),
            highlight_clip_ratio: Some(0.0),
            colorfulness: None,
            colorfulness_p25: None,
            colorfulness_p75: None,
            hue_color_distribution: None,
        };

        assert_eq!(
            reference_relative_contrast_adjustment(&reference, &target, 1.0),
            Some(0.0)
        );
    }

    #[test]
    fn asymmetric_tone_shape_avoids_global_contrast_overcorrection() {
        let reference = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: Some(0.03),
            luminance_p10: Some(0.15),
            luminance_p50: Some(0.50),
            luminance_p90: Some(0.85),
            luminance_p98: Some(0.96),
            shadow_clip_ratio: Some(0.0),
            highlight_clip_ratio: Some(0.0),
            colorfulness: None,
            colorfulness_p25: None,
            colorfulness_p75: None,
            hue_color_distribution: None,
        };
        let target = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: Some(0.04),
            luminance_p10: Some(0.30),
            luminance_p50: Some(0.50),
            luminance_p90: Some(0.95),
            luminance_p98: Some(0.99),
            shadow_clip_ratio: Some(0.0),
            highlight_clip_ratio: Some(0.0),
            colorfulness: None,
            colorfulness_p25: None,
            colorfulness_p75: None,
            hue_color_distribution: None,
        };

        let contrast =
            reference_relative_contrast_adjustment(&reference, &target, 0.0).unwrap();
        assert!(contrast > 0.0);
        assert!(contrast < 5.0);
    }

    #[test]
    fn endpoint_pressure_damps_positive_global_contrast() {
        let reference = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: Some(0.04),
            luminance_p10: Some(0.15),
            luminance_p50: Some(0.50),
            luminance_p90: Some(0.85),
            luminance_p98: Some(0.95),
            shadow_clip_ratio: Some(0.0),
            highlight_clip_ratio: Some(0.0),
            colorfulness: None,
            colorfulness_p25: None,
            colorfulness_p75: None,
            hue_color_distribution: None,
        };
        let target = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(),
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 1.0,
            luminance_p02: Some(0.005),
            luminance_p10: Some(0.30),
            luminance_p50: Some(0.50),
            luminance_p90: Some(0.70),
            luminance_p98: Some(0.995),
            shadow_clip_ratio: Some(0.0),
            highlight_clip_ratio: Some(0.0),
            colorfulness: None,
            colorfulness_p25: None,
            colorfulness_p75: None,
            hue_color_distribution: None,
        };

        let contrast =
            reference_relative_contrast_adjustment(&reference, &target, 0.0).unwrap();
        assert!(contrast > 0.0);
        assert!(contrast < 8.0);
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
            hue_color_distribution: None,
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
            hue_color_distribution: None,
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
            hue_color_distribution: None,
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
            hue_color_distribution: None,
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
            hue_color_distribution: None,
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
            hue_color_distribution: None,
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
            hue_color_distribution: None,
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
            hue_color_distribution: None,
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
    fn low_light_guard_damps_positive_vibrance_without_disabling_it() {
        let reference = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(), exposure_ev: 0.0, temperature_k: None, tint: None,
            confidence: 1.0, luminance_p02: Some(0.01), luminance_p10: Some(0.10),
            luminance_p50: Some(0.32), luminance_p90: Some(0.72), luminance_p98: Some(0.90),
            shadow_clip_ratio: Some(0.0), highlight_clip_ratio: Some(0.0),
            colorfulness: Some(0.35), colorfulness_p25: Some(0.20), colorfulness_p75: Some(0.50),
        
            hue_color_distribution: None,
        };
        let target = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(), exposure_ev: 0.0, temperature_k: None, tint: None,
            confidence: 1.0, luminance_p02: Some(0.003), luminance_p10: Some(0.02),
            luminance_p50: Some(0.12), luminance_p90: Some(0.55), luminance_p98: Some(0.80),
            shadow_clip_ratio: Some(0.01), highlight_clip_ratio: Some(0.0),
            colorfulness: Some(0.15), colorfulness_p25: Some(0.05), colorfulness_p75: Some(0.30),
        
            hue_color_distribution: None,
        };

        let vibrance =
            reference_relative_vibrance_adjustment(&reference, &target).unwrap();
        assert!(vibrance > 4.0);
        assert!(vibrance < 10.0);
    }

    #[test]
    fn saturated_highlight_guard_damps_color_lift_for_neon_like_tail() {
        let reference = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(), exposure_ev: 0.0, temperature_k: None, tint: None,
            confidence: 1.0, luminance_p02: Some(0.03), luminance_p10: Some(0.12),
            luminance_p50: Some(0.45), luminance_p90: Some(0.82), luminance_p98: Some(0.96),
            shadow_clip_ratio: Some(0.0), highlight_clip_ratio: Some(0.0),
            colorfulness: Some(0.60), colorfulness_p25: Some(0.30), colorfulness_p75: Some(0.90),
        
            hue_color_distribution: None,
        };
        let target = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(), exposure_ev: 0.0, temperature_k: None, tint: None,
            confidence: 1.0, luminance_p02: Some(0.03), luminance_p10: Some(0.12),
            luminance_p50: Some(0.45), luminance_p90: Some(0.88), luminance_p98: Some(0.99),
            shadow_clip_ratio: Some(0.0), highlight_clip_ratio: Some(0.02),
            colorfulness: Some(0.35), colorfulness_p25: Some(0.10), colorfulness_p75: Some(0.82),
        
            hue_color_distribution: None,
        };

        let vibrance =
            reference_relative_vibrance_adjustment(&reference, &target).unwrap();
        assert!(vibrance > 8.0);
        assert!(vibrance < 14.0);
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
            hue_color_distribution: None,
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
            hue_color_distribution: None,
        };

        let vibrance =
            reference_relative_vibrance_adjustment(&reference, &target).unwrap();
        assert!(vibrance < 3.0);
    }

    #[test]
    fn localized_saturated_tail_does_not_desaturate_muted_frame_aggressively() {
        let reference = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(), exposure_ev: 0.0, temperature_k: None, tint: None,
            confidence: 1.0, luminance_p02: None, luminance_p10: None, luminance_p50: None,
            luminance_p90: None, luminance_p98: None, shadow_clip_ratio: None,
            highlight_clip_ratio: None, colorfulness: Some(0.22), colorfulness_p25: Some(0.10),
            colorfulness_p75: Some(0.38),
            hue_color_distribution: None,
        };
        let target = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(), exposure_ev: 0.0, temperature_k: None, tint: None,
            confidence: 1.0, luminance_p02: None, luminance_p10: None, luminance_p50: None,
            luminance_p90: None, luminance_p98: None, shadow_clip_ratio: None,
            highlight_clip_ratio: None, colorfulness: Some(0.34), colorfulness_p25: Some(0.10),
            colorfulness_p75: Some(0.70),
            hue_color_distribution: None,
        };

        let saturation =
            reference_relative_saturation_adjustment(&reference, &target).unwrap();
        assert!(saturation < 0.0);
        assert!(saturation > -4.0);
    }

    #[test]
    fn broadly_colorful_target_still_gets_meaningful_global_desaturation() {
        let reference = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(), exposure_ev: 0.0, temperature_k: None, tint: None,
            confidence: 1.0, luminance_p02: None, luminance_p10: None, luminance_p50: None,
            luminance_p90: None, luminance_p98: None, shadow_clip_ratio: None,
            highlight_clip_ratio: None, colorfulness: Some(0.20), colorfulness_p25: Some(0.10),
            colorfulness_p75: Some(0.35),
            hue_color_distribution: None,
        };
        let target = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(), exposure_ev: 0.0, temperature_k: None, tint: None,
            confidence: 1.0, luminance_p02: None, luminance_p10: None, luminance_p50: None,
            luminance_p90: None, luminance_p98: None, shadow_clip_ratio: None,
            highlight_clip_ratio: None, colorfulness: Some(0.45), colorfulness_p25: Some(0.25),
            colorfulness_p75: Some(0.65),
            hue_color_distribution: None,
        };

        let saturation =
            reference_relative_saturation_adjustment(&reference, &target).unwrap();
        assert!(saturation < -15.0);
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
            hue_color_distribution: None,
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
            hue_color_distribution: None,
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
    fn coordinated_color_adjustment_prefers_vibrance_for_muted_deficit() {
        let reference = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(), exposure_ev: 0.0, temperature_k: None, tint: None,
            confidence: 1.0, luminance_p02: None, luminance_p10: None, luminance_p50: None,
            luminance_p90: None, luminance_p98: None, shadow_clip_ratio: None,
            highlight_clip_ratio: None, colorfulness: Some(0.30), colorfulness_p25: Some(0.20),
            colorfulness_p75: Some(0.50),
            hue_color_distribution: None,
        };
        let target = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(), exposure_ev: 0.0, temperature_k: None, tint: None,
            confidence: 1.0, luminance_p02: None, luminance_p10: None, luminance_p50: None,
            luminance_p90: None, luminance_p98: None, shadow_clip_ratio: None,
            highlight_clip_ratio: None, colorfulness: Some(0.34), colorfulness_p25: Some(0.12),
            colorfulness_p75: Some(0.54),
            hue_color_distribution: None,
        };

        let (saturation, vibrance) =
            reference_relative_color_adjustments(&reference, &target);
        assert_eq!(saturation, Some(0.0));
        assert!(vibrance.unwrap() > 0.0);
    }

    #[test]
    fn coordinated_color_adjustment_prefers_desaturation_without_muted_deficit() {
        let reference = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(), exposure_ev: 0.0, temperature_k: None, tint: None,
            confidence: 1.0, luminance_p02: None, luminance_p10: None, luminance_p50: None,
            luminance_p90: None, luminance_p98: None, shadow_clip_ratio: None,
            highlight_clip_ratio: None, colorfulness: Some(0.20), colorfulness_p25: Some(0.10),
            colorfulness_p75: Some(0.35),
            hue_color_distribution: None,
        };
        let target = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(), exposure_ev: 0.0, temperature_k: None, tint: None,
            confidence: 1.0, luminance_p02: None, luminance_p10: None, luminance_p50: None,
            luminance_p90: None, luminance_p98: None, shadow_clip_ratio: None,
            highlight_clip_ratio: None, colorfulness: Some(0.42), colorfulness_p25: Some(0.18),
            colorfulness_p75: Some(0.60),
            hue_color_distribution: None,
        };

        let (saturation, vibrance) =
            reference_relative_color_adjustments(&reference, &target);
        assert!(saturation.unwrap() < 0.0);
        assert_eq!(vibrance, Some(0.0));
    }

    #[test]
    fn mixed_color_distribution_is_detected_as_global_control_conflict() {
        let reference = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(), exposure_ev: 0.0, temperature_k: None, tint: None,
            confidence: 1.0, luminance_p02: None, luminance_p10: None, luminance_p50: None,
            luminance_p90: None, luminance_p98: None, shadow_clip_ratio: None,
            highlight_clip_ratio: None, colorfulness: Some(0.30), colorfulness_p25: Some(0.20),
            colorfulness_p75: Some(0.48),
            hue_color_distribution: None,
        };
        let target = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(), exposure_ev: 0.0, temperature_k: None, tint: None,
            confidence: 1.0, luminance_p02: None, luminance_p10: None, luminance_p50: None,
            luminance_p90: None, luminance_p98: None, shadow_clip_ratio: None,
            highlight_clip_ratio: None, colorfulness: Some(0.34), colorfulness_p25: Some(0.10),
            colorfulness_p75: Some(0.68),
            hue_color_distribution: None,
        };

        assert!(reference_relative_color_distribution_conflict(&reference, &target));
    }

    #[test]
    fn selective_color_mixer_targets_conflicting_blue_tail_without_global_guess() {
        let mut reference = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(), exposure_ev: 0.0, temperature_k: None, tint: None,
            confidence: 1.0, luminance_p02: None, luminance_p10: None, luminance_p50: None,
            luminance_p90: None, luminance_p98: None, shadow_clip_ratio: None,
            highlight_clip_ratio: None, colorfulness: Some(0.30), colorfulness_p25: Some(0.20),
            colorfulness_p75: Some(0.48), hue_color_distribution: None,
        };
        let mut target = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(), exposure_ev: 0.0, temperature_k: None, tint: None,
            confidence: 1.0, luminance_p02: None, luminance_p10: None, luminance_p50: None,
            luminance_p90: None, luminance_p98: None, shadow_clip_ratio: None,
            highlight_clip_ratio: None, colorfulness: Some(0.34), colorfulness_p25: Some(0.10),
            colorfulness_p75: Some(0.68), hue_color_distribution: None,
        };
        let mut reference_hues = HueColorDistribution::default();
        reference_hues.blue = HueColorBin { coverage: 0.20, saturation: 0.45 };
        let mut target_hues = HueColorDistribution::default();
        target_hues.blue = HueColorBin { coverage: 0.22, saturation: 0.75 };
        reference.hue_color_distribution = Some(reference_hues);
        target.hue_color_distribution = Some(target_hues);

        let mixer = reference_relative_color_mixer_saturation(&reference, &target).unwrap();
        assert!(mixer.blue.unwrap() < -8.0);
        assert!(mixer.red.is_none());
        assert!(mixer.orange.is_none());
    }

    #[test]
    fn selective_color_mixer_requires_conflict_and_hue_evidence() {
        let mut reference = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(), exposure_ev: 0.0, temperature_k: None, tint: None,
            confidence: 1.0, luminance_p02: None, luminance_p10: None, luminance_p50: None,
            luminance_p90: None, luminance_p98: None, shadow_clip_ratio: None,
            highlight_clip_ratio: None, colorfulness: Some(0.30), colorfulness_p25: Some(0.20),
            colorfulness_p75: Some(0.48), hue_color_distribution: None,
        };
        let target = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(), exposure_ev: 0.0, temperature_k: None, tint: None,
            confidence: 1.0, luminance_p02: None, luminance_p10: None, luminance_p50: None,
            luminance_p90: None, luminance_p98: None, shadow_clip_ratio: None,
            highlight_clip_ratio: None, colorfulness: Some(0.31), colorfulness_p25: Some(0.19),
            colorfulness_p75: Some(0.50), hue_color_distribution: None,
        };
        reference.hue_color_distribution = Some(HueColorDistribution::default());
        assert!(reference_relative_color_mixer_saturation(&reference, &target).is_none());
    }

    #[test]
    fn selective_color_mixer_skips_hues_with_large_coverage_mismatch() {
        let mut reference = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(), exposure_ev: 0.0, temperature_k: None, tint: None,
            confidence: 1.0, luminance_p02: None, luminance_p10: None, luminance_p50: None,
            luminance_p90: None, luminance_p98: None, shadow_clip_ratio: None,
            highlight_clip_ratio: None, colorfulness: Some(0.30), colorfulness_p25: Some(0.20),
            colorfulness_p75: Some(0.48), hue_color_distribution: None,
        };
        let mut target = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(), exposure_ev: 0.0, temperature_k: None, tint: None,
            confidence: 1.0, luminance_p02: None, luminance_p10: None, luminance_p50: None,
            luminance_p90: None, luminance_p98: None, shadow_clip_ratio: None,
            highlight_clip_ratio: None, colorfulness: Some(0.34), colorfulness_p25: Some(0.10),
            colorfulness_p75: Some(0.68), hue_color_distribution: None,
        };
        let mut reference_hues = HueColorDistribution::default();
        reference_hues.blue = HueColorBin { coverage: 0.03, saturation: 0.45 };
        let mut target_hues = HueColorDistribution::default();
        target_hues.blue = HueColorBin { coverage: 0.20, saturation: 0.78 };
        reference.hue_color_distribution = Some(reference_hues);
        target.hue_color_distribution = Some(target_hues);

        assert!(reference_relative_color_mixer_saturation(&reference, &target).is_none());
    }

    #[test]
    fn selective_color_mixer_caps_positive_orange_lift_for_warm_subjects() {
        let mut reference = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(), exposure_ev: 0.0, temperature_k: None, tint: None,
            confidence: 1.0, luminance_p02: None, luminance_p10: None, luminance_p50: None,
            luminance_p90: None, luminance_p98: None, shadow_clip_ratio: None,
            highlight_clip_ratio: None, colorfulness: Some(0.30), colorfulness_p25: Some(0.20),
            colorfulness_p75: Some(0.48), hue_color_distribution: None,
        };
        let mut target = PhotoExposureAnalysis {
            asset_id: Uuid::new_v4(), exposure_ev: 0.0, temperature_k: None, tint: None,
            confidence: 1.0, luminance_p02: None, luminance_p10: None, luminance_p50: None,
            luminance_p90: None, luminance_p98: None, shadow_clip_ratio: None,
            highlight_clip_ratio: None, colorfulness: Some(0.34), colorfulness_p25: Some(0.10),
            colorfulness_p75: Some(0.68), hue_color_distribution: None,
        };
        let mut reference_hues = HueColorDistribution::default();
        reference_hues.orange = HueColorBin { coverage: 0.25, saturation: 0.85 };
        let mut target_hues = HueColorDistribution::default();
        target_hues.orange = HueColorBin { coverage: 0.25, saturation: 0.35 };
        reference.hue_color_distribution = Some(reference_hues);
        target.hue_color_distribution = Some(target_hues);

        let mixer = reference_relative_color_mixer_saturation(&reference, &target).unwrap();
        assert!((mixer.orange.unwrap() - 5.0).abs() < 0.001);
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
            hue_color_distribution: None,
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
            hue_color_distribution: None,
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
            hue_color_distribution: None,
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
