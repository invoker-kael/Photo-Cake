use crate::Recipe;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS recipe_review_overrides (
    asset_id TEXT PRIMARY KEY NOT NULL,
    override_json TEXT NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL
);
"#;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecipeReviewOverride {
    pub asset_id: Uuid,
    pub exposure_delta_ev: f32,
    pub contrast_delta: f32,
    pub saturation_delta: f32,
}

impl RecipeReviewOverride {
    pub fn neutral(asset_id: Uuid) -> Self {
        Self {
            asset_id,
            exposure_delta_ev: 0.0,
            contrast_delta: 0.0,
            saturation_delta: 0.0,
        }
    }

    pub fn is_neutral(&self) -> bool {
        self.exposure_delta_ev.abs() <= f32::EPSILON
            && self.contrast_delta.abs() <= f32::EPSILON
            && self.saturation_delta.abs() <= f32::EPSILON
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
        mut review: RecipeReviewOverride,
    ) -> Result<RecipeReviewOverride, RecipeReviewStoreError> {
        review.exposure_delta_ev = review.exposure_delta_ev.clamp(-3.0, 3.0);
        review.contrast_delta = review.contrast_delta.clamp(-100.0, 100.0);
        review.saturation_delta = review.saturation_delta.clamp(-100.0, 100.0);

        if review.is_neutral() {
            self.clear(review.asset_id)?;
            return Ok(review);
        }

        let conn = self.connect()?;
        conn.execute(
            "INSERT INTO recipe_review_overrides (asset_id, override_json, updated_at_unix_ms)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(asset_id) DO UPDATE SET
                 override_json = excluded.override_json,
                 updated_at_unix_ms = excluded.updated_at_unix_ms",
            params![
                review.asset_id.to_string(),
                serde_json::to_string(&review)?,
                unix_time_ms()
            ],
        )?;
        Ok(review)
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
}

fn add_delta(base: Option<f32>, delta: f32, min: f32, max: f32) -> Option<f32> {
    if delta.abs() <= f32::EPSILON {
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
                temperature: None,
                tint: None,
                saturation: Some(3.0),
            },
        }
    }

    #[test]
    fn review_override_round_trips_by_asset_id() {
        let dir = tempdir().unwrap();
        let store = RecipeReviewStore::open(dir.path().join("project.sqlite3")).unwrap();
        let asset_id = Uuid::new_v4();
        let value = RecipeReviewOverride {
            asset_id,
            exposure_delta_ev: 0.25,
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
            contrast_delta: -4.0,
            saturation_delta: 2.0,
        }
        .apply_to_recipe(&mut target)
        .unwrap();

        assert_eq!(target.adjustments.exposure, Some(0.6));
        assert_eq!(target.adjustments.contrast, Some(6.0));
        assert_eq!(target.adjustments.saturation, Some(5.0));
        assert!(target.adjustments.temperature.is_none());
        assert!(target.adjustments.tint.is_none());
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
    fn override_cannot_move_to_another_recipe_target() {
        let mut target = recipe(Uuid::new_v4());
        let error = RecipeReviewOverride {
            asset_id: Uuid::new_v4(),
            exposure_delta_ev: 0.1,
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
