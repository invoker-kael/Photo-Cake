use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PhotoCategory {
    Portrait,
    NonPortrait,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SceneTag {
    Landscape,
    Architecture,
    Food,
    Night,
    Document,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassificationSignals {
    pub asset_id: Uuid,
    pub detected_person_count: u32,
    pub detected_face_count: u32,
    pub primary_subject_ratio: f32,
    pub people_confidence: f32,
    pub scene_tags: Vec<SceneTag>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhotoClassification {
    pub asset_id: Uuid,
    pub category: PhotoCategory,
    pub detected_person_count: u32,
    pub detected_face_count: u32,
    pub primary_subject_ratio: f32,
    pub portrait_confidence: f32,
    pub portrait_retouch_eligible: bool,
    pub scene_tags: Vec<SceneTag>,
    pub classifier_id: String,
    pub classifier_version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProcessingRoute {
    Portrait,
    Scene,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ClassificationInvalidationScope {
    SemanticGroupingAndRetouch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassificationInvalidation {
    pub asset_id: Uuid,
    pub scope: ClassificationInvalidationScope,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PortraitClassificationPolicy {
    pub min_people_confidence: f32,
    pub min_primary_subject_ratio: f32,
    pub min_face_count_for_small_subject: u32,
    pub small_subject_ratio: f32,
}

impl Default for PortraitClassificationPolicy {
    fn default() -> Self {
        Self {
            min_people_confidence: 0.65,
            min_primary_subject_ratio: 0.08,
            min_face_count_for_small_subject: 1,
            small_subject_ratio: 0.03,
        }
    }
}

pub fn classify_photo(
    signals: ClassificationSignals,
    classifier_id: impl Into<String>,
    classifier_version: impl Into<String>,
    policy: PortraitClassificationPolicy,
) -> PhotoClassification {
    let has_person = signals.detected_person_count > 0;
    let confident_people = signals.people_confidence >= policy.min_people_confidence;
    let meaningful_subject = signals.primary_subject_ratio >= policy.min_primary_subject_ratio;
    let face_supported_small_subject = signals.primary_subject_ratio >= policy.small_subject_ratio
        && signals.detected_face_count >= policy.min_face_count_for_small_subject;

    let portrait = has_person && confident_people && (meaningful_subject || face_supported_small_subject);
    let portrait_confidence = if portrait {
        signals.people_confidence.max(signals.primary_subject_ratio.min(1.0))
    } else {
        (signals.people_confidence * signals.primary_subject_ratio.clamp(0.0, 1.0)).min(1.0)
    };

    PhotoClassification {
        asset_id: signals.asset_id,
        category: if portrait {
            PhotoCategory::Portrait
        } else {
            PhotoCategory::NonPortrait
        },
        detected_person_count: signals.detected_person_count,
        detected_face_count: signals.detected_face_count,
        primary_subject_ratio: signals.primary_subject_ratio,
        portrait_confidence,
        portrait_retouch_eligible: portrait && signals.detected_face_count > 0,
        scene_tags: signals.scene_tags,
        classifier_id: classifier_id.into(),
        classifier_version: classifier_version.into(),
    }
}

pub fn processing_route(classification: &PhotoClassification) -> ProcessingRoute {
    match classification.category {
        PhotoCategory::Portrait => ProcessingRoute::Portrait,
        PhotoCategory::NonPortrait => ProcessingRoute::Scene,
    }
}

pub fn should_run_portrait_retouch(classification: &PhotoClassification) -> bool {
    classification.category == PhotoCategory::Portrait && classification.portrait_retouch_eligible
}

pub fn reclassification_invalidation(asset_id: Uuid) -> ClassificationInvalidation {
    ClassificationInvalidation {
        asset_id,
        scope: ClassificationInvalidationScope::SemanticGroupingAndRetouch,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signals(
        people: u32,
        faces: u32,
        ratio: f32,
        confidence: f32,
    ) -> ClassificationSignals {
        ClassificationSignals {
            asset_id: Uuid::new_v4(),
            detected_person_count: people,
            detected_face_count: faces,
            primary_subject_ratio: ratio,
            people_confidence: confidence,
            scene_tags: Vec::new(),
        }
    }

    #[test]
    fn meaningful_person_is_portrait() {
        let classification = classify_photo(
            signals(1, 1, 0.35, 0.98),
            "test",
            "1",
            PortraitClassificationPolicy::default(),
        );
        assert_eq!(classification.category, PhotoCategory::Portrait);
        assert!(classification.portrait_retouch_eligible);
        assert_eq!(processing_route(&classification), ProcessingRoute::Portrait);
    }

    #[test]
    fn tiny_background_person_does_not_force_portrait_mode() {
        let classification = classify_photo(
            signals(2, 0, 0.01, 0.96),
            "test",
            "1",
            PortraitClassificationPolicy::default(),
        );
        assert_eq!(classification.category, PhotoCategory::NonPortrait);
        assert!(!should_run_portrait_retouch(&classification));
    }

    #[test]
    fn no_person_is_non_portrait() {
        let classification = classify_photo(
            signals(0, 0, 0.0, 0.0),
            "test",
            "1",
            PortraitClassificationPolicy::default(),
        );
        assert_eq!(classification.category, PhotoCategory::NonPortrait);
        assert_eq!(processing_route(&classification), ProcessingRoute::Scene);
    }

    #[test]
    fn face_can_keep_small_but_meaningful_person_in_portrait_mode() {
        let classification = classify_photo(
            signals(1, 1, 0.05, 0.90),
            "test",
            "1",
            PortraitClassificationPolicy::default(),
        );
        assert_eq!(classification.category, PhotoCategory::Portrait);
    }
}
