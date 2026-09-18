use crate::{
    build_group_culling_result, AnalysisCache, AssetMetadataEvidence, BatchStore, CatalogError,
    CullingEvidenceError, CullingReview, CullingReviewStore, CullingReviewStoreError,
    CullingUserDecision, GroupCullingResult, GroupReferenceBinding, PhotoGroup, PreviewSource,
    PreviewStore, PreviewStoreError, RawAsset, RawCatalog, RawMetadataStore, RawMetadataStoreError,
    ReferenceSet, ReferenceStore, ReferenceStoreError, StoreError,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use thiserror::Error;
use uuid::Uuid;

pub const COMPANION_SNAPSHOT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanionAsset {
    pub id: Uuid,
    pub filename: String,
    pub extension: String,
    pub camera_id: Option<String>,
    pub capture_time_ms: Option<i64>,
    pub file_time_ms: Option<i64>,
    pub sequence_number: Option<u64>,
}

impl From<&RawAsset> for CompanionAsset {
    fn from(asset: &RawAsset) -> Self {
        Self {
            id: asset.id,
            filename: asset.filename.clone(),
            extension: asset.extension.clone(),
            camera_id: asset.camera_id.clone(),
            capture_time_ms: asset.capture_time_ms,
            file_time_ms: asset.file_time_ms,
            sequence_number: asset.sequence_number,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanionPreviewIndex {
    pub asset_id: Uuid,
    pub transport_name: String,
    pub revision: String,
    pub mime_type: String,
    pub width: u32,
    pub height: u32,
    pub source: PreviewSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanionReferenceState {
    pub binding: GroupReferenceBinding,
    pub reference_set: ReferenceSet,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanionSnapshot {
    pub schema_version: u32,
    pub snapshot_id: Uuid,
    pub batch_id: Uuid,
    pub batch_name: String,
    pub assets: Vec<CompanionAsset>,
    pub groups: Vec<PhotoGroup>,
    pub culling: Vec<GroupCullingResult>,
    pub culling_reviews: Vec<CullingReview>,
    pub references: Vec<CompanionReferenceState>,
    pub metadata: Vec<AssetMetadataEvidence>,
    pub previews: Vec<CompanionPreviewIndex>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompanionCullingChange {
    pub asset_id: Uuid,
    pub decision: Option<CullingUserDecision>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompanionReferenceChange {
    pub group_id: Uuid,
    pub selected_reference_asset_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompanionDecisionPatch {
    pub schema_version: u32,
    pub base_snapshot_id: Uuid,
    pub batch_id: Uuid,
    pub culling_changes: Vec<CompanionCullingChange>,
    pub reference_changes: Vec<CompanionReferenceChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompanionPatchApplyReport {
    pub culling_changes_applied: usize,
    pub reference_changes_applied: usize,
}

#[derive(Debug, Error)]
pub enum CompanionError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    #[error(transparent)]
    CullingEvidence(#[from] CullingEvidenceError),
    #[error(transparent)]
    CullingReview(#[from] CullingReviewStoreError),
    #[error(transparent)]
    Reference(#[from] ReferenceStoreError),
    #[error(transparent)]
    Metadata(#[from] RawMetadataStoreError),
    #[error(transparent)]
    Preview(#[from] PreviewStoreError),
    #[error("unsupported companion schema version: {0}")]
    UnsupportedSchema(u32),
    #[error("patch targets snapshot {actual}, expected {expected}")]
    SnapshotMismatch { expected: Uuid, actual: Uuid },
    #[error("patch targets batch {actual}, expected {expected}")]
    BatchMismatch { expected: Uuid, actual: Uuid },
    #[error("companion patch references unknown asset: {0}")]
    UnknownAsset(Uuid),
    #[error("companion patch references unknown group: {0}")]
    UnknownGroup(Uuid),
    #[error("asset {asset_id} is not a member of group {group_id}")]
    ReferenceOutsideGroup { group_id: Uuid, asset_id: Uuid },
    #[error("explicitly rejected asset {asset_id} cannot be selected as group {group_id} reference")]
    RejectedReference { group_id: Uuid, asset_id: Uuid },
    #[error("workstation culling decision changed since snapshot for asset {0}")]
    ConcurrentCullingChange(Uuid),
    #[error("workstation reference changed since snapshot for group {0}")]
    ConcurrentReferenceChange(Uuid),
    #[error("companion patch contains duplicate culling change for asset {0}")]
    DuplicateCullingChange(Uuid),
    #[error("companion patch contains duplicate reference change for group {0}")]
    DuplicateReferenceChange(Uuid),
}

#[allow(clippy::too_many_arguments)]
pub fn build_companion_snapshot(
    batch_id: Uuid,
    batch_store: &BatchStore,
    catalog: &RawCatalog,
    analysis_cache: &AnalysisCache,
    preview_store: &PreviewStore,
    metadata_store: &RawMetadataStore,
    culling_reviews: &CullingReviewStore,
    reference_store: &ReferenceStore,
    duplicate_threshold: f32,
) -> Result<CompanionSnapshot, CompanionError> {
    let batch = batch_store.load_batch(batch_id)?;
    let asset_ids = batch
        .items
        .iter()
        .filter_map(|item| item.asset_id)
        .collect::<HashSet<_>>();
    let ordered_asset_ids = batch
        .items
        .iter()
        .filter_map(|item| item.asset_id)
        .collect::<Vec<_>>();

    let assets = catalog
        .list_assets()?
        .into_iter()
        .filter(|asset| asset_ids.contains(&asset.id))
        .collect::<Vec<_>>();
    let groups = catalog.list_effective_groups_for_collection(batch_id)?;

    let culling = groups
        .iter()
        .map(|group| build_group_culling_result(analysis_cache, group, duplicate_threshold))
        .collect::<Result<Vec<_>, _>>()?;

    let culling_reviews = culling_reviews.list_for_assets(&ordered_asset_ids)?;
    let metadata = metadata_store.list_for_assets(&ordered_asset_ids)?;
    let previews = preview_store
        .list_for_assets(&ordered_asset_ids)?
        .into_iter()
        .map(|preview| CompanionPreviewIndex {
            asset_id: preview.asset_id,
            transport_name: format!("{}.jpg", preview.asset_id),
            revision: preview.revision,
            mime_type: preview.mime_type,
            width: preview.width,
            height: preview.height,
            source: preview.source,
        })
        .collect();

    let mut references = Vec::new();
    for group in &groups {
        let Some(binding) = reference_store.group_binding(group.id)? else {
            continue;
        };
        let reference_set = reference_store
            .get_set(binding.reference_set_id)?
            .ok_or(ReferenceStoreError::ReferenceSetNotFound(
                binding.reference_set_id,
            ))?;
        references.push(CompanionReferenceState {
            binding,
            reference_set,
        });
    }

    Ok(CompanionSnapshot {
        schema_version: COMPANION_SNAPSHOT_SCHEMA_VERSION,
        snapshot_id: Uuid::new_v4(),
        batch_id,
        batch_name: batch.name,
        assets: assets.iter().map(CompanionAsset::from).collect(),
        groups,
        culling,
        culling_reviews,
        references,
        metadata,
        previews,
    })
}

pub fn apply_companion_patch(
    snapshot: &CompanionSnapshot,
    patch: &CompanionDecisionPatch,
    culling_reviews: &CullingReviewStore,
    reference_store: &ReferenceStore,
) -> Result<CompanionPatchApplyReport, CompanionError> {
    validate_patch_identity(snapshot, patch)?;
    validate_current_state(snapshot, patch, culling_reviews, reference_store)?;

    let asset_ids = snapshot
        .assets
        .iter()
        .map(|asset| asset.id)
        .collect::<HashSet<_>>();
    let groups = snapshot
        .groups
        .iter()
        .map(|group| (group.id, group))
        .collect::<HashMap<_, _>>();

    let mut final_decisions = snapshot
        .culling_reviews
        .iter()
        .map(|review| (review.asset_id, Some(review.decision)))
        .collect::<HashMap<_, _>>();
    let mut seen_assets = HashSet::new();

    for change in &patch.culling_changes {
        if !asset_ids.contains(&change.asset_id) {
            return Err(CompanionError::UnknownAsset(change.asset_id));
        }
        if !seen_assets.insert(change.asset_id) {
            return Err(CompanionError::DuplicateCullingChange(change.asset_id));
        }
        final_decisions.insert(change.asset_id, change.decision);
    }

    let mut final_references = snapshot
        .references
        .iter()
        .map(|state| {
            (
                state.binding.group_id,
                Some(state.binding.selected_reference_asset_id),
            )
        })
        .collect::<HashMap<_, _>>();
    let mut seen_groups = HashSet::new();

    for change in &patch.reference_changes {
        let group = groups
            .get(&change.group_id)
            .ok_or(CompanionError::UnknownGroup(change.group_id))?;
        if !seen_groups.insert(change.group_id) {
            return Err(CompanionError::DuplicateReferenceChange(change.group_id));
        }
        if let Some(asset_id) = change.selected_reference_asset_id {
            if !group.asset_ids.contains(&asset_id) {
                return Err(CompanionError::ReferenceOutsideGroup {
                    group_id: change.group_id,
                    asset_id,
                });
            }
        }
        final_references.insert(change.group_id, change.selected_reference_asset_id);
    }

    for (group_id, selected_reference_asset_id) in final_references {
        let Some(asset_id) = selected_reference_asset_id else {
            continue;
        };
        if final_decisions.get(&asset_id).copied().flatten()
            == Some(CullingUserDecision::Reject)
        {
            return Err(CompanionError::RejectedReference { group_id, asset_id });
        }
    }

    for change in &patch.culling_changes {
        match change.decision {
            Some(decision) => {
                culling_reviews.set(change.asset_id, decision)?;
            }
            None => culling_reviews.clear(change.asset_id)?,
        }
    }

    for change in &patch.reference_changes {
        match change.selected_reference_asset_id {
            Some(asset_id) => {
                reference_store.set_single_photo_reference(
                    change.group_id,
                    asset_id,
                    format!("Group {} reference", change.group_id),
                )?;
            }
            None => reference_store.clear_group_binding(change.group_id)?,
        }
    }

    Ok(CompanionPatchApplyReport {
        culling_changes_applied: patch.culling_changes.len(),
        reference_changes_applied: patch.reference_changes.len(),
    })
}

fn validate_patch_identity(
    snapshot: &CompanionSnapshot,
    patch: &CompanionDecisionPatch,
) -> Result<(), CompanionError> {
    if snapshot.schema_version != COMPANION_SNAPSHOT_SCHEMA_VERSION {
        return Err(CompanionError::UnsupportedSchema(snapshot.schema_version));
    }
    if patch.schema_version != COMPANION_SNAPSHOT_SCHEMA_VERSION {
        return Err(CompanionError::UnsupportedSchema(patch.schema_version));
    }
    if patch.base_snapshot_id != snapshot.snapshot_id {
        return Err(CompanionError::SnapshotMismatch {
            expected: snapshot.snapshot_id,
            actual: patch.base_snapshot_id,
        });
    }
    if patch.batch_id != snapshot.batch_id {
        return Err(CompanionError::BatchMismatch {
            expected: snapshot.batch_id,
            actual: patch.batch_id,
        });
    }
    Ok(())
}

fn validate_current_state(
    snapshot: &CompanionSnapshot,
    patch: &CompanionDecisionPatch,
    culling_reviews: &CullingReviewStore,
    reference_store: &ReferenceStore,
) -> Result<(), CompanionError> {
    let snapshot_reviews = snapshot
        .culling_reviews
        .iter()
        .map(|review| (review.asset_id, Some(review.decision)))
        .collect::<HashMap<_, _>>();

    let touched_assets = patch
        .culling_changes
        .iter()
        .map(|change| change.asset_id)
        .collect::<HashSet<_>>();

    for asset_id in &touched_assets {
        let current = culling_reviews.get(*asset_id)?.map(|review| review.decision);
        let expected = snapshot_reviews.get(asset_id).copied().flatten();
        if current != expected {
            return Err(CompanionError::ConcurrentCullingChange(*asset_id));
        }
    }

    let snapshot_references = snapshot
        .references
        .iter()
        .map(|state| (state.binding.group_id, Some(state.binding.clone())))
        .collect::<HashMap<_, _>>();

    let mut touched_groups = patch
        .reference_changes
        .iter()
        .map(|change| change.group_id)
        .collect::<HashSet<_>>();

    for state in &snapshot.references {
        if touched_assets.contains(&state.binding.selected_reference_asset_id) {
            touched_groups.insert(state.binding.group_id);
        }
    }

    for group_id in touched_groups {
        let current = reference_store.group_binding(group_id)?;
        let expected = snapshot_references.get(&group_id).cloned().flatten();
        if current != expected {
            return Err(CompanionError::ConcurrentReferenceChange(group_id));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GroupingBasis, PhotoGroupKind};
    use tempfile::tempdir;

    fn snapshot(asset_id: Uuid, group_id: Uuid) -> CompanionSnapshot {
        CompanionSnapshot {
            schema_version: COMPANION_SNAPSHOT_SCHEMA_VERSION,
            snapshot_id: Uuid::new_v4(),
            batch_id: Uuid::new_v4(),
            batch_name: "Trip".into(),
            assets: vec![CompanionAsset {
                id: asset_id,
                filename: "IMG_0001.CR3".into(),
                extension: "cr3".into(),
                camera_id: Some("Canon EOS R5".into()),
                capture_time_ms: Some(1_800_000_000_000),
                file_time_ms: None,
                sequence_number: Some(1),
            }],
            groups: vec![PhotoGroup {
                id: group_id,
                kind: PhotoGroupKind::Similar,
                basis: GroupingBasis::SemanticSimilarity,
                asset_ids: vec![asset_id],
                manual_locked: false,
            }],
            culling: Vec::new(),
            culling_reviews: Vec::new(),
            references: Vec::new(),
            metadata: Vec::new(),
            previews: Vec::new(),
        }
    }

    #[test]
    fn companion_asset_serialization_does_not_leak_raw_source_path() {
        let value = serde_json::to_value(CompanionAsset {
            id: Uuid::new_v4(),
            filename: "IMG_0001.CR3".into(),
            extension: "cr3".into(),
            camera_id: None,
            capture_time_ms: None,
            file_time_ms: None,
            sequence_number: Some(1),
        })
        .unwrap();

        assert!(value.get("source_path").is_none());
    }

    #[test]
    fn applies_valid_cull_and_reference_patch() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("project.sqlite3");
        let culling = CullingReviewStore::open(&db).unwrap();
        let references = ReferenceStore::open(&db).unwrap();
        let asset_id = Uuid::new_v4();
        let group_id = Uuid::new_v4();
        let snapshot = snapshot(asset_id, group_id);
        let patch = CompanionDecisionPatch {
            schema_version: COMPANION_SNAPSHOT_SCHEMA_VERSION,
            base_snapshot_id: snapshot.snapshot_id,
            batch_id: snapshot.batch_id,
            culling_changes: vec![CompanionCullingChange {
                asset_id,
                decision: Some(CullingUserDecision::Keep),
            }],
            reference_changes: vec![CompanionReferenceChange {
                group_id,
                selected_reference_asset_id: Some(asset_id),
            }],
        };

        let report = apply_companion_patch(&snapshot, &patch, &culling, &references).unwrap();

        assert_eq!(report.culling_changes_applied, 1);
        assert_eq!(report.reference_changes_applied, 1);
        assert_eq!(
            culling.get(asset_id).unwrap().unwrap().decision,
            CullingUserDecision::Keep
        );
        assert_eq!(
            references
                .group_binding(group_id)
                .unwrap()
                .unwrap()
                .selected_reference_asset_id,
            asset_id
        );
    }

    #[test]
    fn rejects_reference_that_patch_also_rejects() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("project.sqlite3");
        let culling = CullingReviewStore::open(&db).unwrap();
        let references = ReferenceStore::open(&db).unwrap();
        let asset_id = Uuid::new_v4();
        let group_id = Uuid::new_v4();
        let snapshot = snapshot(asset_id, group_id);
        let patch = CompanionDecisionPatch {
            schema_version: COMPANION_SNAPSHOT_SCHEMA_VERSION,
            base_snapshot_id: snapshot.snapshot_id,
            batch_id: snapshot.batch_id,
            culling_changes: vec![CompanionCullingChange {
                asset_id,
                decision: Some(CullingUserDecision::Reject),
            }],
            reference_changes: vec![CompanionReferenceChange {
                group_id,
                selected_reference_asset_id: Some(asset_id),
            }],
        };

        assert!(matches!(
            apply_companion_patch(&snapshot, &patch, &culling, &references),
            Err(CompanionError::RejectedReference { .. })
        ));
        assert!(culling.get(asset_id).unwrap().is_none());
        assert!(references.group_binding(group_id).unwrap().is_none());
    }

    #[test]
    fn cannot_reject_existing_reference_without_clearing_or_replacing_it() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("project.sqlite3");
        let culling = CullingReviewStore::open(&db).unwrap();
        let references = ReferenceStore::open(&db).unwrap();
        let asset_id = Uuid::new_v4();
        let group_id = Uuid::new_v4();
        let mut snapshot = snapshot(asset_id, group_id);
        let (reference_set, binding) = references
            .set_single_photo_reference(group_id, asset_id, "Group reference")
            .unwrap();
        snapshot.references.push(CompanionReferenceState {
            binding,
            reference_set,
        });

        let patch = CompanionDecisionPatch {
            schema_version: COMPANION_SNAPSHOT_SCHEMA_VERSION,
            base_snapshot_id: snapshot.snapshot_id,
            batch_id: snapshot.batch_id,
            culling_changes: vec![CompanionCullingChange {
                asset_id,
                decision: Some(CullingUserDecision::Reject),
            }],
            reference_changes: Vec::new(),
        };

        assert!(matches!(
            apply_companion_patch(&snapshot, &patch, &culling, &references),
            Err(CompanionError::RejectedReference { group_id: id, asset_id: asset })
                if id == group_id && asset == asset_id
        ));
        assert!(culling.get(asset_id).unwrap().is_none());
    }

    #[test]
    fn duplicate_changes_are_rejected_before_writes() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("project.sqlite3");
        let culling = CullingReviewStore::open(&db).unwrap();
        let references = ReferenceStore::open(&db).unwrap();
        let asset_id = Uuid::new_v4();
        let group_id = Uuid::new_v4();
        let snapshot = snapshot(asset_id, group_id);
        let patch = CompanionDecisionPatch {
            schema_version: COMPANION_SNAPSHOT_SCHEMA_VERSION,
            base_snapshot_id: snapshot.snapshot_id,
            batch_id: snapshot.batch_id,
            culling_changes: vec![
                CompanionCullingChange {
                    asset_id,
                    decision: Some(CullingUserDecision::Keep),
                },
                CompanionCullingChange {
                    asset_id,
                    decision: Some(CullingUserDecision::Review),
                },
            ],
            reference_changes: Vec::new(),
        };

        assert!(matches!(
            apply_companion_patch(&snapshot, &patch, &culling, &references),
            Err(CompanionError::DuplicateCullingChange(id)) if id == asset_id
        ));
        assert!(culling.get(asset_id).unwrap().is_none());
    }

    #[test]
    fn unrelated_workstation_change_does_not_block_patch() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("project.sqlite3");
        let culling = CullingReviewStore::open(&db).unwrap();
        let references = ReferenceStore::open(&db).unwrap();
        let asset_id = Uuid::new_v4();
        let unrelated = Uuid::new_v4();
        let group_id = Uuid::new_v4();
        let snapshot = snapshot(asset_id, group_id);

        culling
            .set(unrelated, CullingUserDecision::Review)
            .unwrap();

        let patch = CompanionDecisionPatch {
            schema_version: COMPANION_SNAPSHOT_SCHEMA_VERSION,
            base_snapshot_id: snapshot.snapshot_id,
            batch_id: snapshot.batch_id,
            culling_changes: vec![CompanionCullingChange {
                asset_id,
                decision: Some(CullingUserDecision::Keep),
            }],
            reference_changes: Vec::new(),
        };

        apply_companion_patch(&snapshot, &patch, &culling, &references).unwrap();
        assert_eq!(
            culling.get(asset_id).unwrap().unwrap().decision,
            CullingUserDecision::Keep
        );
        assert_eq!(
            culling.get(unrelated).unwrap().unwrap().decision,
            CullingUserDecision::Review
        );
    }

    #[test]
    fn refuses_silent_overwrite_after_workstation_decision_changed() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("project.sqlite3");
        let culling = CullingReviewStore::open(&db).unwrap();
        let references = ReferenceStore::open(&db).unwrap();
        let asset_id = Uuid::new_v4();
        let group_id = Uuid::new_v4();
        let snapshot = snapshot(asset_id, group_id);
        culling
            .set(asset_id, CullingUserDecision::Review)
            .unwrap();

        let patch = CompanionDecisionPatch {
            schema_version: COMPANION_SNAPSHOT_SCHEMA_VERSION,
            base_snapshot_id: snapshot.snapshot_id,
            batch_id: snapshot.batch_id,
            culling_changes: vec![CompanionCullingChange {
                asset_id,
                decision: Some(CullingUserDecision::Keep),
            }],
            reference_changes: Vec::new(),
        };

        assert!(matches!(
            apply_companion_patch(&snapshot, &patch, &culling, &references),
            Err(CompanionError::ConcurrentCullingChange(id)) if id == asset_id
        ));
    }
}
