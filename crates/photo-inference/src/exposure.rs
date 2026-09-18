use image::DynamicImage;
use photo_core::PhotoColorAnalysis;
use uuid::Uuid;

/// Measure a robust, preview-relative exposure signal.
///
/// Embedded JPEG previews have already passed through the camera's rendering
/// pipeline, so this value is intentionally used only as a relative group
/// signal. It is not camera-metering EV and it never claims RAW white balance.
pub fn analyze_preview_exposure(asset_id: Uuid, image: &DynamicImage) -> PhotoColorAnalysis {
    let gray = image.thumbnail(384, 384).to_luma8();
    let mut values = gray
        .pixels()
        .map(|pixel| pixel[0])
        .collect::<Vec<_>>();

    if values.is_empty() {
        return PhotoColorAnalysis {
            asset_id,
            exposure_ev: 0.0,
            temperature_k: None,
            tint: None,
            confidence: 0.0,
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

    let clipped = values
        .iter()
        .filter(|value| **value <= 4 || **value >= 251)
        .count() as f32
        / total as f32;
    let confidence = (1.0 - clipped * 2.5).clamp(0.15, 1.0);

    PhotoColorAnalysis {
        asset_id,
        exposure_ev,
        temperature_k: None,
        tint: None,
        confidence,
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
