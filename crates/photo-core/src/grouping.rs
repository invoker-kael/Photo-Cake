use crate::RawAsset;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PhotoGroupKind {
    Moment,
    Similar,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GroupingBasis {
    Time,
    TimeAndSequence,
    SequenceFallback,
    SemanticSimilarity,
    Singleton,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhotoGroup {
    pub id: Uuid,
    pub kind: PhotoGroupKind,
    pub basis: GroupingBasis,
    pub asset_ids: Vec<Uuid>,
    pub manual_locked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct InitialGroupingConfig {
    pub short_gap_ms: i64,
    pub max_gap_ms: i64,
    pub max_sequence_gap: u64,
}

impl Default for InitialGroupingConfig {
    fn default() -> Self {
        Self {
            short_gap_ms: 1_500,
            max_gap_ms: 4_000,
            max_sequence_gap: 1,
        }
    }
}

pub fn initial_group_raw_assets(
    assets: &[RawAsset],
    config: InitialGroupingConfig,
) -> Vec<PhotoGroup> {
    if assets.is_empty() {
        return Vec::new();
    }

    let mut ordered = assets.iter().collect::<Vec<_>>();
    ordered.sort_by(compare_assets);

    let mut groups: Vec<PhotoGroup> = Vec::new();
    let mut current: Vec<&RawAsset> = vec![ordered[0]];
    let mut current_basis = GroupingBasis::Singleton;

    for asset in ordered.into_iter().skip(1) {
        let previous = *current.last().expect("group always has an item");
        if let Some(basis) = should_join(previous, asset, config) {
            current.push(asset);
            current_basis = merge_basis(current_basis, basis);
        } else {
            groups.push(build_group(&current, current_basis));
            current = vec![asset];
            current_basis = GroupingBasis::Singleton;
        }
    }

    groups.push(build_group(&current, current_basis));
    groups
}

fn should_join(
    previous: &RawAsset,
    next: &RawAsset,
    config: InitialGroupingConfig,
) -> Option<GroupingBasis> {
    if camera_conflicts(previous, next) {
        return None;
    }

    let sequence_match = sequence_is_close(previous, next, config.max_sequence_gap);
    let prefix_match = sequence_prefix(&previous.filename) == sequence_prefix(&next.filename);

    match (previous.effective_time_ms(), next.effective_time_ms()) {
        (Some(left), Some(right)) => {
            let gap = right.saturating_sub(left).abs();
            if gap <= config.short_gap_ms {
                return Some(if sequence_match {
                    GroupingBasis::TimeAndSequence
                } else {
                    GroupingBasis::Time
                });
            }
            if gap <= config.max_gap_ms && sequence_match && prefix_match {
                return Some(GroupingBasis::TimeAndSequence);
            }
            None
        }
        _ if sequence_match && prefix_match => Some(GroupingBasis::SequenceFallback),
        _ => None,
    }
}

fn camera_conflicts(left: &RawAsset, right: &RawAsset) -> bool {
    matches!(
        (&left.camera_id, &right.camera_id),
        (Some(left), Some(right)) if left != right
    )
}

fn sequence_is_close(left: &RawAsset, right: &RawAsset, max_gap: u64) -> bool {
    match (left.sequence_number, right.sequence_number) {
        (Some(left), Some(right)) => left.abs_diff(right) <= max_gap,
        _ => false,
    }
}

fn sequence_prefix(filename: &str) -> String {
    let stem = Path::new(filename)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(filename);
    stem.trim_end_matches(|character: char| character.is_ascii_digit())
        .to_ascii_lowercase()
}

fn compare_assets(left: &&RawAsset, right: &&RawAsset) -> Ordering {
    match (left.effective_time_ms(), right.effective_time_ms()) {
        (Some(left_time), Some(right_time)) => left_time
            .cmp(&right_time)
            .then_with(|| left.sequence_number.cmp(&right.sequence_number))
            .then_with(|| left.filename.cmp(&right.filename)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => left
            .sequence_number
            .cmp(&right.sequence_number)
            .then_with(|| left.filename.cmp(&right.filename)),
    }
}

fn merge_basis(current: GroupingBasis, next: GroupingBasis) -> GroupingBasis {
    match (current, next) {
        (_, GroupingBasis::TimeAndSequence) => GroupingBasis::TimeAndSequence,
        (GroupingBasis::Singleton, basis) => basis,
        (GroupingBasis::SequenceFallback, GroupingBasis::Time) => GroupingBasis::Time,
        (basis, _) => basis,
    }
}

fn build_group(items: &[&RawAsset], basis: GroupingBasis) -> PhotoGroup {
    PhotoGroup {
        id: Uuid::new_v4(),
        kind: PhotoGroupKind::Moment,
        basis: if items.len() == 1 {
            GroupingBasis::Singleton
        } else {
            basis
        },
        asset_ids: items.iter().map(|asset| asset.id).collect(),
        manual_locked: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str, time: Option<i64>, camera: Option<&str>) -> RawAsset {
        RawAsset {
            id: Uuid::new_v4(),
            source_path: name.to_string(),
            filename: name.to_string(),
            extension: "cr3".to_string(),
            camera_id: camera.map(str::to_string),
            capture_time_ms: time,
            file_time_ms: None,
            sequence_number: crate::extract_sequence_number(name),
        }
    }

    #[test]
    fn groups_a_short_burst_and_splits_a_later_scene() {
        let assets = vec![
            asset("IMG_1001.CR3", Some(0), Some("cam-a")),
            asset("IMG_1002.CR3", Some(700), Some("cam-a")),
            asset("IMG_1003.CR3", Some(1_400), Some("cam-a")),
            asset("IMG_1004.CR3", Some(15_000), Some("cam-a")),
        ];

        let groups = initial_group_raw_assets(&assets, InitialGroupingConfig::default());
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].asset_ids.len(), 3);
        assert_eq!(groups[1].asset_ids.len(), 1);
    }

    #[test]
    fn different_known_cameras_do_not_merge() {
        let assets = vec![
            asset("IMG_1001.CR3", Some(0), Some("cam-a")),
            asset("IMG_1002.CR3", Some(500), Some("cam-b")),
        ];

        let groups = initial_group_raw_assets(&assets, InitialGroupingConfig::default());
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn sequence_can_group_when_time_is_unavailable() {
        let assets = vec![
            asset("DSC_0200.NEF", None, None),
            asset("DSC_0201.NEF", None, None),
        ];

        let groups = initial_group_raw_assets(&assets, InitialGroupingConfig::default());
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].basis, GroupingBasis::SequenceFallback);
    }
}
