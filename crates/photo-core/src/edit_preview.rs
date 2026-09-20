use crate::{ColorMixerSaturation, Recipe};
use image::{DynamicImage, ImageBuffer, Rgb};
use std::path::Path;
use thiserror::Error;

const PREVIEW_LONG_EDGE: u32 = 1200;

#[derive(Debug, Error)]
pub enum EditPreviewError {
    #[error("failed to decode preview image: {0}")]
    Decode(String),
    #[error("failed to write edited preview image: {0}")]
    Write(String),
}

/// Render a lightweight visual approximation of Recipe adjustments against an
/// already-extracted RAW preview.
///
/// This deliberately does not claim RAW-engine parity. It exists so the
/// photographer can inspect direction and exceptions before Lightroom handoff.
/// Unsupported/unknown adjustments remain untouched.
pub fn render_recipe_preview(
    source_preview: &Path,
    destination: &Path,
    recipe: &Recipe,
) -> Result<(), EditPreviewError> {
    let image = image::open(source_preview)
        .map_err(|error| EditPreviewError::Decode(error.to_string()))?;
    let image = constrain_long_edge(image, PREVIEW_LONG_EDGE);
    let edited = apply_preview_adjustments(image, recipe);

    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| EditPreviewError::Write(error.to_string()))?;
    }

    edited
        .save_with_format(destination, image::ImageFormat::Jpeg)
        .map_err(|error| EditPreviewError::Write(error.to_string()))
}

pub fn apply_preview_adjustments(image: DynamicImage, recipe: &Recipe) -> DynamicImage {
    let exposure = recipe.adjustments.exposure.unwrap_or(0.0).clamp(-5.0, 5.0);
    let contrast = recipe.adjustments.contrast.unwrap_or(0.0).clamp(-100.0, 100.0);
    let highlights = recipe.adjustments.highlights.unwrap_or(0.0).clamp(-100.0, 100.0);
    let shadows = recipe.adjustments.shadows.unwrap_or(0.0).clamp(-100.0, 100.0);
    let whites = recipe.adjustments.whites.unwrap_or(0.0).clamp(-100.0, 100.0);
    let blacks = recipe.adjustments.blacks.unwrap_or(0.0).clamp(-100.0, 100.0);
    let saturation = recipe
        .adjustments
        .saturation
        .unwrap_or(0.0)
        .clamp(-100.0, 100.0);
    let vibrance = recipe
        .adjustments
        .vibrance
        .unwrap_or(0.0)
        .clamp(-100.0, 100.0);

    if exposure.abs() < 0.0001
        && contrast.abs() < 0.0001
        && highlights.abs() < 0.0001
        && shadows.abs() < 0.0001
        && whites.abs() < 0.0001
        && blacks.abs() < 0.0001
        && saturation.abs() < 0.0001
        && vibrance.abs() < 0.0001
    {
        return image;
    }

    let source = image.to_rgb8();
    let (width, height) = source.dimensions();
    let exposure_factor = 2.0_f32.powf(exposure);
    let contrast_factor = 1.0 + contrast / 100.0;
    let saturation_factor = 1.0 + saturation / 100.0;

    let mut output = ImageBuffer::<Rgb<u8>, Vec<u8>>::new(width, height);
    for (x, y, pixel) in source.enumerate_pixels() {
        let mut rgb = [
            f32::from(pixel[0]) / 255.0,
            f32::from(pixel[1]) / 255.0,
            f32::from(pixel[2]) / 255.0,
        ];

        let mut linear_rgb = [
            srgb_to_linear(rgb[0]) * exposure_factor,
            srgb_to_linear(rgb[1]) * exposure_factor,
            srgb_to_linear(rgb[2]) * exposure_factor,
        ];
        preserve_linear_highlight_ratios(&mut linear_rgb);
        for (channel, linear) in rgb.iter_mut().zip(linear_rgb) {
            *channel = linear_to_srgb(linear.clamp(0.0, 1.0));
        }

        let pre_contrast_luminance =
            (rgb[0] * 0.2126) + (rgb[1] * 0.7152) + (rgb[2] * 0.0722);
        let contrast_target =
            ((pre_contrast_luminance - 0.5) * contrast_factor + 0.5).clamp(0.0, 1.0);
        remap_luminance_preserving_hue(
            &mut rgb,
            pre_contrast_luminance,
            contrast_target - pre_contrast_luminance,
        );

        let luminance = (rgb[0] * 0.2126) + (rgb[1] * 0.7152) + (rgb[2] * 0.0722);
        let shadow_mask = 1.0 - smoothstep(0.08, 0.68, luminance);
        let highlight_mask = smoothstep(0.32, 0.92, luminance);
        let tone_delta =
            (shadows / 100.0) * shadow_mask * 0.28
            + (highlights / 100.0) * highlight_mask * 0.28;
        remap_luminance_preserving_hue(&mut rgb, luminance, tone_delta);

        let post_tone_luminance =
            (rgb[0] * 0.2126) + (rgb[1] * 0.7152) + (rgb[2] * 0.0722);
        let black_mask = 1.0 - smoothstep(0.02, 0.38, post_tone_luminance);
        let white_mask = smoothstep(0.62, 0.98, post_tone_luminance);
        let endpoint_delta =
            (blacks / 100.0) * black_mask * 0.18
            + (whites / 100.0) * white_mask * 0.18;
        remap_luminance_preserving_hue(&mut rgb, post_tone_luminance, endpoint_delta);

        let toned_luminance = (rgb[0] * 0.2126) + (rgb[1] * 0.7152) + (rgb[2] * 0.0722);
        let max_channel = rgb[0].max(rgb[1]).max(rgb[2]);
        let min_channel = rgb[0].min(rgb[1]).min(rgb[2]);
        let pixel_saturation = if max_channel <= 1e-6 {
            0.0
        } else {
            ((max_channel - min_channel) / max_channel).clamp(0.0, 1.0)
        };
        let vibrance_weight = if vibrance >= 0.0 {
            (1.0 - pixel_saturation).powf(1.25)
        } else {
            0.6 + 0.4 * (1.0 - pixel_saturation)
        };
        let vibrance_factor = (1.0 + (vibrance / 100.0) * vibrance_weight).max(0.0);
        scale_chroma_with_headroom(&mut rgb, toned_luminance, vibrance_factor);
        let saturation_luminance =
            (rgb[0] * 0.2126) + (rgb[1] * 0.7152) + (rgb[2] * 0.0722);
        scale_chroma_with_headroom(&mut rgb, saturation_luminance, saturation_factor);

        if let Some(mixer) = recipe.adjustments.color_mixer_saturation.as_ref() {
            if let Some(hue) = preview_hue_degrees(rgb) {
                let adjustment = color_mixer_saturation_for_hue(mixer, hue);
                if adjustment.abs() >= 0.01 {
                    let mixer_luminance =
                        (rgb[0] * 0.2126) + (rgb[1] * 0.7152) + (rgb[2] * 0.0722);
                    scale_chroma_with_headroom(
                        &mut rgb,
                        mixer_luminance,
                        (1.0 + adjustment / 100.0).max(0.0),
                    );
                }
            }
        }

        output.put_pixel(
            x,
            y,
            Rgb([
                (rgb[0] * 255.0).round() as u8,
                (rgb[1] * 255.0).round() as u8,
                (rgb[2] * 255.0).round() as u8,
            ]),
        );
    }

    DynamicImage::ImageRgb8(output)
}

fn preview_hue_degrees(rgb: [f32; 3]) -> Option<f32> {
    let max = rgb[0].max(rgb[1]).max(rgb[2]);
    let min = rgb[0].min(rgb[1]).min(rgb[2]);
    let delta = max - min;
    if max <= 1e-6 || delta / max < 0.05 {
        return None;
    }
    let mut hue = if (max - rgb[0]).abs() < 1e-6 {
        60.0 * (((rgb[1] - rgb[2]) / delta) % 6.0)
    } else if (max - rgb[1]).abs() < 1e-6 {
        60.0 * (((rgb[2] - rgb[0]) / delta) + 2.0)
    } else {
        60.0 * (((rgb[0] - rgb[1]) / delta) + 4.0)
    };
    if hue < 0.0 {
        hue += 360.0;
    }
    Some(hue)
}

fn color_mixer_saturation_for_hue(mixer: &ColorMixerSaturation, hue: f32) -> f32 {
    let centers = [0.0f32, 30.0, 60.0, 120.0, 180.0, 240.0, 270.0, 300.0];
    let values = [
        mixer.red.unwrap_or(0.0),
        mixer.orange.unwrap_or(0.0),
        mixer.yellow.unwrap_or(0.0),
        mixer.green.unwrap_or(0.0),
        mixer.aqua.unwrap_or(0.0),
        mixer.blue.unwrap_or(0.0),
        mixer.purple.unwrap_or(0.0),
        mixer.magenta.unwrap_or(0.0),
    ];
    let hue = hue.rem_euclid(360.0);
    for index in 0..centers.len() {
        let start = centers[index];
        let end = if index + 1 < centers.len() {
            centers[index + 1]
        } else {
            360.0
        };
        if hue >= start && hue <= end {
            let next_index = (index + 1) % centers.len();
            let width = (end - start).max(1e-6);
            let t = ((hue - start) / width).clamp(0.0, 1.0);
            return values[index] * (1.0 - t) + values[next_index] * t;
        }
    }
    values[0]
}

fn preserve_linear_highlight_ratios(rgb: &mut [f32; 3]) {
    let peak = rgb[0].max(rgb[1]).max(rgb[2]);
    if peak <= 1.0 {
        return;
    }
    let scale = 1.0 / peak.max(1e-6);
    for channel in rgb {
        *channel *= scale;
    }
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    if edge1 <= edge0 {
        return if value >= edge1 { 1.0 } else { 0.0 };
    }
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn scale_chroma_with_headroom(rgb: &mut [f32; 3], luminance: f32, requested_factor: f32) {
    if (requested_factor - 1.0).abs() < 1e-6 {
        return;
    }

    let mut factor = requested_factor.max(0.0);
    if factor > 1.0 {
        for channel in rgb.iter() {
            let delta = *channel - luminance;
            if delta > 1e-6 {
                factor = factor.min((1.0 - luminance) / delta);
            } else if delta < -1e-6 {
                factor = factor.min(luminance / -delta);
            }
        }
    }

    for channel in rgb {
        *channel = (luminance + (*channel - luminance) * factor).clamp(0.0, 1.0);
    }
}

fn remap_luminance_preserving_hue(rgb: &mut [f32; 3], luminance: f32, delta: f32) {
    let target = (luminance + delta).clamp(0.0, 1.0);
    if (target - luminance).abs() < 1e-6 {
        return;
    }

    if target > luminance {
        let denominator = (1.0 - luminance).max(1e-6);
        let amount = ((target - luminance) / denominator).clamp(0.0, 1.0);
        for channel in rgb {
            *channel = (*channel + (1.0 - *channel) * amount).clamp(0.0, 1.0);
        }
    } else {
        let scale = if luminance <= 1e-6 {
            0.0
        } else {
            (target / luminance).clamp(0.0, 1.0)
        };
        for channel in rgb {
            *channel = (*channel * scale).clamp(0.0, 1.0);
        }
    }
}

fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(value: f32) -> f32 {
    if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

fn constrain_long_edge(image: DynamicImage, edge: u32) -> DynamicImage {
    let width = image.width();
    let height = image.height();
    let current = width.max(height);
    if current <= edge {
        return image;
    }

    let scale = edge as f32 / current as f32;
    image.resize(
        (width as f32 * scale).round().max(1.0) as u32,
        (height as f32 * scale).round().max(1.0) as u32,
        image::imageops::FilterType::Lanczos3,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EditAdjustments, Recipe};
    use tempfile::tempdir;
    use uuid::Uuid;

    fn recipe_with_tone(
        exposure: f32,
        contrast: f32,
        highlights: f32,
        shadows: f32,
        saturation: f32,
    ) -> Recipe {
        Recipe {
            id: Uuid::new_v4(),
            name: "preview".into(),
            target_asset_id: Some(Uuid::new_v4()),
            source_reference_ids: Vec::new(),
            adjustments: EditAdjustments {
                exposure: Some(exposure),
                contrast: Some(contrast),
                highlights: Some(highlights),
                shadows: Some(shadows),
                whites: None,
                blacks: None,
                temperature: None,
                tint: None,
                saturation: Some(saturation),
                vibrance: None,
                color_mixer_saturation: None,
            },
        }
    }

    fn recipe(exposure: f32, contrast: f32, saturation: f32) -> Recipe {
        recipe_with_tone(exposure, contrast, 0.0, 0.0, saturation)
    }

    fn recipe_with_endpoints(whites: f32, blacks: f32) -> Recipe {
        let mut recipe = recipe_with_tone(0.0, 0.0, 0.0, 0.0, 0.0);
        recipe.adjustments.whites = Some(whites);
        recipe.adjustments.blacks = Some(blacks);
        recipe
    }

    #[test]
    fn neutral_preview_keeps_pixels_unchanged() {
        let source = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(
            8,
            8,
            Rgb([80, 120, 160]),
        ));
        let edited = apply_preview_adjustments(source.clone(), &recipe(0.0, 0.0, 0.0));
        assert_eq!(source.to_rgb8(), edited.to_rgb8());
    }

    #[test]
    fn positive_exposure_brightens_preview() {
        let source = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(
            8,
            8,
            Rgb([64, 64, 64]),
        ));
        let edited = apply_preview_adjustments(source, &recipe(1.0, 0.0, 0.0));
        assert!(edited.to_rgb8().get_pixel(0, 0)[0] > 64);
    }

    #[test]
    fn one_stop_exposure_uses_linear_light_instead_of_gamma_doubling() {
        let source = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(
            1,
            1,
            Rgb([128, 128, 128]),
        ));
        let edited = apply_preview_adjustments(source, &recipe(1.0, 0.0, 0.0)).to_rgb8();
        let value = edited.get_pixel(0, 0)[0];
        assert!((170..=180).contains(&value));
    }

    #[test]
    fn srgb_linear_round_trip_is_stable() {
        for value in [0.0, 0.02, 0.18, 0.5, 0.9, 1.0] {
            let round_trip = linear_to_srgb(srgb_to_linear(value));
            assert!((round_trip - value).abs() < 1e-5);
        }
    }

    #[test]
    fn positive_exposure_preserves_colored_highlight_channel_order() {
        let source = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(
            1,
            1,
            Rgb([240, 150, 70]),
        ));
        let edited = apply_preview_adjustments(source, &recipe(1.0, 0.0, 0.0)).to_rgb8();
        let pixel = edited.get_pixel(0, 0).0;
        assert!(pixel[0] > pixel[1] && pixel[1] > pixel[2]);
        assert!(pixel[1] < 230);
    }

    #[test]
    fn contrast_preserves_colored_midpoint_channel_order() {
        let source = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(
            1,
            1,
            Rgb([170, 110, 70]),
        ));
        let edited = apply_preview_adjustments(source, &recipe(0.0, 40.0, 0.0)).to_rgb8();
        let pixel = edited.get_pixel(0, 0).0;
        assert!(pixel[0] > pixel[1] && pixel[1] > pixel[2]);
    }

    #[test]
    fn smooth_tone_masks_are_monotonic_across_luminance() {
        let shadow_dark = 1.0 - smoothstep(0.08, 0.68, 0.15);
        let shadow_mid = 1.0 - smoothstep(0.08, 0.68, 0.45);
        let shadow_bright = 1.0 - smoothstep(0.08, 0.68, 0.80);
        assert!(shadow_dark > shadow_mid && shadow_mid > shadow_bright);

        let highlight_dark = smoothstep(0.32, 0.92, 0.15);
        let highlight_mid = smoothstep(0.32, 0.92, 0.60);
        let highlight_bright = smoothstep(0.32, 0.92, 0.90);
        assert!(highlight_dark < highlight_mid && highlight_mid < highlight_bright);
    }

    #[test]
    fn positive_shadows_lift_dark_pixels_more_than_bright_pixels() {
        let source = DynamicImage::ImageRgb8(ImageBuffer::from_fn(2, 1, |x, _| {
            if x == 0 { Rgb([30, 30, 30]) } else { Rgb([200, 200, 200]) }
        }));
        let edited = apply_preview_adjustments(
            source,
            &recipe_with_tone(0.0, 0.0, 0.0, 60.0, 0.0),
        ).to_rgb8();
        let dark_gain = i16::from(edited.get_pixel(0, 0)[0]) - 30;
        let bright_gain = i16::from(edited.get_pixel(1, 0)[0]) - 200;
        assert!(dark_gain > bright_gain);
    }

    #[test]
    fn negative_highlights_recover_bright_pixels_more_than_dark_pixels() {
        let source = DynamicImage::ImageRgb8(ImageBuffer::from_fn(2, 1, |x, _| {
            if x == 0 { Rgb([40, 40, 40]) } else { Rgb([230, 230, 230]) }
        }));
        let edited = apply_preview_adjustments(
            source,
            &recipe_with_tone(0.0, 0.0, -60.0, 0.0, 0.0),
        ).to_rgb8();
        let dark_drop = 40 - i16::from(edited.get_pixel(0, 0)[0]);
        let bright_drop = 230 - i16::from(edited.get_pixel(1, 0)[0]);
        assert!(bright_drop > dark_drop);
    }

    #[test]
    fn negative_highlights_preserve_colored_highlight_channel_ratios() {
        let source = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(
            1,
            1,
            Rgb([240, 160, 80]),
        ));
        let edited = apply_preview_adjustments(
            source,
            &recipe_with_tone(0.0, 0.0, -50.0, 0.0, 0.0),
        )
        .to_rgb8();
        let pixel = edited.get_pixel(0, 0).0;
        let rg = f32::from(pixel[0]) / f32::from(pixel[1].max(1));
        let gb = f32::from(pixel[1]) / f32::from(pixel[2].max(1));
        assert!((rg - 1.5).abs() < 0.08);
        assert!((gb - 2.0).abs() < 0.12);
    }

    #[test]
    fn lifted_colored_shadows_keep_channel_order() {
        let source = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(
            1,
            1,
            Rgb([70, 35, 18]),
        ));
        let edited = apply_preview_adjustments(
            source,
            &recipe_with_tone(0.0, 0.0, 0.0, 60.0, 0.0),
        )
        .to_rgb8();
        let pixel = edited.get_pixel(0, 0).0;
        assert!(pixel[0] > pixel[1] && pixel[1] > pixel[2]);
    }

    #[test]
    fn whites_target_bright_pixels_more_than_midtones() {
        let source = DynamicImage::ImageRgb8(ImageBuffer::from_fn(2, 1, |x, _| {
            if x == 0 { Rgb([128, 128, 128]) } else { Rgb([230, 230, 230]) }
        }));
        let edited = apply_preview_adjustments(source, &recipe_with_endpoints(50.0, 0.0))
            .to_rgb8();
        let mid_gain = i16::from(edited.get_pixel(0, 0)[0]) - 128;
        let bright_gain = i16::from(edited.get_pixel(1, 0)[0]) - 230;
        assert!(bright_gain > mid_gain);
    }

    #[test]
    fn blacks_target_dark_pixels_more_than_midtones() {
        let source = DynamicImage::ImageRgb8(ImageBuffer::from_fn(2, 1, |x, _| {
            if x == 0 { Rgb([25, 25, 25]) } else { Rgb([128, 128, 128]) }
        }));
        let edited = apply_preview_adjustments(source, &recipe_with_endpoints(0.0, 50.0))
            .to_rgb8();
        let dark_gain = i16::from(edited.get_pixel(0, 0)[0]) - 25;
        let mid_gain = i16::from(edited.get_pixel(1, 0)[0]) - 128;
        assert!(dark_gain > mid_gain);
    }

    #[test]
    fn positive_vibrance_boosts_muted_color_more_than_saturated_color() {
        let source = DynamicImage::ImageRgb8(ImageBuffer::from_fn(2, 1, |x, _| {
            if x == 0 { Rgb([150, 130, 120]) } else { Rgb([220, 40, 40]) }
        }));
        let mut edit = recipe(0.0, 0.0, 0.0);
        edit.adjustments.vibrance = Some(50.0);
        let edited = apply_preview_adjustments(source, &edit).to_rgb8();

        let muted = edited.get_pixel(0, 0).0;
        let saturated = edited.get_pixel(1, 0).0;
        let muted_gain =
            (i16::from(muted[0]) - i16::from(muted[2])) - 30;
        let saturated_gain =
            (i16::from(saturated[0]) - i16::from(saturated[1])) - 180;
        assert!(muted_gain > 0);
        assert!(muted_gain > saturated_gain);
    }

    #[test]
    fn strong_positive_vibrance_does_not_clip_muted_color_channels() {
        let source = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(
            1,
            1,
            Rgb([210, 185, 170]),
        ));
        let mut edit = recipe(0.0, 0.0, 0.0);
        edit.adjustments.vibrance = Some(100.0);
        let edited = apply_preview_adjustments(source, &edit).to_rgb8();
        let pixel = edited.get_pixel(0, 0).0;
        assert!(pixel.iter().all(|value| *value < 255));
        assert!(pixel[0] > pixel[1] && pixel[1] > pixel[2]);
    }

    #[test]
    fn selective_blue_saturation_changes_blue_more_than_orange() {
        let source = DynamicImage::ImageRgb8(ImageBuffer::from_fn(2, 1, |x, _| {
            if x == 0 { Rgb([40, 80, 220]) } else { Rgb([220, 120, 40]) }
        }));
        let mut edit = recipe(0.0, 0.0, 0.0);
        edit.adjustments.color_mixer_saturation = Some(ColorMixerSaturation {
            blue: Some(-40.0),
            ..ColorMixerSaturation::default()
        });
        let edited = apply_preview_adjustments(source, &edit).to_rgb8();

        let blue = edited.get_pixel(0, 0).0;
        let orange = edited.get_pixel(1, 0).0;
        let blue_range = i16::from(*blue.iter().max().unwrap()) - i16::from(*blue.iter().min().unwrap());
        let orange_range = i16::from(*orange.iter().max().unwrap()) - i16::from(*orange.iter().min().unwrap());
        assert!(blue_range < 150);
        assert!(orange_range > blue_range);
    }

    #[test]
    fn color_mixer_interpolates_across_adjacent_hue_centers() {
        let mixer = ColorMixerSaturation {
            red: Some(0.0),
            orange: Some(20.0),
            ..ColorMixerSaturation::default()
        };
        let middle = color_mixer_saturation_for_hue(&mixer, 15.0);
        assert!((middle - 10.0).abs() < 0.01);
    }

    #[test]
    fn negative_saturation_moves_color_toward_gray() {
        let source = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(
            8,
            8,
            Rgb([220, 40, 40]),
        ));
        let edited = apply_preview_adjustments(source, &recipe(0.0, 0.0, -100.0));
        let pixel = edited.to_rgb8().get_pixel(0, 0).0;
        assert!((i16::from(pixel[0]) - i16::from(pixel[1])).abs() <= 1);
        assert!((i16::from(pixel[1]) - i16::from(pixel[2])).abs() <= 1);
    }

    #[test]
    fn rendered_preview_is_small_cached_jpeg() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("source.jpg");
        let destination = dir.path().join("cache").join("edited.jpg");
        DynamicImage::ImageRgb8(ImageBuffer::from_pixel(
            1800,
            1200,
            Rgb([100, 120, 140]),
        ))
        .save(&source)
        .unwrap();

        render_recipe_preview(&source, &destination, &recipe(0.3, 5.0, 2.0)).unwrap();

        let edited = image::open(destination).unwrap();
        assert_eq!(edited.width().max(edited.height()), PREVIEW_LONG_EDGE);
    }
}
