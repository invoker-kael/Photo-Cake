use crate::{ExportRenderer, ExportWorkerError};
use image::{DynamicImage, ImageFormat};
use std::path::Path;

/// Baseline full-resolution renderer.
///
/// This implementation intentionally keeps the edit graph boundary simple:
/// it provides deterministic image materialization for the export stage.
/// RAW demosaic and non-destructive edit evaluation are added above this layer.
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

        let format = match destination
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "jpg" | "jpeg" => ImageFormat::Jpeg,
            "tif" | "tiff" => ImageFormat::Tiff,
            _ => ImageFormat::Png,
        };

        write_image(image, destination, format, self.jpeg_quality)
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
