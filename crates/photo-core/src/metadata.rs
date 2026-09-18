use exif::{Context, In, Reader, Tag, Value};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawRational {
    pub num: u32,
    pub denom: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawWhiteBalanceEvidence {
    pub as_shot_neutral: Option<[RawRational; 3]>,
    pub as_shot_white_xy: Option<[RawRational; 2]>,
}

impl RawWhiteBalanceEvidence {
    pub fn is_empty(&self) -> bool {
        self.as_shot_neutral.is_none() && self.as_shot_white_xy.is_none()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawMetadataEvidence {
    pub camera_id: Option<String>,
    pub capture_time_ms: Option<i64>,
    pub white_balance: Option<RawWhiteBalanceEvidence>,
}

pub fn read_raw_metadata(path: impl AsRef<Path>) -> RawMetadataEvidence {
    let Ok(file) = File::open(path) else {
        return RawMetadataEvidence::default();
    };
    let mut reader = BufReader::new(file);
    let Ok(exif) = Reader::new().read_from_container(&mut reader) else {
        return RawMetadataEvidence::default();
    };

    let make = exif
        .get_field(Tag::Make, In::PRIMARY)
        .and_then(ascii_field)
        .map(normalize_text)
        .filter(|value| !value.is_empty());
    let model = exif
        .get_field(Tag::Model, In::PRIMARY)
        .and_then(ascii_field)
        .map(normalize_text)
        .filter(|value| !value.is_empty());

    let capture_time_ms = exif
        .get_field(Tag::DateTimeOriginal, In::PRIMARY)
        .and_then(ascii_field)
        .and_then(|value| parse_exif_datetime_ms(&value))
        .or_else(|| {
            exif.get_field(Tag::DateTime, In::PRIMARY)
                .and_then(ascii_field)
                .and_then(|value| parse_exif_datetime_ms(&value))
        });

    let white_balance = RawWhiteBalanceEvidence {
        as_shot_neutral: rational_array::<3>(
            exif.get_field(Tag(Context::Tiff, 0xC628), In::PRIMARY),
        ),
        as_shot_white_xy: rational_array::<2>(
            exif.get_field(Tag(Context::Tiff, 0xC629), In::PRIMARY),
        ),
    };
    let white_balance = (!white_balance.is_empty()).then_some(white_balance);

    RawMetadataEvidence {
        camera_id: combine_camera_id(make.as_deref(), model.as_deref()),
        capture_time_ms,
        white_balance,
    }
}

fn rational_array<const N: usize>(field: Option<&exif::Field>) -> Option<[RawRational; N]> {
    let Value::Rational(values) = &field?.value else {
        return None;
    };
    if values.len() != N || values.iter().any(|value| value.denom == 0) {
        return None;
    }
    std::array::from_fn(|index| RawRational {
        num: values[index].num,
        denom: values[index].denom,
    })
    .into()
}

fn ascii_field(field: &exif::Field) -> Option<String> {
    match &field.value {
        Value::Ascii(values) => values
            .first()
            .map(|value| String::from_utf8_lossy(value).into_owned()),
        _ => None,
    }
}

fn normalize_text(value: String) -> String {
    value
        .trim_matches(|character: char| character == '\0' || character.is_whitespace())
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn combine_camera_id(make: Option<&str>, model: Option<&str>) -> Option<String> {
    match (make, model) {
        (Some(make), Some(model)) if model.to_ascii_lowercase().starts_with(&make.to_ascii_lowercase()) => {
            Some(model.to_string())
        }
        (Some(make), Some(model)) => Some(format!("{make} {model}")),
        (Some(make), None) => Some(make.to_string()),
        (None, Some(model)) => Some(model.to_string()),
        (None, None) => None,
    }
}

/// Parse EXIF's local wall-clock timestamp as a stable timeline value.
///
/// EXIF DateTimeOriginal often has no timezone. Photo-Cake currently uses this
/// value for within-shoot ordering/grouping rather than claiming UTC truth, so
/// the naive local timestamp is mapped onto a UTC-shaped epoch only to obtain a
/// monotonic millisecond coordinate.
fn parse_exif_datetime_ms(value: &str) -> Option<i64> {
    let value = value.trim_matches(|character: char| character == '\0' || character.is_whitespace());
    let mut parts = value.split_whitespace();
    let date = parts.next()?;
    let time = parts.next()?;
    if parts.next().is_some() {
        return None;
    }

    let mut date_parts = date.split(':');
    let year = date_parts.next()?.parse::<i32>().ok()?;
    let month = date_parts.next()?.parse::<u32>().ok()?;
    let day = date_parts.next()?.parse::<u32>().ok()?;
    if date_parts.next().is_some() {
        return None;
    }

    let mut time_parts = time.split(':');
    let hour = time_parts.next()?.parse::<u32>().ok()?;
    let minute = time_parts.next()?.parse::<u32>().ok()?;
    let second = time_parts.next()?.parse::<u32>().ok()?;
    if time_parts.next().is_some()
        || !(1..=12).contains(&month)
        || day == 0
        || day > days_in_month(year, month)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }

    let days = days_from_civil(year, month, day)?;
    let seconds = days
        .checked_mul(86_400)?
        .checked_add(i64::from(hour) * 3_600)?
        .checked_add(i64::from(minute) * 60)?
        .checked_add(i64::from(second.min(59)))?;
    seconds.checked_mul(1_000)
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_from_civil(year: i32, month: u32, day: u32) -> Option<i64> {
    if year < 1 {
        return None;
    }
    let adjusted_year = year - i32::from(month <= 2);
    let era = adjusted_year.div_euclid(400);
    let year_of_era = adjusted_year - era * 400;
    let shifted_month = i32::try_from(month).ok()? + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + i32::try_from(day).ok()? - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some(i64::from(era * 146_097 + day_of_era - 719_468))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_identity_avoids_duplicate_make_prefix() {
        assert_eq!(
            combine_camera_id(Some("SONY"), Some("SONY ILCE-7M4")),
            Some("SONY ILCE-7M4".into())
        );
        assert_eq!(
            combine_camera_id(Some("Canon"), Some("EOS R5")),
            Some("Canon EOS R5".into())
        );
    }

    #[test]
    fn parses_exif_datetime_for_group_timeline() {
        let first = parse_exif_datetime_ms("2026:09:18 14:30:10").unwrap();
        let second = parse_exif_datetime_ms("2026:09:18 14:30:15").unwrap();
        assert_eq!(second - first, 5_000);
    }

    #[test]
    fn validates_leap_day_and_rejects_invalid_dates() {
        assert!(parse_exif_datetime_ms("2024:02:29 23:59:59").is_some());
        assert!(parse_exif_datetime_ms("2025:02:29 12:00:00").is_none());
        assert!(parse_exif_datetime_ms("2026:13:01 12:00:00").is_none());
    }

    #[test]
    fn raw_white_balance_evidence_is_exact_and_never_implies_kelvin() {
        let evidence = RawWhiteBalanceEvidence {
            as_shot_neutral: Some([
                RawRational { num: 1, denom: 2 },
                RawRational { num: 1, denom: 1 },
                RawRational { num: 3, denom: 5 },
            ]),
            as_shot_white_xy: None,
        };
        assert!(!evidence.is_empty());
    }

    #[test]
    fn unreadable_or_non_exif_file_returns_empty_evidence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fake.cr3");
        std::fs::write(&path, b"not-real-raw").unwrap();
        assert_eq!(read_raw_metadata(path), RawMetadataEvidence::default());
    }
}
