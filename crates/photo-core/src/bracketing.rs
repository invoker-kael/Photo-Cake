use crate::{
    embedding_similarity, AnalysisCache, AnalysisCacheError, InferenceTask, PhotoColorAnalysis,
    PhotoGroup,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use thiserror::Error;
use uuid::Uuid;

const BRACKET_WINDOW_SIZES: [usize; 4] = [9, 7, 5, 3];
const MIN_SPAN_EV: f32 = 0.6;
const MIN_STEP_EV: f32 = 0.18;
const SYMMETRY_TOLERANCE_EV: f32 = 0.5;
const MIN_EMBEDDING_SIMILARITY: f32 = 0.94;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExposureBracketRole {
    Under,
    Base,
    Over,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExposureBracketMember {
    pub asset_id: Uuid,
    pub exposure_ev: f32,
    pub offset_from_center_ev: f32,
    pub role: ExposureBracketRole,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExposureBracketSet {
    pub group_id: Uuid,
    pub center_asset_id: Uuid,
    pub members: Vec<ExposureBracketMember>,
    pub span_ev: f32,
    pub minimum_embedding_similarity: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExposureBracketRouting {
    pub sets: Vec<ExposureBracketSet>,
    pub source_asset_ids: Vec<Uuid>,
    pub standard_asset_ids: Vec<Uuid>,
}

#[derive(Debug, Error)]
pub enum ExposureBracketError {
    #[error(transparent)]
    Analysis(#[from] AnalysisCacheError),
    #[error("invalid cached exposure analysis for asset {asset_id}: {source}")]
    InvalidExposure {
        asset_id: Uuid,
        source: serde_json::Error,
    },
    #[error("invalid cached image embedding for asset {asset_id}: {source}")]
    InvalidEmbedding {
        asset_id: Uuid,
        source: serde_json::Error,
    },
}

#[derive(Debug, Clone)]
struct BracketObservation {
    asset_id: Uuid,
    exposure_ev: f32,
    embedding: Vec<f32>,
}

/// Detect conservative AEB/HDR source sets from evidence already produced by Analyze.
///
/// Detection requires both a symmetric exposure ladder and highly similar image
/// embeddings, so ordinary brightness changes across unrelated photos are not
/// treated as an exposure bracket.
pub fn detect_exposure_brackets(
    cache: &AnalysisCache,
    group: &PhotoGroup,
) -> Result<Vec<ExposureBracketSet>, ExposureBracketError> {
    let mut observations = Vec::with_capacity(group.asset_ids.len());
    for asset_id in &group.asset_ids {
        observations.push(load_observation(cache, *asset_id)?);
    }

    let mut sets = Vec::new();
    let mut start = 0usize;
    while start + 3 <= observations.len() {
        let mut matched = None;
        for size in BRACKET_WINDOW_SIZES {
            if start + size > observations.len() {
                continue;
            }
            if let Some(candidate) = build_candidate(group.id, &observations[start..start + size]) {
                matched = Some(candidate);
                break;
            }
        }

        if let Some(candidate) = matched {
            start += candidate.members.len();
            sets.push(candidate);
        } else {
            start += 1;
        }
    }

    Ok(sets)
}

/// Split one photography group into intentional HDR/AEB source frames and
/// ordinary frames that may continue through the standard Reference -> Recipe
/// -> Lightroom XMP path. Asset order remains identical to the group order.
pub fn route_exposure_bracket_sources(
    cache: &AnalysisCache,
    group: &PhotoGroup,
) -> Result<ExposureBracketRouting, ExposureBracketError> {
    let sets = detect_exposure_brackets(cache, group)?;
    let source_ids = sets
        .iter()
        .flat_map(|set| set.members.iter().map(|member| member.asset_id))
        .collect::<HashSet<_>>();
    let source_asset_ids = group
        .asset_ids
        .iter()
        .copied()
        .filter(|asset_id| source_ids.contains(asset_id))
        .collect::<Vec<_>>();
    let standard_asset_ids = group
        .asset_ids
        .iter()
        .copied()
        .filter(|asset_id| !source_ids.contains(asset_id))
        .collect::<Vec<_>>();

    Ok(ExposureBracketRouting {
        sets,
        source_asset_ids,
        standard_asset_ids,
    })
}

fn load_observation(
    cache: &AnalysisCache,
    asset_id: Uuid,
) -> Result<Option<BracketObservation>, ExposureBracketError> {
    let Some(exposure_artifact) =
        cache.latest_for_asset_task(asset_id, InferenceTask::ExposureAnalysis)?
    else {
        return Ok(None);
    };
    let exposure = serde_json::from_value::<PhotoColorAnalysis>(exposure_artifact.payload_json)
        .map_err(|source| ExposureBracketError::InvalidExposure { asset_id, source })?;

    let Some(embedding_artifact) =
        cache.latest_for_asset_task(asset_id, InferenceTask::ImageEmbedding)?
    else {
        return Ok(None);
    };
    let Some(value) = embedding_artifact.payload_json.get("embedding").cloned() else {
        return Ok(None);
    };
    let embedding = serde_json::from_value::<Vec<f32>>(value)
        .map_err(|source| ExposureBracketError::InvalidEmbedding { asset_id, source })?;
    if embedding.is_empty() {
        return Ok(None);
    }

    Ok(Some(BracketObservation {
        asset_id,
        exposure_ev: exposure.exposure_ev,
        embedding,
    }))
}

fn build_candidate(
    group_id: Uuid,
    window: &[Option<BracketObservation>],
) -> Option<ExposureBracketSet> {
    let observations = window
        .iter()
        .map(Option::as_ref)
        .collect::<Option<Vec<_>>>()?;
    if observations.len() < 3 || observations.len() % 2 == 0 {
        return None;
    }

    let mut exposure_order = observations.clone();
    exposure_order.sort_by(|left, right| left.exposure_ev.total_cmp(&right.exposure_ev));

    let span_ev = exposure_order.last()?.exposure_ev - exposure_order.first()?.exposure_ev;
    if span_ev < MIN_SPAN_EV {
        return None;
    }
    if exposure_order
        .windows(2)
        .any(|pair| pair[1].exposure_ev - pair[0].exposure_ev < MIN_STEP_EV)
    {
        return None;
    }

    let center = exposure_order[exposure_order.len() / 2];
    for index in 0..exposure_order.len() / 2 {
        let low = exposure_order[index].exposure_ev;
        let high = exposure_order[exposure_order.len() - 1 - index].exposure_ev;
        let midpoint = (low + high) / 2.0;
        if (midpoint - center.exposure_ev).abs() > SYMMETRY_TOLERANCE_EV {
            return None;
        }
    }

    let mut minimum_embedding_similarity = 1.0f32;
    for observation in &observations {
        if observation.embedding.len() != center.embedding.len() {
            return None;
        }
        let similarity = embedding_similarity(&observation.embedding, &center.embedding);
        minimum_embedding_similarity = minimum_embedding_similarity.min(similarity);
        if similarity < MIN_EMBEDDING_SIMILARITY {
            return None;
        }
    }

    let members = observations
        .into_iter()
        .map(|observation| {
            let offset = observation.exposure_ev - center.exposure_ev;
            let role = if observation.asset_id == center.asset_id {
                ExposureBracketRole::Base
            } else if offset < 0.0 {
                ExposureBracketRole::Under
            } else {
                ExposureBracketRole::Over
            };
            ExposureBracketMember {
                asset_id: observation.asset_id,
                exposure_ev: observation.exposure_ev,
                offset_from_center_ev: offset,
                role,
            }
        })
        .collect();

    Some(ExposureBracketSet {
        group_id,
        center_asset_id: center.asset_id,
        members,
        span_ev,
        minimum_embedding_similarity,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AnalysisArtifact, AnalysisCacheKey, GroupingBasis, PhotoGroupKind};
    use tempfile::tempdir;

    fn put_observation(
        cache: &AnalysisCache,
        asset_id: Uuid,
        exposure_ev: f32,
        embedding: Vec<f32>,
    ) {
        cache
            .put(&AnalysisArtifact {
                key: AnalysisCacheKey {
                    asset_id,
                    source_fingerprint: "raw-v1".into(),
                    preview_revision: "preview-v1".into(),
                    task: InferenceTask::ExposureAnalysis,
                    model_id: "preview-relative-exposure".into(),
                    model_version: "1".into(),
                    config_hash: "test".into(),
                },
                payload_json: serde_json::to_value(PhotoColorAnalysis {
                    asset_id,
                    exposure_ev,
                    temperature_k: None,
                    tint: None,
                    confidence: 0.95,
                })
                .unwrap(),
            })
            .unwrap();
        cache
            .put(&AnalysisArtifact {
                key: AnalysisCacheKey {
                    asset_id,
                    source_fingerprint: "raw-v1".into(),
                    preview_revision: "preview-v1".into(),
                    task: InferenceTask::ImageEmbedding,
                    model_id: "embedding".into(),
                    model_version: "1".into(),
                    config_hash: "test".into(),
                },
                payload_json: serde_json::json!({ "embedding": embedding }),
            })
            .unwrap();
    }

    fn group(asset_ids: Vec<Uuid>) -> PhotoGroup {
        PhotoGroup {
            id: Uuid::new_v4(),
            kind: PhotoGroupKind::Moment,
            basis: GroupingBasis::TimeAndSequence,
            asset_ids,
            manual_locked: false,
        }
    }

    #[test]
    fn detects_three_frame_bracket_even_when_base_is_captured_first() {
        let dir = tempdir().unwrap();
        let cache = AnalysisCache::open(dir.path().join("project.sqlite3")).unwrap();
        let base = Uuid::new_v4();
        let under = Uuid::new_v4();
        let over = Uuid::new_v4();
        put_observation(&cache, base, 0.0, vec![1.0, 0.0]);
        put_observation(&cache, under, -1.2, vec![0.999, 0.01]);
        put_observation(&cache, over, 1.1, vec![0.998, -0.01]);

        let detected =
            detect_exposure_brackets(&cache, &group(vec![base, under, over])).unwrap();

        assert_eq!(detected.len(), 1);
        assert_eq!(detected[0].center_asset_id, base);
        assert_eq!(detected[0].members.len(), 3);
        assert!(detected[0].span_ev > 2.0);
    }

    #[test]
    fn repeated_three_frame_sequences_are_kept_as_separate_brackets() {
        let dir = tempdir().unwrap();
        let cache = AnalysisCache::open(dir.path().join("project.sqlite3")).unwrap();
        let ids = (0..6).map(|_| Uuid::new_v4()).collect::<Vec<_>>();
        for (index, exposure) in [0.0, -1.0, 1.0, 0.0, -1.0, 1.0]
            .into_iter()
            .enumerate()
        {
            put_observation(
                &cache,
                ids[index],
                exposure,
                vec![1.0, index as f32 * 0.001],
            );
        }

        let detected = detect_exposure_brackets(&cache, &group(ids)).unwrap();
        assert_eq!(detected.len(), 2);
        assert!(detected.iter().all(|value| value.members.len() == 3));
    }

    #[test]
    fn routing_keeps_bracket_sources_out_of_standard_recipe_targets() {
        let dir = tempdir().unwrap();
        let cache = AnalysisCache::open(dir.path().join("project.sqlite3")).unwrap();
        let base = Uuid::new_v4();
        let under = Uuid::new_v4();
        let over = Uuid::new_v4();
        let normal = Uuid::new_v4();
        put_observation(&cache, base, 0.0, vec![1.0, 0.0]);
        put_observation(&cache, under, -1.0, vec![0.999, 0.01]);
        put_observation(&cache, over, 1.0, vec![0.998, -0.01]);
        put_observation(&cache, normal, 3.2, vec![0.0, 1.0]);

        let routing =
            route_exposure_bracket_sources(&cache, &group(vec![base, under, over, normal]))
                .unwrap();

        assert_eq!(routing.sets.len(), 1);
        assert_eq!(routing.source_asset_ids, vec![base, under, over]);
        assert_eq!(routing.standard_asset_ids, vec![normal]);
    }

    #[test]
    fn scene_change_blocks_false_bracket_detection() {
        let dir = tempdir().unwrap();
        let cache = AnalysisCache::open(dir.path().join("project.sqlite3")).unwrap();
        let ids = (0..3).map(|_| Uuid::new_v4()).collect::<Vec<_>>();
        put_observation(&cache, ids[0], -1.0, vec![1.0, 0.0]);
        put_observation(&cache, ids[1], 0.0, vec![0.0, 1.0]);
        put_observation(&cache, ids[2], 1.0, vec![1.0, 0.0]);

        assert!(detect_exposure_brackets(&cache, &group(ids))
            .unwrap()
            .is_empty());
    }

    #[test]
    fn non_symmetric_exposure_run_is_not_called_a_bracket() {
        let dir = tempdir().unwrap();
        let cache = AnalysisCache::open(dir.path().join("project.sqlite3")).unwrap();
        let ids = (0..3).map(|_| Uuid::new_v4()).collect::<Vec<_>>();
        put_observation(&cache, ids[0], -2.0, vec![1.0, 0.0]);
        put_observation(&cache, ids[1], -0.1, vec![0.999, 0.01]);
        put_observation(&cache, ids[2], 0.5, vec![0.998, -0.01]);

        assert!(detect_exposure_brackets(&cache, &group(ids))
            .unwrap()
            .is_empty());
    }
}
