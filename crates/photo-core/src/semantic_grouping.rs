use crate::{GroupingBasis, PhotoCategory, PhotoClassification, PhotoGroup, PhotoGroupKind};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageEmbedding {
    pub asset_id: Uuid,
    pub vector: Vec<f32>,
    pub model_id: String,
    pub model_version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SemanticGroupingConfig {
    pub portrait_similarity_threshold: f32,
    pub scene_similarity_threshold: f32,
}

impl Default for SemanticGroupingConfig {
    fn default() -> Self {
        Self {
            portrait_similarity_threshold: 0.88,
            scene_similarity_threshold: 0.84,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SemanticGroupKind {
    PortraitSimilar,
    SceneSimilar,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticPhotoGroup {
    pub id: Uuid,
    pub parent_group_id: Uuid,
    pub kind: SemanticGroupKind,
    pub asset_ids: Vec<Uuid>,
    pub reference_candidate_id: Option<Uuid>,
    pub similarity_threshold: f32,
}

impl SemanticPhotoGroup {
    /// Convert semantic refinement back into the shared PhotoGroup type used by
    /// culling, reference selection and recipe generation.
    pub fn to_photo_group(&self) -> PhotoGroup {
        PhotoGroup {
            id: self.id,
            kind: PhotoGroupKind::Similar,
            basis: GroupingBasis::SemanticSimilarity,
            asset_ids: self.asset_ids.clone(),
            manual_locked: false,
        }
    }
}

#[derive(Debug, Error)]
pub enum SemanticGroupingError {
    #[error("classification missing for asset {0}")]
    MissingClassification(Uuid),
    #[error("embedding missing for asset {0}")]
    MissingEmbedding(Uuid),
    #[error("embedding vector is empty for asset {0}")]
    EmptyEmbedding(Uuid),
    #[error("embedding dimensions differ within one group")]
    DimensionMismatch,
}

pub fn refine_group_by_similarity(
    parent: &PhotoGroup,
    classifications: &[PhotoClassification],
    embeddings: &[ImageEmbedding],
    config: SemanticGroupingConfig,
) -> Result<Vec<SemanticPhotoGroup>, SemanticGroupingError> {
    let mut portrait = Vec::new();
    let mut scene = Vec::new();

    for asset_id in &parent.asset_ids {
        let classification = classifications
            .iter()
            .find(|value| value.asset_id == *asset_id)
            .ok_or(SemanticGroupingError::MissingClassification(*asset_id))?;
        let embedding = embeddings
            .iter()
            .find(|value| value.asset_id == *asset_id)
            .ok_or(SemanticGroupingError::MissingEmbedding(*asset_id))?;
        if embedding.vector.is_empty() {
            return Err(SemanticGroupingError::EmptyEmbedding(*asset_id));
        }

        match classification.category {
            PhotoCategory::Portrait => portrait.push(embedding),
            PhotoCategory::NonPortrait => scene.push(embedding),
        }
    }

    let mut groups = Vec::new();
    groups.extend(cluster_category(
        parent.id,
        &portrait,
        SemanticGroupKind::PortraitSimilar,
        config.portrait_similarity_threshold,
    )?);
    groups.extend(cluster_category(
        parent.id,
        &scene,
        SemanticGroupKind::SceneSimilar,
        config.scene_similarity_threshold,
    )?);
    Ok(groups)
}

fn cluster_category(
    parent_group_id: Uuid,
    embeddings: &[&ImageEmbedding],
    kind: SemanticGroupKind,
    threshold: f32,
) -> Result<Vec<SemanticPhotoGroup>, SemanticGroupingError> {
    if embeddings.is_empty() {
        return Ok(Vec::new());
    }

    let dimensions = embeddings[0].vector.len();
    if embeddings.iter().any(|item| item.vector.len() != dimensions) {
        return Err(SemanticGroupingError::DimensionMismatch);
    }

    let mut clusters: Vec<Vec<&ImageEmbedding>> = Vec::new();
    for embedding in embeddings {
        let mut best_index = None;
        let mut best_similarity = f32::NEG_INFINITY;

        for (index, cluster) in clusters.iter().enumerate() {
            let centroid = centroid(cluster, dimensions);
            let similarity = cosine_similarity(&embedding.vector, &centroid);
            if similarity >= threshold && similarity > best_similarity {
                best_index = Some(index);
                best_similarity = similarity;
            }
        }

        if let Some(index) = best_index {
            clusters[index].push(*embedding);
        } else {
            clusters.push(vec![*embedding]);
        }
    }

    Ok(clusters
        .into_iter()
        .map(|cluster| {
            let reference_candidate_id = choose_medoid(&cluster, dimensions);
            SemanticPhotoGroup {
                id: Uuid::new_v4(),
                parent_group_id,
                kind,
                asset_ids: cluster.iter().map(|item| item.asset_id).collect(),
                reference_candidate_id,
                similarity_threshold: threshold,
            }
        })
        .collect())
}

fn centroid(items: &[&ImageEmbedding], dimensions: usize) -> Vec<f32> {
    let mut centroid = vec![0.0f32; dimensions];
    for item in items {
        for (target, value) in centroid.iter_mut().zip(&item.vector) {
            *target += *value;
        }
    }
    let divisor = items.len().max(1) as f32;
    for value in &mut centroid {
        *value /= divisor;
    }
    centroid
}

fn choose_medoid(items: &[&ImageEmbedding], dimensions: usize) -> Option<Uuid> {
    if items.is_empty() {
        return None;
    }
    let center = centroid(items, dimensions);
    items
        .iter()
        .max_by(|left, right| {
            cosine_similarity(&left.vector, &center)
                .total_cmp(&cosine_similarity(&right.vector, &center))
        })
        .map(|item| item.asset_id)
}

fn cosine_similarity(left: &[f32], right: &[f32]) -> f32 {
    let mut dot = 0.0f32;
    let mut left_norm = 0.0f32;
    let mut right_norm = 0.0f32;
    for (left, right) in left.iter().zip(right) {
        dot += left * right;
        left_norm += left * left;
        right_norm += right * right;
    }
    if left_norm <= f32::EPSILON || right_norm <= f32::EPSILON {
        return 0.0;
    }
    dot / (left_norm.sqrt() * right_norm.sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GroupingBasis, PhotoGroupKind};

    fn classification(asset_id: Uuid, category: PhotoCategory) -> PhotoClassification {
        PhotoClassification {
            asset_id,
            category,
            detected_person_count: if category == PhotoCategory::Portrait { 1 } else { 0 },
            detected_face_count: if category == PhotoCategory::Portrait { 1 } else { 0 },
            primary_subject_ratio: if category == PhotoCategory::Portrait { 0.4 } else { 0.0 },
            portrait_confidence: if category == PhotoCategory::Portrait { 0.9 } else { 0.0 },
            portrait_retouch_eligible: category == PhotoCategory::Portrait,
            scene_tags: Vec::new(),
            classifier_id: "test".to_string(),
            classifier_version: "1".to_string(),
        }
    }

    fn embedding(asset_id: Uuid, vector: &[f32]) -> ImageEmbedding {
        ImageEmbedding {
            asset_id,
            vector: vector.to_vec(),
            model_id: "embed".to_string(),
            model_version: "1".to_string(),
        }
    }

    fn parent(asset_ids: Vec<Uuid>) -> PhotoGroup {
        PhotoGroup {
            id: Uuid::new_v4(),
            kind: PhotoGroupKind::Moment,
            basis: GroupingBasis::Time,
            asset_ids,
            manual_locked: false,
        }
    }

    #[test]
    fn portrait_and_scene_never_merge_even_with_same_embedding() {
        let portrait = Uuid::new_v4();
        let scene = Uuid::new_v4();
        let parent = parent(vec![portrait, scene]);
        let groups = refine_group_by_similarity(
            &parent,
            &[
                classification(portrait, PhotoCategory::Portrait),
                classification(scene, PhotoCategory::NonPortrait),
            ],
            &[
                embedding(portrait, &[1.0, 0.0]),
                embedding(scene, &[1.0, 0.0]),
            ],
            SemanticGroupingConfig::default(),
        )
        .unwrap();
        assert_eq!(groups.len(), 2);
        assert_ne!(groups[0].kind, groups[1].kind);
    }

    #[test]
    fn visually_similar_scene_images_cluster_together() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let third = Uuid::new_v4();
        let parent = parent(vec![first, second, third]);
        let classifications = vec![
            classification(first, PhotoCategory::NonPortrait),
            classification(second, PhotoCategory::NonPortrait),
            classification(third, PhotoCategory::NonPortrait),
        ];
        let groups = refine_group_by_similarity(
            &parent,
            &classifications,
            &[
                embedding(first, &[1.0, 0.0]),
                embedding(second, &[0.99, 0.04]),
                embedding(third, &[0.0, 1.0]),
            ],
            SemanticGroupingConfig {
                portrait_similarity_threshold: 0.9,
                scene_similarity_threshold: 0.9,
            },
        )
        .unwrap();
        assert_eq!(groups.len(), 2);
        assert!(groups.iter().any(|group| group.asset_ids.len() == 2));
    }

    #[test]
    fn semantic_group_promotes_into_shared_photo_group() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let parent = parent(vec![first, second]);
        let groups = refine_group_by_similarity(
            &parent,
            &[
                classification(first, PhotoCategory::NonPortrait),
                classification(second, PhotoCategory::NonPortrait),
            ],
            &[
                embedding(first, &[1.0, 0.0]),
                embedding(second, &[0.99, 0.02]),
            ],
            SemanticGroupingConfig {
                portrait_similarity_threshold: 0.9,
                scene_similarity_threshold: 0.9,
            },
        )
        .unwrap();

        let promoted = groups[0].to_photo_group();
        assert_eq!(promoted.kind, PhotoGroupKind::Similar);
        assert_eq!(promoted.basis, GroupingBasis::SemanticSimilarity);
        assert_eq!(promoted.asset_ids.len(), 2);
    }

    #[test]
    fn output_groups_remain_scoped_to_parent_group() {
        let asset = Uuid::new_v4();
        let parent = parent(vec![asset]);
        let groups = refine_group_by_similarity(
            &parent,
            &[classification(asset, PhotoCategory::Portrait)],
            &[embedding(asset, &[1.0, 0.0])],
            SemanticGroupingConfig::default(),
        )
        .unwrap();
        assert_eq!(groups[0].parent_group_id, parent.id);
    }
}
