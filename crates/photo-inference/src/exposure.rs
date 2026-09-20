use image::DynamicImage;
use photo_core::{HueColorBin, HueColorDistribution, PhotoExposureAnalysis};
use uuid::Uuid;

fn rgb_hue_degrees(red: u8, green: u8, blue: u8) -> Option<(f32, f32)> {
    let r = f32::from(red) / 255.0;
    let g = f32::from(green) / 255.0;
    let b = f32::from(blue) / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    if max < 16.0 / 255.0 || delta <= 1e-6 {
        return None;
    }
    let saturation = (delta / max).clamp(0.0, 1.0);
    if saturation < 0.05 {
        return None;
    }
    let mut hue = if (max - r).abs() < 1e-6 {
        60.0 * (((g - b) / delta) % 6.0)
    } else if (max - g).abs() < 1e-6 {
        60.0 * (((b - r) / delta) + 2.0)
    } else {
        60.0 * (((r - g) / delta) + 4.0)
    };
    if hue < 0.0 {
        hue += 360.0;
    }
    Some((hue, saturation))
}

fn hue_bin_index(hue: f32) -> usize {
    match hue {
        value if value < 15.0 || value >= 330.0 => 0, // red
        value if value < 45.0 => 1,                   // orange
        value if value < 90.0 => 2,                   // yellow
        value if value < 150.0 => 3,                  // green
        value if value < 210.0 => 4,                  // aqua
        value if value < 255.0 => 5,                  // blue
        value if value < 285.0 => 6,                  // purple
        _ => 7,                                       // magenta
    }
}

fn analyze_hue_distribution(rgb: &image::RgbImage) -> HueColorDistribution {
    let visible = rgb
        .pixels()
        .filter(|pixel| pixel[0].max(pixel[1]).max(pixel[2]) >= 16)
        .count()
        .max(1) as f32;
    let mut counts = [0usize; 8];
    let mut saturation_sum = [0.0f32; 8];

    for pixel in rgb.pixels() {
        let Some((hue, saturation)) = rgb_hue_degrees(pixel[0], pixel[1], pixel[2]) else {
            continue;
        };
        let index = hue_bin_index(hue);
        counts[index] += 1;
        saturation_sum[index] += saturation;
    }

    let bin = |index: usize| HueColorBin {
        coverage: counts[index] as f32 / visible,
        saturation: if counts[index] == 0 {
            0.0
        } else {
            saturation_sum[index] / counts[index] as f32
        },
    };

    HueColorDistribution {
        red: bin(0),
        orange: bin(1),
        yellow: bin(2),
        green: bin(3),
        aqua: bin(4),
        blue: bin(5),
        purple: bin(6),
        magenta: bin(7),
    }
}

/// Measure a robust, preview-relative exposure signal.
///
/// Embedded JPEG previews have already passed through the camera's rendering
/// pipeline, so this value is intentionally used only as a relative group
/// signal. It is not camera-metering EV and it never claims RAW white balance.
pub fn analyze_preview_exposure(asset_id: Uuid, image: &DynamicImage) -> PhotoExposureAnalysis {
    let thumbnail = image.thumbnail(384, 384);
    let gray = thumbnail.to_luma8();
    let rgb = thumbnail.to_rgb8();
    let mut values = gray
        .pixels()
        .map(|pixel| pixel[0])
        .collect::<Vec<_>>();

    if values.is_empty() {
        return PhotoExposureAnalysis {
            asset_id,
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 0.0,
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
    }

    values.sort_unstable();
    let total = values.len();
    let trim = (total / 20).min(total.saturating_sub(1) / 2);
    let usable = &values[trim..total - trim];
    let mean = usable
        .iter()
        .map(|value| f32::from(*value) / 255.0)
        .sum::<f32>()
        / usable.len().max(1) as f32;

    // 18% is only a stable normalization anchor. Differences between photos
    // are the useful signal; the absolute number must not be presented as
    // camera-metering EV.
    let exposure_ev = (mean.max(1.0 / 255.0) / 0.18).log2().clamp(-6.0, 6.0);

    let percentile = |fraction: f32| -> f32 {
        let index = ((total.saturating_sub(1)) as f32 * fraction)
            .round()
            .clamp(0.0, total.saturating_sub(1) as f32) as usize;
        f32::from(values[index]) / 255.0
    };
    let rgb_total = (u64::from(rgb.width()) * u64::from(rgb.height())).max(1) as f32;
    let shadow_clip_ratio = rgb
        .pixels()
        .filter(|pixel| pixel[0] <= 4 && pixel[1] <= 4 && pixel[2] <= 4)
        .count() as f32
        / rgb_total;
    let highlight_clip_ratio = rgb
        .pixels()
        .filter(|pixel| pixel[0] >= 251 || pixel[1] >= 251 || pixel[2] >= 251)
        .count() as f32
        / rgb_total;
    let clipped = shadow_clip_ratio + highlight_clip_ratio;
    let confidence = (1.0 - clipped * 2.5).clamp(0.15, 1.0);
    let mut chroma = Vec::new();
    for pixel in rgb.pixels() {
        let max = pixel[0].max(pixel[1]).max(pixel[2]);
        if max < 16 {
            continue;
        }
        let min = pixel[0].min(pixel[1]).min(pixel[2]);
        chroma.push(f32::from(max - min) / f32::from(max));
    }
    chroma.sort_by(|left, right| left.total_cmp(right));
    let colorfulness = (!chroma.is_empty())
        .then_some(chroma.iter().copied().sum::<f32>() / chroma.len() as f32);
    let chroma_percentile = |fraction: f32| -> Option<f32> {
        if chroma.is_empty() {
            return None;
        }
        let index = ((chroma.len().saturating_sub(1)) as f32 * fraction)
            .round()
            .clamp(0.0, chroma.len().saturating_sub(1) as f32) as usize;
        Some(chroma[index])
    };

    PhotoExposureAnalysis {
        asset_id,
        exposure_ev,
        temperature_k: None,
        tint: None,
        confidence,
        luminance_p02: Some(percentile(0.02)),
        luminance_p10: Some(percentile(0.10)),
        luminance_p50: Some(percentile(0.50)),
        luminance_p90: Some(percentile(0.90)),
        luminance_p98: Some(percentile(0.98)),
        shadow_clip_ratio: Some(shadow_clip_ratio),
        highlight_clip_ratio: Some(highlight_clip_ratio),
        colorfulness,
        colorfulness_p25: chroma_percentile(0.25),
        colorfulness_p75: chroma_percentile(0.75),
        hue_color_distribution: Some(analyze_hue_distribution(&rgb)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Luma};

    #[test]
    fn brighter_preview_has_higher_relative_exposure() {
        let asset = Uuid::new_v4();
        let dark = DynamicImage::ImageLuma8(ImageBuffer::from_pixel(64, 64, Luma([48])));
        let bright = DynamicImage::ImageLuma8(ImageBuffer::from_pixel(64, 64, Luma([160])));

        assert!(
            analyze_preview_exposure(asset, &bright).exposure_ev
                > analyze_preview_exposure(asset, &dark).exposure_ev
        );
    }

    #[test]
    fn analysis_records_tonal_percentiles_for_reference_matching() {
        let mut image = ImageBuffer::from_pixel(100, 1, Luma([128]));
        for x in 0..20 {
            image.put_pixel(x, 0, Luma([20]));
        }
        for x in 80..100 {
            image.put_pixel(x, 0, Luma([230]));
        }
        let analysis =
            analyze_preview_exposure(Uuid::new_v4(), &DynamicImage::ImageLuma8(image));
        assert!(analysis.luminance_p02.unwrap() <= analysis.luminance_p10.unwrap());
        assert!(analysis.luminance_p10.unwrap() < analysis.luminance_p50.unwrap());
        assert!(analysis.luminance_p90.unwrap() > analysis.luminance_p50.unwrap());
        assert!(analysis.luminance_p98.unwrap() >= analysis.luminance_p90.unwrap());
    }

    #[test]
    fn colorful_preview_has_more_colorfulness_than_gray_preview() {
        let gray = DynamicImage::ImageRgb8(image::ImageBuffer::from_pixel(
            64,
            64,
            image::Rgb([120, 120, 120]),
        ));
        let red = DynamicImage::ImageRgb8(image::ImageBuffer::from_pixel(
            64,
            64,
            image::Rgb([220, 40, 40]),
        ));
        let gray_value = analyze_preview_exposure(Uuid::new_v4(), &gray)
            .colorfulness
            .unwrap();
        let red_value = analyze_preview_exposure(Uuid::new_v4(), &red)
            .colorfulness
            .unwrap();
        assert!(red_value > gray_value);
    }

    #[test]
    fn colorfulness_distribution_separates_muted_and_saturated_pixels() {
        let image = DynamicImage::ImageRgb8(image::ImageBuffer::from_fn(100, 1, |x, _| {
            if x < 50 {
                image::Rgb([140, 120, 110])
            } else {
                image::Rgb([220, 40, 40])
            }
        }));
        let analysis = analyze_preview_exposure(Uuid::new_v4(), &image);
        assert!(analysis.colorfulness_p25.unwrap() < 0.25);
        assert!(analysis.colorfulness_p75.unwrap() > 0.70);
    }

    #[test]
    fn hue_distribution_separates_red_and_blue_regions() {
        let image = DynamicImage::ImageRgb8(image::ImageBuffer::from_fn(100, 1, |x, _| {
            if x < 60 {
                image::Rgb([220, 40, 40])
            } else {
                image::Rgb([40, 70, 220])
            }
        }));
        let analysis = analyze_preview_exposure(Uuid::new_v4(), &image);
        let hues = analysis.hue_color_distribution.unwrap();
        assert!(hues.red.coverage > 0.55);
        assert!(hues.blue.coverage > 0.35);
        assert!(hues.red.saturation > 0.70);
        assert!(hues.blue.saturation > 0.60);
    }

    #[test]
    fn gray_pixels_do_not_create_unstable_hue_evidence() {
        let gray = DynamicImage::ImageRgb8(image::ImageBuffer::from_pixel(
            64,
            64,
            image::Rgb([128, 128, 128]),
        ));
        let hues = analyze_preview_exposure(Uuid::new_v4(), &gray)
            .hue_color_distribution
            .unwrap();
        assert_eq!(hues.red.coverage, 0.0);
        assert_eq!(hues.blue.coverage, 0.0);
    }

    #[test]
    fn colorfulness_is_stable_across_exposure_for_same_rgb_ratio() {
        let bright = DynamicImage::ImageRgb8(image::ImageBuffer::from_pixel(
            64,
            64,
            image::Rgb([220, 40, 40]),
        ));
        let dark = DynamicImage::ImageRgb8(image::ImageBuffer::from_pixel(
            64,
            64,
            image::Rgb([110, 20, 20]),
        ));
        let bright_value = analyze_preview_exposure(Uuid::new_v4(), &bright)
            .colorfulness
            .unwrap();
        let dark_value = analyze_preview_exposure(Uuid::new_v4(), &dark)
            .colorfulness
            .unwrap();
        assert!((bright_value - dark_value).abs() < 0.01);
    }

    #[test]
    fn single_channel_clipping_is_detected_even_when_luma_is_not_white() {
        let image = DynamicImage::ImageRgb8(image::ImageBuffer::from_pixel(
            64,
            64,
            image::Rgb([255, 80, 80]),
        ));
        let analysis = analyze_preview_exposure(Uuid::new_v4(), &image);
        assert_eq!(analysis.highlight_clip_ratio, Some(1.0));
        assert!(analysis.luminance_p90.unwrap() < 0.8);
    }

    #[test]
    fn saturated_dark_color_is_not_misclassified_as_black_clip() {
        let image = DynamicImage::ImageRgb8(image::ImageBuffer::from_pixel(
            64,
            64,
            image::Rgb([0, 0, 24]),
        ));
        let analysis = analyze_preview_exposure(Uuid::new_v4(), &image);
        assert_eq!(analysis.shadow_clip_ratio, Some(0.0));
    }

    #[test]
    fn preview_analysis_never_invents_white_balance() {
        let analysis = analyze_preview_exposure(
            Uuid::new_v4(),
            &DynamicImage::ImageLuma8(ImageBuffer::from_pixel(64, 64, Luma([96]))),
        );
        assert!(analysis.temperature_k.is_none());
        assert!(analysis.tint.is_none());
    }

    #[test]
    fn heavy_clipping_reduces_confidence() {
        let normal = analyze_preview_exposure(
            Uuid::new_v4(),
            &DynamicImage::ImageLuma8(ImageBuffer::from_pixel(64, 64, Luma([96]))),
        );
        let clipped = analyze_preview_exposure(
            Uuid::new_v4(),
            &DynamicImage::ImageLuma8(ImageBuffer::from_pixel(64, 64, Luma([255]))),
        );
        assert!(normal.confidence > clipped.confidence);
    }
}
