use crate::{
    should_run_portrait_retouch, BatchItem, BatchStage, PhotoClassification, StageExecutor,
};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS photo_classifications (
    asset_id TEXT PRIMARY KEY NOT NULL,
    classification_json TEXT NOT NULL,
    classifier_id TEXT NOT NULL,
    classifier_version TEXT NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL
);
"#;

#[derive(Debug, Error)]
pub enum ClassificationStoreError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone)]
pub struct ClassificationStore {
    path: PathBuf,
}

impl ClassificationStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ClassificationStoreError> {
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

    fn connect(&self) -> Result<Connection, ClassificationStoreError> {
        Ok(Connection::open(&self.path)?)
    }

    pub fn save(&self, classification: &PhotoClassification) -> Result<(), ClassificationStoreError> {
        let conn = self.connect()?;
        conn.execute(
            "INSERT INTO photo_classifications
             (asset_id, classification_json, classifier_id, classifier_version, updated_at_unix_ms)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(asset_id) DO UPDATE SET
                 classification_json = excluded.classification_json,
                 classifier_id = excluded.classifier_id,
                 classifier_version = excluded.classifier_version,
                 updated_at_unix_ms = excluded.updated_at_unix_ms",
            params![
                classification.asset_id.to_string(),
                serde_json::to_string(classification)?,
                classification.classifier_id,
                classification.classifier_version,
                unix_time_ms()
            ],
        )?;
        Ok(())
    }

    pub fn get(
        &self,
        asset_id: Uuid,
    ) -> Result<Option<PhotoClassification>, ClassificationStoreError> {
        let conn = self.connect()?;
        let json = conn
            .query_row(
                "SELECT classification_json FROM photo_classifications WHERE asset_id = ?1",
                [asset_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        json.map(|json| Ok(serde_json::from_str(&json)?)).transpose()
    }
}

pub struct ClassificationRoutingExecutor<E> {
    inner: E,
    classifications: ClassificationStore,
}

impl<E> ClassificationRoutingExecutor<E> {
    pub fn new(inner: E, classifications: ClassificationStore) -> Self {
        Self {
            inner,
            classifications,
        }
    }

    pub fn into_inner(self) -> E {
        self.inner
    }
}

impl<E: StageExecutor> StageExecutor for ClassificationRoutingExecutor<E> {
    fn should_execute(&self, item: &BatchItem) -> Result<bool, String> {
        if item.stage != BatchStage::PortraitRetouch {
            return self.inner.should_execute(item);
        }

        let Some(asset_id) = item.asset_id else {
            return self.inner.should_execute(item);
        };
        let classification = self
            .classifications
            .get(asset_id)
            .map_err(|error| error.to_string())?;

        let Some(classification) = classification else {
            // Until Analyze has produced semantic evidence, never guess that an image is a portrait.
            return Ok(false);
        };

        Ok(should_run_portrait_retouch(&classification) && self.inner.should_execute(item)?)
    }

    fn execute(&mut self, item: &BatchItem) -> Result<(), String> {
        self.inner.execute(item)
    }
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
    use crate::{
        classify_photo, AutomationRunner, Batch, BatchStore, ClassificationSignals, NoopStageExecutor,
        PhotoCategory, PortraitClassificationPolicy,
    };
    use tempfile::tempdir;

    fn classification(asset_id: Uuid, portrait: bool) -> PhotoClassification {
        classify_photo(
            ClassificationSignals {
                asset_id,
                detected_person_count: if portrait { 1 } else { 0 },
                detected_face_count: if portrait { 1 } else { 0 },
                primary_subject_ratio: if portrait { 0.4 } else { 0.0 },
                people_confidence: if portrait { 0.99 } else { 0.0 },
                scene_tags: Vec::new(),
            },
            "test-classifier",
            "1",
            PortraitClassificationPolicy::default(),
        )
    }

    #[test]
    fn classification_round_trips() {
        let dir = tempdir().unwrap();
        let store = ClassificationStore::open(dir.path().join("project.sqlite3")).unwrap();
        let asset_id = Uuid::new_v4();
        let value = classification(asset_id, true);
        store.save(&value).unwrap();
        assert_eq!(store.get(asset_id).unwrap(), Some(value));
    }

    #[test]
    fn non_portrait_skips_portrait_stage_in_runner() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("project.sqlite3");
        let batch_store = BatchStore::open(&db).unwrap();
        let classification_store = ClassificationStore::open(&db).unwrap();
        let asset_id = Uuid::new_v4();
        let classification = classification(asset_id, false);
        assert_eq!(classification.category, PhotoCategory::NonPortrait);
        classification_store.save(&classification).unwrap();

        let batch = Batch::from_imported_assets(
            "scene",
            vec![(asset_id, "scene.cr3".to_string())],
        );
        batch_store.create_batch(&batch).unwrap();

        let executor = ClassificationRoutingExecutor::new(NoopStageExecutor, classification_store);
        let mut runner = AutomationRunner::new(batch_store, executor);
        let finished = runner.run_until_idle(batch.id).unwrap();
        assert_eq!(finished.items[0].stage, BatchStage::Done);
    }

    #[test]
    fn missing_classification_skips_portrait_stage_instead_of_failing_batch() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("project.sqlite3");
        let batch_store = BatchStore::open(&db).unwrap();
        let classification_store = ClassificationStore::open(&db).unwrap();
        let batch = Batch::from_imported_assets(
            "unclassified",
            vec![(Uuid::new_v4(), "unknown.cr3".to_string())],
        );
        batch_store.create_batch(&batch).unwrap();
        let executor = ClassificationRoutingExecutor::new(NoopStageExecutor, classification_store);
        let mut runner = AutomationRunner::new(batch_store, executor);
        let finished = runner.run_until_idle(batch.id).unwrap();
        assert_eq!(finished.items[0].status, crate::JobStatus::Done);
    }
}
