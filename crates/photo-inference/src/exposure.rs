use image::DynamicImage;
use photo_core::PhotoExposureAnalysis;
use uuid::Uuid;

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
            luminance_p10: None,
            luminance_p50: None,
            luminance_p90: None,
            shadow_clip_ratio: None,
            highlight_clip_ratio: None,
            colorfulness: None,
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
    let shadow_clip_ratio = values.iter().filter(|value| **value <= 4).count() as f32 / total as f32;
    let highlight_clip_ratio =
        values.iter().filter(|value| **value >= 251).count() as f32 / total as f32;
    let clipped = shadow_clip_ratio + highlight_clip_ratio;
    let confidence = (1.0 - clipped * 2.5).clamp(0.15, 1.0);
    let mut chroma_sum = 0.0_f32;
    let mut chroma_count = 0_u32;
    for pixel in rgb.pixels() {
        let max = pixel[0].max(pixel[1]).max(pixel[2]);
        if max < 16 {
            continue;
        }
        let min = pixel[0].min(pixel[1]).min(pixel[2]);
        chroma_sum += f32::from(max - min) / f32::from(max);
        chroma_count += 1;
    }
    let colorfulness = (chroma_count > 0).then_some(chroma_sum / chroma_count as f32);

    PhotoExposureAnalysis {
        asset_id,
        exposure_ev,
        temperature_k: None,
        tint: None,
        confidence,
        luminance_p10: Some(percentile(0.10)),
        luminance_p50: Some(percentile(0.50)),
        luminance_p90: Some(percentile(0.90)),
        shadow_clip_ratio: Some(shadow_clip_ratio),
        highlight_clip_ratio: Some(highlight_clip_ratio),
        colorfulness,
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
        assert!(analysis.luminance_p10.unwrap() < analysis.luminance_p50.unwrap());
        assert!(analysis.luminance_p90.unwrap() > analysis.luminance_p50.unwrap());
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
