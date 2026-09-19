use crate::Recipe;
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
    let saturation = recipe
        .adjustments
        .saturation
        .unwrap_or(0.0)
        .clamp(-100.0, 100.0);

    if exposure.abs() < 0.0001
        && contrast.abs() < 0.0001
        && highlights.abs() < 0.0001
        && shadows.abs() < 0.0001
        && saturation.abs() < 0.0001
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

        for channel in &mut rgb {
            let linear = srgb_to_linear(*channel);
            *channel = linear_to_srgb((linear * exposure_factor).clamp(0.0, 1.0));
            *channel = ((*channel - 0.5) * contrast_factor + 0.5).clamp(0.0, 1.0);
        }

        let luminance = (rgb[0] * 0.2126) + (rgb[1] * 0.7152) + (rgb[2] * 0.0722);
        let shadow_mask = (1.0 - luminance).powi(2);
        let highlight_mask = luminance.powi(2);
        let tone_delta =
            (shadows / 100.0) * shadow_mask * 0.28
            + (highlights / 100.0) * highlight_mask * 0.28;
        remap_luminance_preserving_hue(&mut rgb, luminance, tone_delta);

        let toned_luminance = (rgb[0] * 0.2126) + (rgb[1] * 0.7152) + (rgb[2] * 0.0722);
        for channel in &mut rgb {
            *channel = (toned_luminance + (*channel - toned_luminance) * saturation_factor)
                .clamp(0.0, 1.0);
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
                temperature: None,
                tint: None,
                saturation: Some(saturation),
            },
        }
    }

    fn recipe(exposure: f32, contrast: f32, saturation: f32) -> Recipe {
        recipe_with_tone(exposure, contrast, 0.0, 0.0, saturation)
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
