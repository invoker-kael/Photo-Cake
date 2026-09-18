use crate::{ExportRenderer, ExportWorkerError};
use image::{DynamicImage, ImageFormat};
use std::path::Path;

/// Baseline raster renderer.
///
/// This stage materializes already-decoded images. RAW demosaic and
/// non-destructive edit evaluation remain separate stages.
pub struct ImageExportRenderer {
    pub jpeg_quality: u8,
}

impl Default for ImageExportRenderer {
    fn default() -> Self {
        Self { jpeg_quality: 92 }
    }
}

impl ExportRenderer for ImageExportRenderer {
    fn render(&self, source: &Path, destination: &Path) -> Result<(), ExportWorkerError> {
        let image = image::open(source)
            .map_err(|error| ExportWorkerError::Render(error.to_string()))?;

        let format = output_format(destination);
        write_image(image, destination, format, self.jpeg_quality)
    }
}

fn output_format(destination: &Path) -> ImageFormat {
    match destination
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => ImageFormat::Jpeg,
        "tif" | "tiff" => ImageFormat::Tiff,
        _ => ImageFormat::Png,
    }
}

fn write_image(
    image: DynamicImage,
    destination: &Path,
    format: ImageFormat,
    quality: u8,
) -> Result<(), ExportWorkerError> {
    let mut file = std::fs::File::create(destination)
        .map_err(|error| ExportWorkerError::Output(error.to_string()))?;

    match format {
        ImageFormat::Jpeg => {
            let quality = quality.clamp(1, 100);
            let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut file, quality);
            image
                .write_with_encoder(encoder)
                .map_err(|error| ExportWorkerError::Render(error.to_string()))?;
        }
        _ => {
            image
                .write_to(&mut file, format)
                .map_err(|error| ExportWorkerError::Render(error.to_string()))?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};
    use tempfile::tempdir;

    #[test]
    fn renderer_writes_jpeg_output() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("source.png");
        let output = dir.path().join("output.jpg");

        let mut image = RgbImage::new(2, 2);
        image.put_pixel(0, 0, Rgb([255, 0, 0]));
        image.save(&source).unwrap();

        ImageExportRenderer::default()
            .render(&source, &output)
            .unwrap();

        assert!(output.metadata().unwrap().len() > 0);
    }
}
