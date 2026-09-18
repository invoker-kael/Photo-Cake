use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InferenceTask {
    PortraitClassification,
    SceneClassification,
    PersonDetection,
    FaceDetection,
    ImageEmbedding,
    FaceEmbedding,
    Segmentation,
    QualityScoring,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InferenceBackend {
    Cpu,
    DirectMl,
    Cuda,
    TensorRt,
    Nnapi,
    Qnn,
    Vulkan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalModelDescriptor {
    pub id: String,
    pub version: String,
    pub task: InferenceTask,
    pub artifact_path: String,
    pub preferred_backends: Vec<InferenceBackend>,
    pub input_revision: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OnlineInferencePolicy {
    Disabled,
    ExplicitOnly,
}

impl Default for OnlineInferencePolicy {
    fn default() -> Self {
        Self::Disabled
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisCacheKey {
    pub asset_id: Uuid,
    pub source_fingerprint: String,
    pub preview_revision: String,
    pub task: InferenceTask,
    pub model_id: String,
    pub model_version: String,
    pub config_hash: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalysisArtifact {
    pub key: AnalysisCacheKey,
    pub payload_json: serde_json::Value,
}

#[derive(Debug, Error)]
pub enum AnalysisCacheError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone)]
pub struct AnalysisCache {
    path: PathBuf,
}

impl AnalysisCache {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, AnalysisCacheError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let cache = Self { path };
        let conn = cache.connect()?;
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS analysis_cache (
                asset_id TEXT NOT NULL,
                source_fingerprint TEXT NOT NULL,
                preview_revision TEXT NOT NULL,
                task TEXT NOT NULL,
                model_id TEXT NOT NULL,
                model_version TEXT NOT NULL,
                config_hash TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                updated_at_unix_ms INTEGER NOT NULL,
                PRIMARY KEY (
                    asset_id,
                    source_fingerprint,
                    preview_revision,
                    task,
                    model_id,
                    model_version,
                    config_hash
                )
            );
            CREATE INDEX IF NOT EXISTS idx_analysis_cache_asset_task
                ON analysis_cache(asset_id, task);
            "#,
        )?;
        Ok(cache)
    }

    fn connect(&self) -> Result<Connection, AnalysisCacheError> {
        Ok(Connection::open(&self.path)?)
    }

    pub fn get(&self, key: &AnalysisCacheKey) -> Result<Option<AnalysisArtifact>, AnalysisCacheError> {
        let conn = self.connect()?;
        let payload = conn
            .query_row(
                "SELECT payload_json FROM analysis_cache
                 WHERE asset_id = ?1 AND source_fingerprint = ?2 AND preview_revision = ?3
                   AND task = ?4 AND model_id = ?5 AND model_version = ?6 AND config_hash = ?7",
                params![
                    key.asset_id.to_string(),
                    key.source_fingerprint,
                    key.preview_revision,
                    task_to_db(key.task),
                    key.model_id,
                    key.model_version,
                    key.config_hash
                ],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        payload
            .map(|payload_json| {
                Ok(AnalysisArtifact {
                    key: key.clone(),
                    payload_json: serde_json::from_str(&payload_json)?,
                })
            })
            .transpose()
    }

    pub fn put(&self, artifact: &AnalysisArtifact) -> Result<(), AnalysisCacheError> {
        let conn = self.connect()?;
        conn.execute(
            "INSERT OR REPLACE INTO analysis_cache
             (asset_id, source_fingerprint, preview_revision, task, model_id, model_version, config_hash, payload_json, updated_at_unix_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                artifact.key.asset_id.to_string(),
                artifact.key.source_fingerprint,
                artifact.key.preview_revision,
                task_to_db(artifact.key.task),
                artifact.key.model_id,
                artifact.key.model_version,
                artifact.key.config_hash,
                serde_json::to_string(&artifact.payload_json)?,
                unix_time_ms()
            ],
        )?;
        Ok(())
    }

    /// Read the newest cached result for one asset/task without requiring the
    /// caller to know the exact model/config cache key.
    ///
    /// This is intended for downstream photographer workflows (grouping,
    /// culling, review) that consume whichever current evidence Analyze has
    /// already produced. Cache writes remain fully versioned.
    pub fn latest_for_asset_task(
        &self,
        asset_id: Uuid,
        task: InferenceTask,
    ) -> Result<Option<AnalysisArtifact>, AnalysisCacheError> {
        let conn = self.connect()?;
        let row = conn
            .query_row(
                "SELECT source_fingerprint, preview_revision, model_id, model_version, config_hash, payload_json
                 FROM analysis_cache
                 WHERE asset_id = ?1 AND task = ?2
                 ORDER BY updated_at_unix_ms DESC, rowid DESC
                 LIMIT 1",
                params![asset_id.to_string(), task_to_db(task)],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                    ))
                },
            )
            .optional()?;

        row.map(
            |(
                source_fingerprint,
                preview_revision,
                model_id,
                model_version,
                config_hash,
                payload_json,
            )| {
                Ok(AnalysisArtifact {
                    key: AnalysisCacheKey {
                        asset_id,
                        source_fingerprint,
                        preview_revision,
                        task,
                        model_id,
                        model_version,
                        config_hash,
                    },
                    payload_json: serde_json::from_str(&payload_json)?,
                })
            },
        )
        .transpose()
    }

    pub fn invalidate_asset_task(
        &self,
        asset_id: Uuid,
        task: InferenceTask,
    ) -> Result<usize, AnalysisCacheError> {
        let conn = self.connect()?;
        Ok(conn.execute(
            "DELETE FROM analysis_cache WHERE asset_id = ?1 AND task = ?2",
            params![asset_id.to_string(), task_to_db(task)],
        )?)
    }
}

fn task_to_db(task: InferenceTask) -> &'static str {
    match task {
        InferenceTask::PortraitClassification => "PORTRAIT_CLASSIFICATION",
        InferenceTask::SceneClassification => "SCENE_CLASSIFICATION",
        InferenceTask::PersonDetection => "PERSON_DETECTION",
        InferenceTask::FaceDetection => "FACE_DETECTION",
        InferenceTask::ImageEmbedding => "IMAGE_EMBEDDING",
        InferenceTask::FaceEmbedding => "FACE_EMBEDDING",
        InferenceTask::Segmentation => "SEGMENTATION",
        InferenceTask::QualityScoring => "QUALITY_SCORING",
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
    use tempfile::tempdir;

    fn key(model_version: &str) -> AnalysisCacheKey {
        AnalysisCacheKey {
            asset_id: Uuid::new_v4(),
            source_fingerprint: "raw-fingerprint-v1".to_string(),
            preview_revision: "preview-v1".to_string(),
            task: InferenceTask::ImageEmbedding,
            model_id: "photo-embed".to_string(),
            model_version: model_version.to_string(),
            config_hash: "default".to_string(),
        }
    }

    #[test]
    fn exact_analysis_key_reuses_cached_artifact() {
        let dir = tempdir().unwrap();
        let cache = AnalysisCache::open(dir.path().join("photo-cake.sqlite3")).unwrap();
        let key = key("1.0.0");
        let artifact = AnalysisArtifact {
            key: key.clone(),
            payload_json: serde_json::json!({"embedding": [0.1, 0.2, 0.3]}),
        };
        cache.put(&artifact).unwrap();

        assert_eq!(cache.get(&key).unwrap(), Some(artifact));
    }

    #[test]
    fn model_version_change_does_not_reuse_old_result() {
        let dir = tempdir().unwrap();
        let cache = AnalysisCache::open(dir.path().join("photo-cake.sqlite3")).unwrap();
        let old_key = key("1.0.0");
        cache
            .put(&AnalysisArtifact {
                key: old_key.clone(),
                payload_json: serde_json::json!({"value": 1}),
            })
            .unwrap();

        let mut new_key = old_key;
        new_key.model_version = "2.0.0".to_string();
        assert!(cache.get(&new_key).unwrap().is_none());
    }

    #[test]
    fn latest_task_result_hides_cache_key_details_from_consumers() {
        let dir = tempdir().unwrap();
        let cache = AnalysisCache::open(dir.path().join("photo-cake.sqlite3")).unwrap();
        let mut first = key("1.0.0");
        first.task = InferenceTask::QualityScoring;
        let asset_id = first.asset_id;
        cache
            .put(&AnalysisArtifact {
                key: first,
                payload_json: serde_json::json!({"sharpness": 0.4}),
            })
            .unwrap();

        let mut second = AnalysisCacheKey {
            asset_id,
            source_fingerprint: "raw-fingerprint-v2".to_string(),
            preview_revision: "preview-v2".to_string(),
            task: InferenceTask::QualityScoring,
            model_id: "quality".to_string(),
            model_version: "2".to_string(),
            config_hash: "new".to_string(),
        };
        cache
            .put(&AnalysisArtifact {
                key: second.clone(),
                payload_json: serde_json::json!({"sharpness": 0.9}),
            })
            .unwrap();

        let latest = cache
            .latest_for_asset_task(asset_id, InferenceTask::QualityScoring)
            .unwrap()
            .unwrap();
        assert_eq!(latest.key.model_version, "2");
        assert_eq!(latest.payload_json["sharpness"], 0.9);

        second.asset_id = Uuid::new_v4();
        assert!(cache
            .latest_for_asset_task(second.asset_id, InferenceTask::QualityScoring)
            .unwrap()
            .is_none());
    }

    #[test]
    fn invalidation_is_scoped_to_one_asset_task() {
        let dir = tempdir().unwrap();
        let cache = AnalysisCache::open(dir.path().join("photo-cake.sqlite3")).unwrap();
        let key = key("1.0.0");
        cache
            .put(&AnalysisArtifact {
                key: key.clone(),
                payload_json: serde_json::json!({"value": 1}),
            })
            .unwrap();
        assert_eq!(
            cache
                .invalidate_asset_task(key.asset_id, InferenceTask::ImageEmbedding)
                .unwrap(),
            1
        );
        assert!(cache.get(&key).unwrap().is_none());
    }
}
