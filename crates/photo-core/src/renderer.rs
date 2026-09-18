use crate::{ExportRenderer, ExportWorkerError};
use image::{DynamicImage, GenericImageView, ImageFormat};
use std::path::Path;

use crate::export::ExportRecipe;

/// Runtime rendering options derived from export recipe.
#[derive(Debug, Clone, Copy)]
pub struct RenderOptions {
    pub jpeg_quality: u8,
    pub resize_long_edge: Option<u32>,
    pub allow_upscale: bool,
}

impl RenderOptions {
    pub fn from_recipe(recipe: &ExportRecipe) -> Self {
        Self {
            jpeg_quality: recipe.jpeg_quality.unwrap_or(92),
            resize_long_edge: recipe.resize.as_ref().map(|resize| resize.long_edge_px),
            allow_upscale: recipe
                .resize
                .as_ref()
                .map(|resize| resize.allow_upscale)
                .unwrap_or(false),
        }
    }
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            jpeg_quality: 92,
            resize_long_edge: None,
            allow_upscale: false,
        }
    }
}

/// Baseline raster renderer.
///
/// This stage materializes already-decoded images. RAW demosaic and
/// non-destructive edit evaluation remain separate stages.
pub struct ImageExportRenderer {
    pub options: RenderOptions,
}

impl ImageExportRenderer {
    pub fn from_recipe(recipe: &ExportRecipe) -> Self {
        Self {
            options: RenderOptions::from_recipe(recipe),
        }
    }
}

impl Default for ImageExportRenderer {
    fn default() -> Self {
        Self {
            options: RenderOptions::default(),
        }
    }
}

impl ExportRenderer for ImageExportRenderer {
    fn render(&self, source: &Path, destination: &Path) -> Result<(), ExportWorkerError> {
        let mut image = image::open(source)
            .map_err(|error| ExportWorkerError::Render(error.to_string()))?;

        if let Some(edge) = self.options.resize_long_edge {
            image = resize_long_edge(image, edge, self.options.allow_upscale);
        }

        write_image(
            image,
            destination,
            output_format(destination),
            self.options.jpeg_quality,
        )
    }
}

fn resize_long_edge(image: DynamicImage, edge: u32, allow_upscale: bool) -> DynamicImage {
    let (width, height) = image.dimensions();
    let current_edge = width.max(height);

    if current_edge <= edge || (!allow_upscale && current_edge < edge) {
        return image;
    }

    let scale = edge as f32 / current_edge as f32;
    image.resize(
        (width as f32 * scale) as u32,
        (height as f32 * scale) as u32,
        image::imageops::FilterType::Lanczos3,
    )
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
            let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut file, quality.clamp(1, 100));
            image
                .write_with_encoder(encoder)
                .map_err(|error| ExportWorkerError::Render(error.to_string()))?;
        }
        _ => image
            .write_to(&mut file, format)
            .map_err(|error| ExportWorkerError::Render(error.to_string()))?,
    }

    Ok(())
}
