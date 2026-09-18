use crate::{
    detect_exposure_brackets, refine_group_by_similarity, AnalysisCache, AnalysisCacheError,
    CatalogError, ClassificationStore, ClassificationStoreError, ExposureBracketError,
    GroupingBasis, ImageEmbedding, InferenceTask, PhotoGroup, RawCatalog, SemanticGroupingConfig,
    SemanticGroupingError,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticRefinementReport {
    pub collection_id: Uuid,
    pub refined_parent_group_ids: Vec<Uuid>,
    #[serde(default)]
    pub protected_bracket_parent_group_ids: Vec<Uuid>,
    pub pending_asset_ids: Vec<Uuid>,
    pub effective_groups: Vec<PhotoGroup>,
}

#[derive(Debug, Error)]
pub enum SemanticRefinementError {
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    #[error(transparent)]
    Classification(#[from] ClassificationStoreError),
    #[error(transparent)]
    Analysis(#[from] AnalysisCacheError),
    #[error(transparent)]
    Bracketing(#[from] ExposureBracketError),
    #[error(transparent)]
    Grouping(#[from] SemanticGroupingError),
    #[error("invalid cached embedding for asset {asset_id}: {message}")]
    InvalidEmbedding { asset_id: Uuid, message: String },
}

/// Persist semantic child groups for every fully analyzed automatic moment group.
///
/// Moment groups remain authoritative parents. Missing evidence leaves the
/// parent untouched so partial analysis never destroys an earlier refinement.
pub fn refine_collection_semantic_groups(
    catalog: &RawCatalog,
    classifications: &ClassificationStore,
    analysis: &AnalysisCache,
    collection_id: Uuid,
    config: SemanticGroupingConfig,
) -> Result<SemanticRefinementReport, SemanticRefinementError> {
    let parents = catalog.list_groups_for_collection(collection_id)?;
    let mut refined_parent_group_ids = Vec::new();
    let mut protected_bracket_parent_group_ids = Vec::new();
    let mut pending_asset_ids = HashSet::new();

    for parent in parents {
        if parent.manual_locked || parent.basis == GroupingBasis::SemanticSimilarity {
            continue;
        }

        let mut parent_classifications = Vec::with_capacity(parent.asset_ids.len());
        let mut embeddings = Vec::with_capacity(parent.asset_ids.len());
        let mut ready = true;

        for asset_id in &parent.asset_ids {
            match classifications.get(*asset_id)? {
                Some(value) => parent_classifications.push(value),
                None => {
                    pending_asset_ids.insert(*asset_id);
                    ready = false;
                }
            }

            match analysis.latest_for_asset_task(*asset_id, InferenceTask::ImageEmbedding)? {
                Some(artifact) => {
                    let vector = artifact
                        .payload_json
                        .get("embedding")
                        .cloned()
                        .ok_or_else(|| SemanticRefinementError::InvalidEmbedding {
                            asset_id: *asset_id,
                            message: "missing embedding field".to_string(),
                        })
                        .and_then(|value| {
                            serde_json::from_value::<Vec<f32>>(value).map_err(|error| {
                                SemanticRefinementError::InvalidEmbedding {
                                    asset_id: *asset_id,
                                    message: error.to_string(),
                                }
                            })
                        })?;
                    embeddings.push(ImageEmbedding {
                        asset_id: *asset_id,
                        vector,
                        model_id: artifact.key.model_id,
                        model_version: artifact.key.model_version,
                    });
                }
                None => {
                    pending_asset_ids.insert(*asset_id);
                    ready = false;
                }
            }
        }

        if !ready {
            continue;
        }

        let brackets = detect_exposure_brackets(analysis, &parent)?;
        if brackets.len() == 1 && brackets[0].members.len() == parent.asset_ids.len() {
            catalog.replace_semantic_groups(parent.id, &[])?;
            protected_bracket_parent_group_ids.push(parent.id);
            continue;
        }

        let existing = catalog.list_semantic_groups_for_parent(parent.id)?;
        let mut refined = refine_group_by_similarity(
            &parent,
            &parent_classifications,
            &embeddings,
            config,
        )?;
        preserve_semantic_group_ids(&existing, &mut refined);
        catalog.replace_semantic_groups(parent.id, &refined)?;
        refined_parent_group_ids.push(parent.id);
    }

    let mut pending_asset_ids = pending_asset_ids.into_iter().collect::<Vec<_>>();
    pending_asset_ids.sort_by_key(|value| value.as_u128());

    Ok(SemanticRefinementReport {
        collection_id,
        refined_parent_group_ids,
        protected_bracket_parent_group_ids,
        pending_asset_ids,
        effective_groups: catalog.list_effective_groups_for_collection(collection_id)?,
    })
}

fn preserve_semantic_group_ids(
    existing: &[crate::SemanticPhotoGroup],
    refined: &mut [crate::SemanticPhotoGroup],
) {
    for group in refined {
        if let Some(previous) = existing.iter().find(|candidate| {
            candidate.kind == group.kind
                && same_asset_members(&candidate.asset_ids, &group.asset_ids)
        }) {
            group.id = previous.id;
        }
    }
}

fn same_asset_members(left: &[Uuid], right: &[Uuid]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut left = left.iter().map(|value| value.as_u128()).collect::<Vec<_>>();
    let mut right = right.iter().map(|value| value.as_u128()).collect::<Vec<_>>();
    left.sort_unstable();
    right.sort_unstable();
    left == right
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        classify_photo, initial_group_raw_assets, AnalysisArtifact, AnalysisCacheKey,
        ClassificationSignals, InitialGroupingConfig, PortraitClassificationPolicy,
        RawAsset, SceneTag,
    };
    use tempfile::tempdir;

    fn asset(path: &str, sequence: u64) -> RawAsset {
        RawAsset {
            id: Uuid::new_v4(),
            source_path: path.into(),
            filename: path.into(),
            extension: "cr3".into(),
            camera_id: Some("camera".into()),
            capture_time_ms: Some(sequence as i64 * 500),
            file_time_ms: None,
            sequence_number: Some(sequence),
        }
    }

    fn save_exposure(analysis: &AnalysisCache, asset_id: Uuid, exposure_ev: f32) {
        analysis
            .put(&AnalysisArtifact {
                key: AnalysisCacheKey {
                    asset_id,
                    source_fingerprint: "raw".into(),
                    preview_revision: "preview".into(),
                    task: InferenceTask::ExposureAnalysis,
                    model_id: "preview-relative-exposure".into(),
                    model_version: "1".into(),
                    config_hash: "test".into(),
                },
                payload_json: serde_json::to_value(crate::PhotoColorAnalysis {
                    asset_id,
                    exposure_ev,
                    temperature_k: None,
                    tint: None,
                    confidence: 0.95,
                })
                .unwrap(),
            })
            .unwrap();
    }

    fn save_evidence(
        classifications: &ClassificationStore,
        analysis: &AnalysisCache,
        asset_id: Uuid,
        vector: Vec<f32>,
        portrait: bool,
    ) {
        classifications
            .save(&classify_photo(
                ClassificationSignals {
                    asset_id,
                    detected_person_count: if portrait { 1 } else { 0 },
                    detected_face_count: if portrait { 1 } else { 0 },
                    primary_subject_ratio: if portrait { 0.4 } else { 0.0 },
                    people_confidence: if portrait { 0.95 } else { 0.0 },
                    scene_tags: vec![SceneTag::Other],
                },
                "classifier",
                "1",
                PortraitClassificationPolicy::default(),
            ))
            .unwrap();
        analysis
            .put(&AnalysisArtifact {
                key: AnalysisCacheKey {
                    asset_id,
                    source_fingerprint: "raw".into(),
                    preview_revision: "preview".into(),
                    task: InferenceTask::ImageEmbedding,
                    model_id: "embedding".into(),
                    model_version: "1".into(),
                    config_hash: "default".into(),
                },
                payload_json: serde_json::json!({ "embedding": vector }),
            })
            .unwrap();
    }

    #[test]
    fn complete_evidence_persists_effective_semantic_groups() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("project.sqlite3");
        let catalog = RawCatalog::open(&db).unwrap();
        let classifications = ClassificationStore::open(&db).unwrap();
        let analysis = AnalysisCache::open(&db).unwrap();
        let assets = catalog
            .ensure_assets(&[
                asset("one.cr3", 1),
                asset("two.cr3", 2),
                asset("three.cr3", 3),
            ])
            .unwrap();
        let collection = Uuid::new_v4();
        let parents = initial_group_raw_assets(&assets, InitialGroupingConfig::default());
        catalog
            .replace_automatic_groups(collection, &parents)
            .unwrap();

        save_evidence(&classifications, &analysis, assets[0].id, vec![1.0, 0.0], false);
        save_evidence(&classifications, &analysis, assets[1].id, vec![0.99, 0.02], false);
        save_evidence(&classifications, &analysis, assets[2].id, vec![0.0, 1.0], false);

        let report = refine_collection_semantic_groups(
            &catalog,
            &classifications,
            &analysis,
            collection,
            SemanticGroupingConfig {
                portrait_similarity_threshold: 0.9,
                scene_similarity_threshold: 0.9,
            },
        )
        .unwrap();

        assert_eq!(report.refined_parent_group_ids.len(), 1);
        assert!(report.pending_asset_ids.is_empty());
        assert_eq!(catalog.list_groups_for_collection(collection).unwrap().len(), 1);
        assert_eq!(report.effective_groups.len(), 2);
        assert!(report
            .effective_groups
            .iter()
            .all(|group| group.basis == GroupingBasis::SemanticSimilarity));
    }

    #[test]
    fn full_exposure_bracket_parent_is_preserved_from_semantic_splitting() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("project.sqlite3");
        let catalog = RawCatalog::open(&db).unwrap();
        let classifications = ClassificationStore::open(&db).unwrap();
        let analysis = AnalysisCache::open(&db).unwrap();
        let assets = catalog
            .ensure_assets(&[
                asset("base.cr3", 1),
                asset("under.cr3", 2),
                asset("over.cr3", 3),
            ])
            .unwrap();
        let collection = Uuid::new_v4();
        let parents = initial_group_raw_assets(&assets, InitialGroupingConfig::default());
        catalog
            .replace_automatic_groups(collection, &parents)
            .unwrap();

        for (asset, exposure, vector) in [
            (&assets[0], 0.0, vec![1.0, 0.0]),
            (&assets[1], -1.0, vec![0.999, 0.01]),
            (&assets[2], 1.0, vec![0.998, -0.01]),
        ] {
            save_evidence(&classifications, &analysis, asset.id, vector, false);
            save_exposure(&analysis, asset.id, exposure);
        }

        let report = refine_collection_semantic_groups(
            &catalog,
            &classifications,
            &analysis,
            collection,
            SemanticGroupingConfig {
                portrait_similarity_threshold: 1.0,
                scene_similarity_threshold: 1.0,
            },
        )
        .unwrap();

        assert!(report.refined_parent_group_ids.is_empty());
        assert_eq!(report.protected_bracket_parent_group_ids, vec![parents[0].id]);
        assert_eq!(report.effective_groups, parents);
    }

    #[test]
    fn repeated_refinement_preserves_semantic_group_ids_when_membership_is_unchanged() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("project.sqlite3");
        let catalog = RawCatalog::open(&db).unwrap();
        let classifications = ClassificationStore::open(&db).unwrap();
        let analysis = AnalysisCache::open(&db).unwrap();
        let assets = catalog
            .ensure_assets(&[
                asset("one.cr3", 1),
                asset("two.cr3", 2),
                asset("three.cr3", 3),
            ])
            .unwrap();
        let collection = Uuid::new_v4();
        let parents = initial_group_raw_assets(&assets, InitialGroupingConfig::default());
        catalog
            .replace_automatic_groups(collection, &parents)
            .unwrap();

        save_evidence(&classifications, &analysis, assets[0].id, vec![1.0, 0.0], false);
        save_evidence(&classifications, &analysis, assets[1].id, vec![0.99, 0.02], false);
        save_evidence(&classifications, &analysis, assets[2].id, vec![0.0, 1.0], false);

        let config = SemanticGroupingConfig {
            portrait_similarity_threshold: 0.9,
            scene_similarity_threshold: 0.9,
        };

        refine_collection_semantic_groups(
            &catalog,
            &classifications,
            &analysis,
            collection,
            config,
        )
        .unwrap();
        let first = catalog
            .list_semantic_groups_for_parent(parents[0].id)
            .unwrap();

        refine_collection_semantic_groups(
            &catalog,
            &classifications,
            &analysis,
            collection,
            config,
        )
        .unwrap();
        let second = catalog
            .list_semantic_groups_for_parent(parents[0].id)
            .unwrap();

        let mut first_ids = first.iter().map(|group| group.id).collect::<Vec<_>>();
        let mut second_ids = second.iter().map(|group| group.id).collect::<Vec<_>>();
        first_ids.sort_by_key(|value| value.as_u128());
        second_ids.sort_by_key(|value| value.as_u128());

        assert_eq!(first_ids, second_ids);
    }

    #[test]
    fn missing_evidence_keeps_parent_and_reports_pending_asset() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("project.sqlite3");
        let catalog = RawCatalog::open(&db).unwrap();
        let classifications = ClassificationStore::open(&db).unwrap();
        let analysis = AnalysisCache::open(&db).unwrap();
        let assets = catalog
            .ensure_assets(&[asset("one.cr3", 1), asset("two.cr3", 2)])
            .unwrap();
        let collection = Uuid::new_v4();
        let parents = initial_group_raw_assets(&assets, InitialGroupingConfig::default());
        catalog
            .replace_automatic_groups(collection, &parents)
            .unwrap();

        save_evidence(&classifications, &analysis, assets[0].id, vec![1.0, 0.0], false);

        let report = refine_collection_semantic_groups(
            &catalog,
            &classifications,
            &analysis,
            collection,
            SemanticGroupingConfig::default(),
        )
        .unwrap();

        assert!(report.refined_parent_group_ids.is_empty());
        assert_eq!(report.pending_asset_ids, vec![assets[1].id]);
        assert_eq!(report.effective_groups, parents);
    }
}
