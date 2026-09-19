use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS culling_reviews (
    asset_id TEXT PRIMARY KEY NOT NULL,
    decision TEXT NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS moment_quick_cull_operations (
    operation_id TEXT PRIMARY KEY NOT NULL,
    batch_id TEXT NOT NULL,
    group_ids_json TEXT NOT NULL,
    reviews_json TEXT NOT NULL,
    created_at_unix_ms INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_moment_quick_cull_batch_created
    ON moment_quick_cull_operations(batch_id, created_at_unix_ms DESC);
"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CullingUserDecision {
    Keep,
    Review,
    Reject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CullingReview {
    pub asset_id: Uuid,
    pub decision: CullingUserDecision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MomentQuickCullOperation {
    pub operation_id: Uuid,
    pub batch_id: Uuid,
    pub group_ids: Vec<Uuid>,
    pub reviews: Vec<CullingReview>,
    pub created_at_unix_ms: i64,
}

#[derive(Debug, Error)]
pub enum CullingReviewStoreError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid uuid in culling review store: {0}")]
    Uuid(#[from] uuid::Error),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid culling decision in store: {0}")]
    InvalidDecision(String),
    #[error("moment quick-cull operation not found: {0}")]
    OperationNotFound(Uuid),
    #[error("moment quick-cull operation is stale because photographer decisions changed: {0}")]
    OperationStale(Uuid),
}

#[derive(Debug, Clone)]
pub struct CullingReviewStore {
    path: PathBuf,
}

impl CullingReviewStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, CullingReviewStoreError> {
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

    fn connect(&self) -> Result<Connection, CullingReviewStoreError> {
        Ok(Connection::open(&self.path)?)
    }

    pub fn set(
        &self,
        asset_id: Uuid,
        decision: CullingUserDecision,
    ) -> Result<CullingReview, CullingReviewStoreError> {
        let conn = self.connect()?;
        conn.execute(
            "INSERT INTO culling_reviews (asset_id, decision, updated_at_unix_ms)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(asset_id) DO UPDATE SET
                 decision = excluded.decision,
                 updated_at_unix_ms = excluded.updated_at_unix_ms",
            params![
                asset_id.to_string(),
                decision_to_db(decision),
                unix_time_ms()
            ],
        )?;
        Ok(CullingReview { asset_id, decision })
    }

    pub fn set_many(
        &self,
        reviews: &[CullingReview],
    ) -> Result<(), CullingReviewStoreError> {
        if reviews.is_empty() {
            return Ok(());
        }

        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        let updated_at = unix_time_ms();
        for review in reviews {
            tx.execute(
                "INSERT INTO culling_reviews (asset_id, decision, updated_at_unix_ms)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(asset_id) DO UPDATE SET
                     decision = excluded.decision,
                     updated_at_unix_ms = excluded.updated_at_unix_ms",
                params![
                    review.asset_id.to_string(),
                    decision_to_db(review.decision),
                    updated_at
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn set_many_as_moment_quick_cull(
        &self,
        batch_id: Uuid,
        group_ids: &[Uuid],
        reviews: &[CullingReview],
    ) -> Result<MomentQuickCullOperation, CullingReviewStoreError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        let created_at_unix_ms = unix_time_ms();
        let operation = MomentQuickCullOperation {
            operation_id: Uuid::new_v4(),
            batch_id,
            group_ids: group_ids.to_vec(),
            reviews: reviews.to_vec(),
            created_at_unix_ms,
        };

        for review in reviews {
            tx.execute(
                "INSERT INTO culling_reviews (asset_id, decision, updated_at_unix_ms)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(asset_id) DO UPDATE SET
                     decision = excluded.decision,
                     updated_at_unix_ms = excluded.updated_at_unix_ms",
                params![
                    review.asset_id.to_string(),
                    decision_to_db(review.decision),
                    created_at_unix_ms
                ],
            )?;
        }
        tx.execute(
            "INSERT INTO moment_quick_cull_operations
             (operation_id, batch_id, group_ids_json, reviews_json, created_at_unix_ms)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                operation.operation_id.to_string(),
                batch_id.to_string(),
                serde_json::to_string(group_ids)?,
                serde_json::to_string(reviews)?,
                created_at_unix_ms,
            ],
        )?;
        tx.commit()?;
        Ok(operation)
    }

    pub fn latest_moment_quick_cull(
        &self,
        batch_id: Uuid,
    ) -> Result<Option<MomentQuickCullOperation>, CullingReviewStoreError> {
        let conn = self.connect()?;
        let row = conn
            .query_row(
                "SELECT operation_id, group_ids_json, reviews_json, created_at_unix_ms
                 FROM moment_quick_cull_operations
                 WHERE batch_id = ?1
                 ORDER BY created_at_unix_ms DESC, rowid DESC
                 LIMIT 1",
                [batch_id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .optional()?;

        row.map(|(operation_id, group_ids_json, reviews_json, created_at_unix_ms)| {
            Ok(MomentQuickCullOperation {
                operation_id: Uuid::parse_str(&operation_id)?,
                batch_id,
                group_ids: serde_json::from_str(&group_ids_json)?,
                reviews: serde_json::from_str(&reviews_json)?,
                created_at_unix_ms,
            })
        })
        .transpose()
    }

    pub fn undo_moment_quick_cull(
        &self,
        operation_id: Uuid,
    ) -> Result<MomentQuickCullOperation, CullingReviewStoreError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        let row = tx
            .query_row(
                "SELECT batch_id, group_ids_json, reviews_json, created_at_unix_ms
                 FROM moment_quick_cull_operations
                 WHERE operation_id = ?1",
                [operation_id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .optional()?
            .ok_or(CullingReviewStoreError::OperationNotFound(operation_id))?;

        let operation = MomentQuickCullOperation {
            operation_id,
            batch_id: Uuid::parse_str(&row.0)?,
            group_ids: serde_json::from_str(&row.1)?,
            reviews: serde_json::from_str(&row.2)?,
            created_at_unix_ms: row.3,
        };

        for review in &operation.reviews {
            let current = tx
                .query_row(
                    "SELECT decision, updated_at_unix_ms FROM culling_reviews WHERE asset_id = ?1",
                    [review.asset_id.to_string()],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
                )
                .optional()?;
            let Some((current_decision, current_updated_at)) = current else {
                return Err(CullingReviewStoreError::OperationStale(operation_id));
            };
            if decision_from_db(&current_decision)? != review.decision
                || current_updated_at != operation.created_at_unix_ms
            {
                return Err(CullingReviewStoreError::OperationStale(operation_id));
            }
        }

        for review in &operation.reviews {
            tx.execute(
                "DELETE FROM culling_reviews WHERE asset_id = ?1",
                [review.asset_id.to_string()],
            )?;
        }
        tx.execute(
            "DELETE FROM moment_quick_cull_operations WHERE operation_id = ?1",
            [operation_id.to_string()],
        )?;
        tx.commit()?;
        Ok(operation)
    }

    pub fn clear(&self, asset_id: Uuid) -> Result<(), CullingReviewStoreError> {
        let conn = self.connect()?;
        conn.execute(
            "DELETE FROM culling_reviews WHERE asset_id = ?1",
            [asset_id.to_string()],
        )?;
        Ok(())
    }

    pub fn get(
        &self,
        asset_id: Uuid,
    ) -> Result<Option<CullingReview>, CullingReviewStoreError> {
        let conn = self.connect()?;
        let decision = conn
            .query_row(
                "SELECT decision FROM culling_reviews WHERE asset_id = ?1",
                [asset_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        decision
            .map(|value| {
                Ok(CullingReview {
                    asset_id,
                    decision: decision_from_db(&value)?,
                })
            })
            .transpose()
    }

    pub fn list_for_assets(
        &self,
        asset_ids: &[Uuid],
    ) -> Result<Vec<CullingReview>, CullingReviewStoreError> {
        let mut reviews = Vec::new();
        for asset_id in asset_ids {
            if let Some(review) = self.get(*asset_id)? {
                reviews.push(review);
            }
        }
        Ok(reviews)
    }
}

fn decision_to_db(value: CullingUserDecision) -> &'static str {
    match value {
        CullingUserDecision::Keep => "KEEP",
        CullingUserDecision::Review => "REVIEW",
        CullingUserDecision::Reject => "REJECT",
    }
}

fn decision_from_db(value: &str) -> Result<CullingUserDecision, CullingReviewStoreError> {
    match value {
        "KEEP" => Ok(CullingUserDecision::Keep),
        "REVIEW" => Ok(CullingUserDecision::Review),
        "REJECT" => Ok(CullingUserDecision::Reject),
        other => Err(CullingReviewStoreError::InvalidDecision(other.to_string())),
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

    #[test]
    fn user_review_round_trips_and_can_be_cleared() {
        let dir = tempdir().unwrap();
        let store = CullingReviewStore::open(dir.path().join("project.sqlite3")).unwrap();
        let asset_id = Uuid::new_v4();

        store.set(asset_id, CullingUserDecision::Keep).unwrap();
        assert_eq!(
            store.get(asset_id).unwrap(),
            Some(CullingReview {
                asset_id,
                decision: CullingUserDecision::Keep,
            })
        );

        store.set(asset_id, CullingUserDecision::Reject).unwrap();
        assert_eq!(
            store.get(asset_id).unwrap().unwrap().decision,
            CullingUserDecision::Reject
        );

        store.clear(asset_id).unwrap();
        assert!(store.get(asset_id).unwrap().is_none());
    }

    #[test]
    fn batch_reviews_commit_together() {
        let dir = tempdir().unwrap();
        let store = CullingReviewStore::open(dir.path().join("project.sqlite3")).unwrap();
        let keep = Uuid::new_v4();
        let reject = Uuid::new_v4();

        store
            .set_many(&[
                CullingReview {
                    asset_id: keep,
                    decision: CullingUserDecision::Keep,
                },
                CullingReview {
                    asset_id: reject,
                    decision: CullingUserDecision::Reject,
                },
            ])
            .unwrap();

        assert_eq!(
            store.get(keep).unwrap().unwrap().decision,
            CullingUserDecision::Keep
        );
        assert_eq!(
            store.get(reject).unwrap().unwrap().decision,
            CullingUserDecision::Reject
        );
    }

    #[test]
    fn quick_cull_operation_round_trips_and_undoes_atomically() {
        let dir = tempdir().unwrap();
        let store = CullingReviewStore::open(dir.path().join("project.sqlite3")).unwrap();
        let batch_id = Uuid::new_v4();
        let group_id = Uuid::new_v4();
        let keep = Uuid::new_v4();
        let review = Uuid::new_v4();
        let reviews = vec![
            CullingReview {
                asset_id: keep,
                decision: CullingUserDecision::Keep,
            },
            CullingReview {
                asset_id: review,
                decision: CullingUserDecision::Review,
            },
        ];

        let operation = store
            .set_many_as_moment_quick_cull(batch_id, &[group_id], &reviews)
            .unwrap();
        assert_eq!(
            store.latest_moment_quick_cull(batch_id).unwrap(),
            Some(operation.clone())
        );
        assert_eq!(store.get(keep).unwrap().unwrap().decision, CullingUserDecision::Keep);

        let undone = store
            .undo_moment_quick_cull(operation.operation_id)
            .unwrap();
        assert_eq!(undone, operation);
        assert!(store.get(keep).unwrap().is_none());
        assert!(store.get(review).unwrap().is_none());
        assert!(store.latest_moment_quick_cull(batch_id).unwrap().is_none());
    }

    #[test]
    fn quick_cull_undo_refuses_to_erase_later_photographer_changes() {
        let dir = tempdir().unwrap();
        let store = CullingReviewStore::open(dir.path().join("project.sqlite3")).unwrap();
        let asset_id = Uuid::new_v4();
        let operation = store
            .set_many_as_moment_quick_cull(
                Uuid::new_v4(),
                &[Uuid::new_v4()],
                &[CullingReview {
                    asset_id,
                    decision: CullingUserDecision::Review,
                }],
            )
            .unwrap();

        store.set(asset_id, CullingUserDecision::Keep).unwrap();
        assert!(matches!(
            store.undo_moment_quick_cull(operation.operation_id),
            Err(CullingReviewStoreError::OperationStale(id)) if id == operation.operation_id
        ));
        assert_eq!(
            store.get(asset_id).unwrap().unwrap().decision,
            CullingUserDecision::Keep
        );
    }

    #[test]
    fn quick_cull_history_is_scoped_to_batch() {
        let dir = tempdir().unwrap();
        let store = CullingReviewStore::open(dir.path().join("project.sqlite3")).unwrap();
        let first_batch = Uuid::new_v4();
        let second_batch = Uuid::new_v4();
        let first = store
            .set_many_as_moment_quick_cull(
                first_batch,
                &[Uuid::new_v4()],
                &[CullingReview {
                    asset_id: Uuid::new_v4(),
                    decision: CullingUserDecision::Keep,
                }],
            )
            .unwrap();
        let second = store
            .set_many_as_moment_quick_cull(
                second_batch,
                &[Uuid::new_v4()],
                &[CullingReview {
                    asset_id: Uuid::new_v4(),
                    decision: CullingUserDecision::Review,
                }],
            )
            .unwrap();

        assert_eq!(
            store.latest_moment_quick_cull(first_batch).unwrap(),
            Some(first)
        );
        assert_eq!(
            store.latest_moment_quick_cull(second_batch).unwrap(),
            Some(second)
        );
    }

    #[test]
    fn review_state_is_scoped_by_stable_asset_id() {
        let dir = tempdir().unwrap();
        let store = CullingReviewStore::open(dir.path().join("project.sqlite3")).unwrap();
        let keep = Uuid::new_v4();
        let review = Uuid::new_v4();
        let untouched = Uuid::new_v4();

        store.set(keep, CullingUserDecision::Keep).unwrap();
        store.set(review, CullingUserDecision::Review).unwrap();

        let values = store.list_for_assets(&[keep, review, untouched]).unwrap();
        assert_eq!(values.len(), 2);
        assert!(values.iter().any(|value| value.asset_id == keep));
        assert!(values.iter().any(|value| value.asset_id == review));
    }
}
