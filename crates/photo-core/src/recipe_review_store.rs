use crate::Recipe;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

const NEUTRAL_EPSILON: f32 = 0.0001;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS recipe_review_overrides (
    asset_id TEXT PRIMARY KEY NOT NULL,
    override_json TEXT NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS recipe_review_confirmations (
    asset_id TEXT PRIMARY KEY NOT NULL,
    recipe_fingerprint TEXT NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL
);
"#;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipeReviewConfirmation {
    pub asset_id: Uuid,
    pub recipe_fingerprint: String,
}

#[derive(Serialize)]
struct RecipeFingerprintPayload<'a> {
    schema: &'static str,
    target_asset_id: Uuid,
    source_reference_ids: &'a [Uuid],
    adjustments: &'a crate::EditAdjustments,
}

pub fn recipe_review_fingerprint(
    recipe: &Recipe,
) -> Result<String, RecipeReviewStoreError> {
    let target_asset_id = recipe
        .target_asset_id
        .ok_or(RecipeReviewStoreError::RecipeMissingTarget(recipe.id))?;
    let payload = RecipeFingerprintPayload {
        schema: "photo-cake-recipe-review-v1",
        target_asset_id,
        source_reference_ids: &recipe.source_reference_ids,
        adjustments: &recipe.adjustments,
    };
    Ok(stable_fingerprint(&serde_json::to_vec(&payload)?))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipeReviewSyncFields {
    pub exposure: bool,
    #[serde(default)]
    pub highlights: bool,
    #[serde(default)]
    pub shadows: bool,
    #[serde(default)]
    pub whites: bool,
    #[serde(default)]
    pub blacks: bool,
    pub contrast: bool,
    pub saturation: bool,
}

impl RecipeReviewSyncFields {
    pub fn any(self) -> bool {
        self.exposure
            || self.highlights
            || self.shadows
            || self.whites
            || self.blacks
            || self.contrast
            || self.saturation
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecipeReviewOverride {
    pub asset_id: Uuid,
    pub exposure_delta_ev: f32,
    #[serde(default)]
    pub highlights_delta: f32,
    #[serde(default)]
    pub shadows_delta: f32,
    #[serde(default)]
    pub whites_delta: f32,
    #[serde(default)]
    pub blacks_delta: f32,
    pub contrast_delta: f32,
    pub saturation_delta: f32,
}

impl RecipeReviewOverride {
    pub fn neutral(asset_id: Uuid) -> Self {
        Self {
            asset_id,
            exposure_delta_ev: 0.0,
            highlights_delta: 0.0,
            shadows_delta: 0.0,
            whites_delta: 0.0,
            blacks_delta: 0.0,
            contrast_delta: 0.0,
            saturation_delta: 0.0,
        }
    }

    pub fn is_neutral(&self) -> bool {
        self.exposure_delta_ev.abs() <= NEUTRAL_EPSILON
            && self.highlights_delta.abs() <= NEUTRAL_EPSILON
            && self.shadows_delta.abs() <= NEUTRAL_EPSILON
            && self.whites_delta.abs() <= NEUTRAL_EPSILON
            && self.blacks_delta.abs() <= NEUTRAL_EPSILON
            && self.contrast_delta.abs() <= NEUTRAL_EPSILON
            && self.saturation_delta.abs() <= NEUTRAL_EPSILON
    }

    pub fn copy_selected_to(
        &self,
        target: &RecipeReviewOverride,
        fields: RecipeReviewSyncFields,
    ) -> RecipeReviewOverride {
        RecipeReviewOverride {
            asset_id: target.asset_id,
            exposure_delta_ev: if fields.exposure {
                self.exposure_delta_ev
            } else {
                target.exposure_delta_ev
            },
            highlights_delta: if fields.highlights {
                self.highlights_delta
            } else {
                target.highlights_delta
            },
            shadows_delta: if fields.shadows {
                self.shadows_delta
            } else {
                target.shadows_delta
            },
            whites_delta: if fields.whites {
                self.whites_delta
            } else {
                target.whites_delta
            },
            blacks_delta: if fields.blacks {
                self.blacks_delta
            } else {
                target.blacks_delta
            },
            contrast_delta: if fields.contrast {
                self.contrast_delta
            } else {
                target.contrast_delta
            },
            saturation_delta: if fields.saturation {
                self.saturation_delta
            } else {
                target.saturation_delta
            },
        }
    }

    pub fn apply_to_recipe(&self, recipe: &mut Recipe) -> Result<(), RecipeReviewStoreError> {
        let target = recipe
            .target_asset_id
            .ok_or(RecipeReviewStoreError::RecipeMissingTarget(recipe.id))?;
        if target != self.asset_id {
            return Err(RecipeReviewStoreError::TargetMismatch {
                recipe_id: recipe.id,
                recipe_asset_id: target,
                override_asset_id: self.asset_id,
            });
        }

        recipe.adjustments.exposure = add_delta(
            recipe.adjustments.exposure,
            self.exposure_delta_ev,
            -5.0,
            5.0,
        );
        recipe.adjustments.highlights = add_delta(
            recipe.adjustments.highlights,
            self.highlights_delta,
            -100.0,
            100.0,
        );
        recipe.adjustments.shadows = add_delta(
            recipe.adjustments.shadows,
            self.shadows_delta,
            -100.0,
            100.0,
        );
        recipe.adjustments.whites = add_delta(
            recipe.adjustments.whites,
            self.whites_delta,
            -100.0,
            100.0,
        );
        recipe.adjustments.blacks = add_delta(
            recipe.adjustments.blacks,
            self.blacks_delta,
            -100.0,
            100.0,
        );
        recipe.adjustments.contrast = add_delta(
            recipe.adjustments.contrast,
            self.contrast_delta,
            -100.0,
            100.0,
        );
        recipe.adjustments.saturation = add_delta(
            recipe.adjustments.saturation,
            self.saturation_delta,
            -100.0,
            100.0,
        );
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum RecipeReviewStoreError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid uuid in recipe review store: {0}")]
    Uuid(#[from] uuid::Error),
    #[error("recipe {0} is not bound to a target asset")]
    RecipeMissingTarget(Uuid),
    #[error("duplicate recipe review override for asset {0}")]
    DuplicateOverride(Uuid),
    #[error("duplicate recipe review confirmation for asset {0}")]
    DuplicateConfirmation(Uuid),
    #[error(
        "recipe {recipe_id} targets {recipe_asset_id}, but review override belongs to {override_asset_id}"
    )]
    TargetMismatch {
        recipe_id: Uuid,
        recipe_asset_id: Uuid,
        override_asset_id: Uuid,
    },
}

#[derive(Debug, Clone)]
pub struct RecipeReviewStore {
    path: PathBuf,
}

impl RecipeReviewStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, RecipeReviewStoreError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let store = Self { path };
        let conn = store.connect()?;
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")?;
        conn.execute_batch(SCHEMA)?;
        Ok(store)
    }

    fn connect(&self) -> Result<Connection, RecipeReviewStoreError> {
        Ok(Connection::open(&self.path)?)
    }

    pub fn set(
        &self,
        review: RecipeReviewOverride,
    ) -> Result<RecipeReviewOverride, RecipeReviewStoreError> {
        Ok(self
            .set_many(std::slice::from_ref(&review))?
            .pop()
            .expect("single review override must produce one result"))
    }

    pub fn set_many(
        &self,
        reviews: &[RecipeReviewOverride],
    ) -> Result<Vec<RecipeReviewOverride>, RecipeReviewStoreError> {
        if reviews.is_empty() {
            return Ok(Vec::new());
        }

        let mut seen = HashSet::with_capacity(reviews.len());
        let mut prepared = Vec::with_capacity(reviews.len());
        for review in reviews {
            if !seen.insert(review.asset_id) {
                return Err(RecipeReviewStoreError::DuplicateOverride(review.asset_id));
            }
            let mut normalized = review.clone();
            normalized.exposure_delta_ev = normalized.exposure_delta_ev.clamp(-3.0, 3.0);
            normalized.highlights_delta = normalized.highlights_delta.clamp(-100.0, 100.0);
            normalized.shadows_delta = normalized.shadows_delta.clamp(-100.0, 100.0);
            normalized.whites_delta = normalized.whites_delta.clamp(-100.0, 100.0);
            normalized.blacks_delta = normalized.blacks_delta.clamp(-100.0, 100.0);
            normalized.contrast_delta = normalized.contrast_delta.clamp(-100.0, 100.0);
            normalized.saturation_delta = normalized.saturation_delta.clamp(-100.0, 100.0);
            let json = if normalized.is_neutral() {
                None
            } else {
                Some(serde_json::to_string(&normalized)?)
            };
            prepared.push((normalized, json));
        }

        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        let updated_at = unix_time_ms();
        for (review, json) in &prepared {
            if let Some(json) = json {
                tx.execute(
                    "INSERT INTO recipe_review_overrides (asset_id, override_json, updated_at_unix_ms)
                     VALUES (?1, ?2, ?3)
                     ON CONFLICT(asset_id) DO UPDATE SET
                         override_json = excluded.override_json,
                         updated_at_unix_ms = excluded.updated_at_unix_ms",
                    params![review.asset_id.to_string(), json, updated_at],
                )?;
            } else {
                tx.execute(
                    "DELETE FROM recipe_review_overrides WHERE asset_id = ?1",
                    [review.asset_id.to_string()],
                )?;
            }
        }
        tx.commit()?;

        Ok(prepared.into_iter().map(|(review, _)| review).collect())
    }

    pub fn get(
        &self,
        asset_id: Uuid,
    ) -> Result<Option<RecipeReviewOverride>, RecipeReviewStoreError> {
        let conn = self.connect()?;
        let json = conn
            .query_row(
                "SELECT override_json FROM recipe_review_overrides WHERE asset_id = ?1",
                [asset_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        json.map(|value| Ok(serde_json::from_str(&value)?)).transpose()
    }

    pub fn list_for_assets(
        &self,
        asset_ids: &[Uuid],
    ) -> Result<Vec<RecipeReviewOverride>, RecipeReviewStoreError> {
        let mut values = Vec::new();
        for asset_id in asset_ids {
            if let Some(value) = self.get(*asset_id)? {
                values.push(value);
            }
        }
        Ok(values)
    }

    pub fn clear(&self, asset_id: Uuid) -> Result<(), RecipeReviewStoreError> {
        let conn = self.connect()?;
        conn.execute(
            "DELETE FROM recipe_review_overrides WHERE asset_id = ?1",
            [asset_id.to_string()],
        )?;
        Ok(())
    }

    pub fn confirm_recipe(
        &self,
        recipe: &Recipe,
    ) -> Result<RecipeReviewConfirmation, RecipeReviewStoreError> {
        Ok(self
            .confirm_recipes(std::slice::from_ref(recipe))?
            .pop()
            .expect("single recipe confirmation must produce one result"))
    }

    pub fn confirm_recipes(
        &self,
        recipes: &[Recipe],
    ) -> Result<Vec<RecipeReviewConfirmation>, RecipeReviewStoreError> {
        if recipes.is_empty() {
            return Ok(Vec::new());
        }

        let mut seen = HashSet::with_capacity(recipes.len());
        let mut confirmations = Vec::with_capacity(recipes.len());
        for recipe in recipes {
            let asset_id = recipe
                .target_asset_id
                .ok_or(RecipeReviewStoreError::RecipeMissingTarget(recipe.id))?;
            if !seen.insert(asset_id) {
                return Err(RecipeReviewStoreError::DuplicateConfirmation(asset_id));
            }
            confirmations.push(RecipeReviewConfirmation {
                asset_id,
                recipe_fingerprint: recipe_review_fingerprint(recipe)?,
            });
        }

        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        let updated_at = unix_time_ms();
        for confirmation in &confirmations {
            tx.execute(
                "INSERT INTO recipe_review_confirmations
                 (asset_id, recipe_fingerprint, updated_at_unix_ms)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(asset_id) DO UPDATE SET
                     recipe_fingerprint = excluded.recipe_fingerprint,
                     updated_at_unix_ms = excluded.updated_at_unix_ms",
                params![
                    confirmation.asset_id.to_string(),
                    confirmation.recipe_fingerprint,
                    updated_at
                ],
            )?;
        }
        tx.commit()?;
        Ok(confirmations)
    }
    pub fn is_recipe_confirmed(
        &self,
        recipe: &Recipe,
    ) -> Result<bool, RecipeReviewStoreError> {
        let asset_id = recipe
            .target_asset_id
            .ok_or(RecipeReviewStoreError::RecipeMissingTarget(recipe.id))?;
        let current = recipe_review_fingerprint(recipe)?;
        let conn = self.connect()?;
        let stored = conn
            .query_row(
                "SELECT recipe_fingerprint
                 FROM recipe_review_confirmations
                 WHERE asset_id = ?1",
                [asset_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        Ok(stored.as_deref() == Some(current.as_str()))
    }

    pub fn clear_confirmation(
        &self,
        asset_id: Uuid,
    ) -> Result<(), RecipeReviewStoreError> {
        let conn = self.connect()?;
        conn.execute(
            "DELETE FROM recipe_review_confirmations WHERE asset_id = ?1",
            [asset_id.to_string()],
        )?;
        Ok(())
    }
}

fn stable_fingerprint(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn add_delta(base: Option<f32>, delta: f32, min: f32, max: f32) -> Option<f32> {
    if delta.abs() <= NEUTRAL_EPSILON {
        return base;
    }
    Some((base.unwrap_or(0.0) + delta).clamp(min, max))
}

fn unix_time_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EditAdjustments, Recipe};
    use tempfile::tempdir;

    fn recipe(asset_id: Uuid) -> Recipe {
        Recipe {
            id: Uuid::new_v4(),
            name: "review".into(),
            target_asset_id: Some(asset_id),
            source_reference_ids: Vec::new(),
            adjustments: EditAdjustments {
                exposure: Some(0.4),
                contrast: Some(10.0),
                highlights: None,
                shadows: None,
                whites: None,
                blacks: None,
                temperature: None,
                tint: None,
                saturation: Some(3.0),
            },
        }
    }

    #[test]
    fn old_override_json_defaults_new_tone_fields_to_zero() {
        let asset_id = Uuid::new_v4();
        let json = format!(
            r#"{{"asset_id":"{asset_id}","exposure_delta_ev":0.2,"contrast_delta":4.0,"saturation_delta":-2.0}}"#
        );
        let value: RecipeReviewOverride = serde_json::from_str(&json).unwrap();
        assert_eq!(value.highlights_delta, 0.0);
        assert_eq!(value.shadows_delta, 0.0);
        assert_eq!(value.whites_delta, 0.0);
        assert_eq!(value.blacks_delta, 0.0);
    }

    #[test]
    fn review_override_round_trips_by_asset_id() {
        let dir = tempdir().unwrap();
        let store = RecipeReviewStore::open(dir.path().join("project.sqlite3")).unwrap();
        let asset_id = Uuid::new_v4();
        let value = RecipeReviewOverride {
            asset_id,
            exposure_delta_ev: 0.25,
            highlights_delta: 0.0,
            shadows_delta: 0.0,
            whites_delta: 0.0,
            blacks_delta: 0.0,
            contrast_delta: 5.0,
            saturation_delta: -2.0,
        };

        store.set(value.clone()).unwrap();
        assert_eq!(store.get(asset_id).unwrap(), Some(value));
    }

    #[test]
    fn review_override_is_additive_and_preserves_unknown_white_balance() {
        let asset_id = Uuid::new_v4();
        let mut target = recipe(asset_id);
        RecipeReviewOverride {
            asset_id,
            exposure_delta_ev: 0.2,
            highlights_delta: 0.0,
            shadows_delta: 0.0,
            whites_delta: 0.0,
            blacks_delta: 0.0,
            contrast_delta: -4.0,
            saturation_delta: 2.0,
        }
        .apply_to_recipe(&mut target)
        .unwrap();

        assert!((target.adjustments.exposure.unwrap() - 0.6).abs() < 1e-6);
        assert_eq!(target.adjustments.contrast, Some(6.0));
        assert_eq!(target.adjustments.saturation, Some(5.0));
        assert!(target.adjustments.temperature.is_none());
        assert!(target.adjustments.tint.is_none());
    }

    #[test]
    fn highlight_shadow_exception_is_additive() {
        let asset_id = Uuid::new_v4();
        let mut target = recipe(asset_id);
        target.adjustments.highlights = Some(-25.0);
        target.adjustments.shadows = Some(20.0);
        target.adjustments.whites = Some(-10.0);
        target.adjustments.blacks = Some(6.0);
        RecipeReviewOverride {
            asset_id,
            exposure_delta_ev: 0.0,
            highlights_delta: -10.0,
            shadows_delta: 15.0,
            whites_delta: -5.0,
            blacks_delta: 4.0,
            contrast_delta: 0.0,
            saturation_delta: 0.0,
        }
        .apply_to_recipe(&mut target)
        .unwrap();
        assert_eq!(target.adjustments.highlights, Some(-35.0));
        assert_eq!(target.adjustments.shadows, Some(35.0));
        assert_eq!(target.adjustments.whites, Some(-15.0));
        assert_eq!(target.adjustments.blacks, Some(10.0));
    }

    #[test]
    fn neutral_override_removes_persisted_exception() {
        let dir = tempdir().unwrap();
        let store = RecipeReviewStore::open(dir.path().join("project.sqlite3")).unwrap();
        let asset_id = Uuid::new_v4();

        store
            .set(RecipeReviewOverride {
                asset_id,
                exposure_delta_ev: 0.2,
                highlights_delta: 0.0,
                shadows_delta: 0.0,
                whites_delta: 0.0,
                blacks_delta: 0.0,
                contrast_delta: 0.0,
                saturation_delta: 0.0,
            })
            .unwrap();
        store
            .set(RecipeReviewOverride::neutral(asset_id))
            .unwrap();

        assert!(store.get(asset_id).unwrap().is_none());
    }

    #[test]
    fn selected_exception_fields_copy_without_replacing_target_baseline_delta() {
        let source = RecipeReviewOverride {
            asset_id: Uuid::new_v4(),
            exposure_delta_ev: 0.35,
            highlights_delta: 0.0,
            shadows_delta: 0.0,
            whites_delta: 8.0,
            blacks_delta: -6.0,
            contrast_delta: 10.0,
            saturation_delta: -4.0,
        };
        let target_id = Uuid::new_v4();
        let target = RecipeReviewOverride {
            asset_id: target_id,
            exposure_delta_ev: -0.1,
            highlights_delta: 0.0,
            shadows_delta: 0.0,
            whites_delta: 2.0,
            blacks_delta: 1.0,
            contrast_delta: 3.0,
            saturation_delta: 7.0,
        };

        let copied = source.copy_selected_to(
            &target,
            RecipeReviewSyncFields {
                exposure: true,
                highlights: true,
                shadows: false,
                whites: true,
                blacks: false,
                contrast: false,
                saturation: true,
            },
        );

        assert_eq!(copied.asset_id, target_id);
        assert_eq!(copied.exposure_delta_ev, 0.35);
        assert_eq!(copied.whites_delta, 8.0);
        assert_eq!(copied.blacks_delta, 1.0);
        assert_eq!(copied.contrast_delta, 3.0);
        assert_eq!(copied.saturation_delta, -4.0);
    }

    #[test]
    fn batch_overrides_commit_and_neutral_targets_clear_transactionally() {
        let dir = tempdir().unwrap();
        let store = RecipeReviewStore::open(dir.path().join("project.sqlite3")).unwrap();
        let first_id = Uuid::new_v4();
        let second_id = Uuid::new_v4();

        store
            .set(RecipeReviewOverride {
                asset_id: second_id,
                exposure_delta_ev: 0.2,
                highlights_delta: 0.0,
                shadows_delta: 0.0,
                whites_delta: 0.0,
                blacks_delta: 0.0,
                contrast_delta: 0.0,
                saturation_delta: 0.0,
            })
            .unwrap();

        let result = store
            .set_many(&[
                RecipeReviewOverride {
                    asset_id: first_id,
                    exposure_delta_ev: 0.3,
                    highlights_delta: 0.0,
                    shadows_delta: 0.0,
                    whites_delta: 0.0,
                    blacks_delta: 0.0,
                    contrast_delta: 5.0,
                    saturation_delta: 0.0,
                },
                RecipeReviewOverride::neutral(second_id),
            ])
            .unwrap();

        assert_eq!(result.len(), 2);
        assert!(store.get(first_id).unwrap().is_some());
        assert!(store.get(second_id).unwrap().is_none());
    }

    #[test]
    fn duplicate_batch_override_is_rejected_before_any_write() {
        let dir = tempdir().unwrap();
        let store = RecipeReviewStore::open(dir.path().join("project.sqlite3")).unwrap();
        let asset_id = Uuid::new_v4();
        let review = RecipeReviewOverride {
            asset_id,
            exposure_delta_ev: 0.25,
            highlights_delta: 0.0,
            shadows_delta: 0.0,
            whites_delta: 0.0,
            blacks_delta: 0.0,
            contrast_delta: 0.0,
            saturation_delta: 0.0,
        };

        assert!(matches!(
            store.set_many(&[review.clone(), review]).unwrap_err(),
            RecipeReviewStoreError::DuplicateOverride(id) if id == asset_id
        ));
        assert!(store.get(asset_id).unwrap().is_none());
    }

    #[test]
    fn confirmation_tracks_recipe_content_not_ephemeral_recipe_id() {
        let dir = tempdir().unwrap();
        let store = RecipeReviewStore::open(dir.path().join("project.sqlite3")).unwrap();
        let asset_id = Uuid::new_v4();
        let mut current = recipe(asset_id);
        current.source_reference_ids = vec![Uuid::new_v4()];

        let confirmation = store.confirm_recipe(&current).unwrap();
        assert_eq!(confirmation.asset_id, asset_id);
        assert!(store.is_recipe_confirmed(&current).unwrap());

        let mut regenerated = current.clone();
        regenerated.id = Uuid::new_v4();
        regenerated.name = "regenerated display name".into();
        assert!(store.is_recipe_confirmed(&regenerated).unwrap());

        regenerated.adjustments.exposure = Some(0.5);
        assert!(!store.is_recipe_confirmed(&regenerated).unwrap());

        regenerated = current.clone();
        regenerated.source_reference_ids = vec![Uuid::new_v4()];
        assert!(!store.is_recipe_confirmed(&regenerated).unwrap());

        store.clear_confirmation(asset_id).unwrap();
        assert!(!store.is_recipe_confirmed(&current).unwrap());
    }

    #[test]
    fn batch_confirmations_commit_together() {
        let dir = tempdir().unwrap();
        let store = RecipeReviewStore::open(dir.path().join("project.sqlite3")).unwrap();
        let first = recipe(Uuid::new_v4());
        let second = recipe(Uuid::new_v4());

        let confirmed = store.confirm_recipes(&[first.clone(), second.clone()]).unwrap();
        assert_eq!(confirmed.len(), 2);
        assert!(store.is_recipe_confirmed(&first).unwrap());
        assert!(store.is_recipe_confirmed(&second).unwrap());
    }

    #[test]
    fn batch_confirmation_validates_every_recipe_before_write() {
        let dir = tempdir().unwrap();
        let store = RecipeReviewStore::open(dir.path().join("project.sqlite3")).unwrap();
        let valid = recipe(Uuid::new_v4());
        let mut invalid = recipe(Uuid::new_v4());
        invalid.target_asset_id = None;

        assert!(matches!(
            store.confirm_recipes(&[valid.clone(), invalid]).unwrap_err(),
            RecipeReviewStoreError::RecipeMissingTarget(_)
        ));
        assert!(!store.is_recipe_confirmed(&valid).unwrap());
    }

    #[test]
    fn batch_confirmation_rejects_duplicate_assets_before_write() {
        let dir = tempdir().unwrap();
        let store = RecipeReviewStore::open(dir.path().join("project.sqlite3")).unwrap();
        let first = recipe(Uuid::new_v4());
        let mut duplicate = first.clone();
        duplicate.id = Uuid::new_v4();

        assert!(matches!(
            store.confirm_recipes(&[first.clone(), duplicate]).unwrap_err(),
            RecipeReviewStoreError::DuplicateConfirmation(asset_id) if asset_id == first.target_asset_id.unwrap()
        ));
        assert!(!store.is_recipe_confirmed(&first).unwrap());
    }
    #[test]
    fn override_cannot_move_to_another_recipe_target() {
        let mut target = recipe(Uuid::new_v4());
        let error = RecipeReviewOverride {
            asset_id: Uuid::new_v4(),
            exposure_delta_ev: 0.1,
            highlights_delta: 0.0,
            shadows_delta: 0.0,
            whites_delta: 0.0,
            blacks_delta: 0.0,
            contrast_delta: 0.0,
            saturation_delta: 0.0,
        }
        .apply_to_recipe(&mut target)
        .unwrap_err();

        assert!(matches!(
            error,
            RecipeReviewStoreError::TargetMismatch { .. }
        ));
    }
}
