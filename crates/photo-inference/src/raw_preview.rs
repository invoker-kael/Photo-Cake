use image::ImageFormat;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use thiserror::Error;

const FINGERPRINT_CHUNK: usize = 64 * 1024;

#[derive(Debug, Error)]
pub enum RawPreviewError {
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("no decodable embedded JPEG preview found in RAW")]
    EmbeddedPreviewMissing,
    #[error("image decode error: {0}")]
    Image(#[from] image::ImageError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtractedPreviewInfo {
    pub width: u32,
    pub height: u32,
}

pub fn source_fingerprint(path: impl AsRef<Path>) -> Result<String, RawPreviewError> {
    let path = path.as_ref();
    let mut file = File::open(path)?;
    let metadata = file.metadata()?;
    let size = metadata.len();
    let modified_ns = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);

    let mut hasher = Sha256::new();
    hasher.update(size.to_le_bytes());
    hasher.update(modified_ns.to_le_bytes());

    let first_len = usize::try_from(size.min(FINGERPRINT_CHUNK as u64)).unwrap_or(0);
    let mut first = vec![0u8; first_len];
    if first_len > 0 {
        file.read_exact(&mut first)?;
        hasher.update(&first);
    }

    if size > FINGERPRINT_CHUNK as u64 {
        file.seek(SeekFrom::End(-(FINGERPRINT_CHUNK as i64)))?;
        let mut last = vec![0u8; FINGERPRINT_CHUNK];
        file.read_exact(&mut last)?;
        hasher.update(&last);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

pub fn extract_largest_embedded_jpeg(
    raw_path: impl AsRef<Path>,
    output_path: impl AsRef<Path>,
) -> Result<ExtractedPreviewInfo, RawPreviewError> {
    let bytes = std::fs::read(raw_path)?;
    let (jpeg, width, height) = largest_decodable_jpeg(&bytes)
        .ok_or(RawPreviewError::EmbeddedPreviewMissing)?;

    if let Some(parent) = output_path.as_ref().parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output_path, jpeg)?;
    Ok(ExtractedPreviewInfo { width, height })
}

fn largest_decodable_jpeg(bytes: &[u8]) -> Option<(&[u8], u32, u32)> {
    let mut best: Option<(&[u8], u32, u32, u64)> = None;
    let mut cursor = 0usize;

    while cursor + 1 < bytes.len() {
        let Some(relative_start) = bytes[cursor..]
            .windows(2)
            .position(|window| window == [0xff, 0xd8])
        else {
            break;
        };
        let start = cursor + relative_start;
        let body_start = start.saturating_add(2);
        let Some(relative_end) = bytes[body_start..]
            .windows(2)
            .position(|window| window == [0xff, 0xd9])
        else {
            break;
        };
        let end = body_start + relative_end + 2;
        let candidate = &bytes[start..end];

        if let Ok(image) = image::load_from_memory_with_format(candidate, ImageFormat::Jpeg) {
            let width = image.width();
            let height = image.height();
            let area = u64::from(width) * u64::from(height);
            if best.as_ref().is_none_or(|(_, _, _, best_area)| area > *best_area) {
                best = Some((candidate, width, height, area));
            }
        }

        cursor = start.saturating_add(2);
    }

    best.map(|(jpeg, width, height, _)| (jpeg, width, height))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageBuffer, Rgb};
    use tempfile::tempdir;

    fn jpeg(width: u32, height: u32, value: u8) -> Vec<u8> {
        let image = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(
            width,
            height,
            Rgb([value, value, value]),
        ));
        let mut output = Vec::new();
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, 85);
        encoder.encode_image(&image).unwrap();
        output
    }

    #[test]
    fn chooses_largest_embedded_jpeg_without_modifying_raw() {
        let dir = tempdir().unwrap();
        let raw = dir.path().join("sample.cr3");
        let output = dir.path().join("cache/preview.jpg");
        let small = jpeg(64, 48, 20);
        let large = jpeg(320, 240, 180);
        let mut bytes = b"RAW-HEADER".to_vec();
        bytes.extend_from_slice(&small);
        bytes.extend_from_slice(b"RAW-MIDDLE");
        bytes.extend_from_slice(&large);
        bytes.extend_from_slice(b"RAW-TAIL");
        std::fs::write(&raw, &bytes).unwrap();

        let before = std::fs::read(&raw).unwrap();
        let info = extract_largest_embedded_jpeg(&raw, &output).unwrap();
        let after = std::fs::read(&raw).unwrap();

        assert_eq!(info.width, 320);
        assert_eq!(info.height, 240);
        assert_eq!(before, after);
        assert!(output.exists());
    }

    #[test]
    fn fingerprint_changes_when_source_content_changes() {
        let dir = tempdir().unwrap();
        let raw = dir.path().join("sample.nef");
        std::fs::write(&raw, vec![1u8; FINGERPRINT_CHUNK * 2 + 10]).unwrap();
        let first = source_fingerprint(&raw).unwrap();
        let mut bytes = std::fs::read(&raw).unwrap();
        let last = bytes.len() - 1;
        bytes[last] = 9;
        std::fs::write(&raw, bytes).unwrap();
        let second = source_fingerprint(&raw).unwrap();
        assert_ne!(first, second);
    }
}
