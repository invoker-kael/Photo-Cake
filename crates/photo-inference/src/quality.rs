use image::{DynamicImage, GrayImage};
use photo_core::CullingScore;

/// Deterministic technical quality evidence derived from the local preview.
///
/// This deliberately scores only evidence we can measure reliably here.
/// Expression/composition remain unknown until dedicated semantic evidence is
/// available, so the culling layer does not fabricate photographer judgment.
pub fn score_image_quality(image: &DynamicImage) -> CullingScore {
    let gray = image.thumbnail(384, 384).to_luma8();
    let sharpness = normalized_laplacian_variance(&gray);
    let exposure = exposure_score(&gray);

    CullingScore {
        sharpness,
        blur_penalty: (1.0 - sharpness).clamp(0.0, 1.0),
        exposure,
        expression: None,
        duplicate_similarity: None,
        composition: None,
    }
}

fn normalized_laplacian_variance(gray: &GrayImage) -> f32 {
    if gray.width() < 3 || gray.height() < 3 {
        return 0.0;
    }

    let mut count = 0.0f32;
    let mut sum = 0.0f32;
    let mut sum_sq = 0.0f32;

    for y in 1..gray.height() - 1 {
        for x in 1..gray.width() - 1 {
            let center = f32::from(gray.get_pixel(x, y)[0]);
            let laplacian =
                4.0 * center
                - f32::from(gray.get_pixel(x - 1, y)[0])
                - f32::from(gray.get_pixel(x + 1, y)[0])
                - f32::from(gray.get_pixel(x, y - 1)[0])
                - f32::from(gray.get_pixel(x, y + 1)[0]);
            count += 1.0;
            sum += laplacian;
            sum_sq += laplacian * laplacian;
        }
    }

    if count == 0.0 {
        return 0.0;
    }

    let mean = sum / count;
    let variance = (sum_sq / count - mean * mean).max(0.0);

    // Saturating normalization keeps this deterministic while avoiding a hard
    // camera-specific sharpness threshold. Later calibration can tune the scale
    // without changing the CullingScore contract.
    (variance / (variance + 1_200.0)).clamp(0.0, 1.0)
}

fn exposure_score(gray: &GrayImage) -> f32 {
    let pixels = gray.pixels().map(|pixel| pixel[0]).collect::<Vec<_>>();
    if pixels.is_empty() {
        return 0.0;
    }

    let total = pixels.len() as f32;
    let clipped_dark = pixels.iter().filter(|value| **value <= 4).count() as f32 / total;
    let clipped_light = pixels.iter().filter(|value| **value >= 251).count() as f32 / total;
    let mean = pixels.iter().map(|value| f32::from(*value)).sum::<f32>() / total / 255.0;

    let clipping_penalty = ((clipped_dark + clipped_light) * 3.0).clamp(0.0, 1.0);
    let extreme_mean_penalty = if mean < 0.05 {
        ((0.05 - mean) / 0.05).clamp(0.0, 1.0)
    } else if mean > 0.95 {
        ((mean - 0.95) / 0.05).clamp(0.0, 1.0)
    } else {
        0.0
    };

    (1.0 - clipping_penalty.max(extreme_mean_penalty)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Luma};

    #[test]
    fn detailed_pattern_scores_sharper_than_flat_frame() {
        let flat = DynamicImage::ImageLuma8(ImageBuffer::from_pixel(128, 128, Luma([128])));
        let detailed = DynamicImage::ImageLuma8(ImageBuffer::from_fn(128, 128, |x, y| {
            if (x / 4 + y / 4) % 2 == 0 {
                Luma([24])
            } else {
                Luma([232])
            }
        }));

        assert!(
            score_image_quality(&detailed).sharpness
                > score_image_quality(&flat).sharpness
        );
    }

    #[test]
    fn fully_clipped_frame_has_bad_exposure_score() {
        let clipped = DynamicImage::ImageLuma8(ImageBuffer::from_pixel(64, 64, Luma([255])));
        assert_eq!(score_image_quality(&clipped).exposure, 0.0);
    }

    #[test]
    fn semantic_scores_remain_unknown() {
        let image = DynamicImage::ImageLuma8(ImageBuffer::from_pixel(64, 64, Luma([128])));
        let score = score_image_quality(&image);
        assert!(score.expression.is_none());
        assert!(score.composition.is_none());
        assert!(score.duplicate_similarity.is_none());
    }
}
