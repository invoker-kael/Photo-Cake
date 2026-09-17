use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

const RAW_EXTENSIONS: &[&str] = &[
    "3fr", "arw", "cr2", "cr3", "dng", "iiq", "nef", "nrw", "orf", "pef", "raf",
    "rw2", "sr2", "srf",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawAsset {
    pub id: Uuid,
    pub source_path: String,
    pub filename: String,
    pub extension: String,
    pub camera_id: Option<String>,
    pub capture_time_ms: Option<i64>,
    pub file_time_ms: Option<i64>,
    pub sequence_number: Option<u64>,
}

impl RawAsset {
    pub fn effective_time_ms(&self) -> Option<i64> {
        self.capture_time_ms.or(self.file_time_ms)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawImportScan {
    pub assets: Vec<RawAsset>,
    pub skipped_non_raw: Vec<String>,
}

pub fn is_supported_raw(path: impl AsRef<Path>) -> bool {
    path.as_ref()
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| RAW_EXTENSIONS.contains(&value.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

pub fn scan_raw_paths(paths: impl IntoIterator<Item = PathBuf>) -> RawImportScan {
    let mut assets = Vec::new();
    let mut skipped_non_raw = Vec::new();

    for path in paths {
        if !is_supported_raw(&path) {
            skipped_non_raw.push(path.to_string_lossy().into_owned());
            continue;
        }

        let filename = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_string();
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let file_time_ms = path
            .metadata()
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(system_time_to_ms);

        assets.push(RawAsset {
            id: Uuid::new_v4(),
            source_path: path.to_string_lossy().into_owned(),
            filename: filename.clone(),
            extension,
            camera_id: None,
            capture_time_ms: None,
            file_time_ms,
            sequence_number: extract_sequence_number(&filename),
        });
    }

    RawImportScan {
        assets,
        skipped_non_raw,
    }
}

pub fn scan_raw_directory(root: impl AsRef<Path>, recursive: bool) -> std::io::Result<RawImportScan> {
    let mut paths = Vec::new();
    collect_files(root.as_ref(), recursive, &mut paths)?;
    Ok(scan_raw_paths(paths))
}

fn collect_files(root: &Path, recursive: bool, output: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_file() {
            output.push(path);
        } else if recursive && file_type.is_dir() {
            collect_files(&path, recursive, output)?;
        }
    }
    Ok(())
}

pub fn extract_sequence_number(filename: &str) -> Option<u64> {
    let stem = Path::new(filename).file_stem()?.to_str()?;
    let digits_rev: String = stem
        .chars()
        .rev()
        .take_while(|character| character.is_ascii_digit())
        .collect();
    if digits_rev.is_empty() {
        return None;
    }
    digits_rev.chars().rev().collect::<String>().parse().ok()
}

fn system_time_to_ms(value: SystemTime) -> Option<i64> {
    let duration = value.duration_since(UNIX_EPOCH).ok()?;
    i64::try_from(duration.as_millis()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn accepts_common_raw_and_rejects_rendered_formats() {
        assert!(is_supported_raw("A.CR3"));
        assert!(is_supported_raw("B.nef"));
        assert!(is_supported_raw("C.ARW"));
        assert!(!is_supported_raw("D.jpg"));
        assert!(!is_supported_raw("E.tiff"));
        assert!(!is_supported_raw("F.png"));
        assert!(!is_supported_raw("G.heic"));
    }

    #[test]
    fn extracts_trailing_sequence_number() {
        assert_eq!(extract_sequence_number("DSC_1042.ARW"), Some(1042));
        assert_eq!(extract_sequence_number("IMG000123.CR3"), Some(123));
        assert_eq!(extract_sequence_number("portrait-final.NEF"), None);
    }

    #[test]
    fn directory_scan_keeps_only_raw_files() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("IMG_0001.CR3"), b"raw").unwrap();
        std::fs::write(dir.path().join("IMG_0001.JPG"), b"jpeg").unwrap();

        let scan = scan_raw_directory(dir.path(), false).unwrap();
        assert_eq!(scan.assets.len(), 1);
        assert_eq!(scan.assets[0].filename, "IMG_0001.CR3");
        assert_eq!(scan.skipped_non_raw.len(), 1);
    }
}
