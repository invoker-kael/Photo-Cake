use crate::{Batch, BatchItem, BatchStage, BatchStore, JobStatus, StoreError};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub trait StageExecutor {
    fn should_execute(&self, _item: &BatchItem) -> Result<bool, String> {
        Ok(true)
    }

    fn execute(&mut self, item: &BatchItem) -> Result<(), String>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct NoopStageExecutor;

impl StageExecutor for NoopStageExecutor {
    fn execute(&mut self, _item: &BatchItem) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RunStep {
    Idle,
    Completed {
        item_id: Uuid,
        stage: BatchStage,
    },
    Skipped {
        item_id: Uuid,
        stage: BatchStage,
    },
    Failed {
        item_id: Uuid,
        stage: BatchStage,
        message: String,
    },
}

#[derive(Debug, Error)]
pub enum RunnerError {
    #[error(transparent)]
    Store(#[from] StoreError),
}

pub struct AutomationRunner<E> {
    store: BatchStore,
    executor: E,
}

impl<E: StageExecutor> AutomationRunner<E> {
    pub fn new(store: BatchStore, executor: E) -> Self {
        Self { store, executor }
    }

    pub fn store(&self) -> &BatchStore {
        &self.store
    }

    pub fn recover_interrupted(&self) -> Result<usize, RunnerError> {
        Ok(self.store.recover_interrupted_jobs()?)
    }

    pub fn create_batch(
        &self,
        name: impl Into<String>,
        paths: impl IntoIterator<Item = String>,
    ) -> Result<Batch, RunnerError> {
        let batch = Batch::new(name, paths);
        self.store.create_batch(&batch)?;
        Ok(batch)
    }

    pub fn load_batch(&self, batch_id: Uuid) -> Result<Batch, RunnerError> {
        Ok(self.store.load_batch(batch_id)?)
    }

    pub fn list_batches(&self) -> Result<Vec<Batch>, RunnerError> {
        Ok(self.store.list_batches()?)
    }

    pub fn run_next(&mut self, batch_id: Uuid) -> Result<RunStep, RunnerError> {
        let mut batch = self.store.load_batch(batch_id)?;
        let Some(index) = batch
            .items
            .iter()
            .position(|item| item.status == JobStatus::Pending)
        else {
            return Ok(RunStep::Idle);
        };

        let auto_qa = batch.auto_qa;
        let item = &mut batch.items[index];
        let stage = item.stage;
        let item_id = item.id;

        item.start();
        self.store.update_item(batch_id, item)?;

        if stage == BatchStage::Qa && !auto_qa {
            item.complete_stage();
            self.store.update_item(batch_id, item)?;
            return Ok(RunStep::Skipped { item_id, stage });
        }

        match self.executor.should_execute(item) {
            Ok(false) => {
                item.complete_stage();
                self.store.update_item(batch_id, item)?;
                return Ok(RunStep::Skipped { item_id, stage });
            }
            Ok(true) => {}
            Err(message) => {
                item.fail(message.clone());
                self.store.update_item(batch_id, item)?;
                return Ok(RunStep::Failed {
                    item_id,
                    stage,
                    message,
                });
            }
        }

        match self.executor.execute(item) {
            Ok(()) => {
                item.complete_stage();
                self.store.update_item(batch_id, item)?;
                Ok(RunStep::Completed { item_id, stage })
            }
            Err(message) => {
                item.fail(message.clone());
                self.store.update_item(batch_id, item)?;
                Ok(RunStep::Failed {
                    item_id,
                    stage,
                    message,
                })
            }
        }
    }

    pub fn run_until_idle(&mut self, batch_id: Uuid) -> Result<Batch, RunnerError> {
        loop {
            let batch = self.store.load_batch(batch_id)?;
            if batch.stop_on_error
                && batch
                    .items
                    .iter()
                    .any(|item| item.status == JobStatus::Failed)
            {
                return Ok(batch);
            }

            if matches!(self.run_next(batch_id)?, RunStep::Idle) {
                return self.load_batch(batch_id);
            }
        }
    }

    pub fn retry_failed(&self, batch_id: Uuid) -> Result<Batch, RunnerError> {
        self.update_matching(batch_id, |item| {
            if item.status == JobStatus::Failed {
                item.retry();
                true
            } else {
                false
            }
        })
    }

    pub fn pause_batch(&self, batch_id: Uuid) -> Result<Batch, RunnerError> {
        self.update_matching(batch_id, |item| {
            if matches!(item.status, JobStatus::Pending | JobStatus::Running) {
                item.status = JobStatus::Paused;
                true
            } else {
                false
            }
        })
    }

    pub fn resume_batch(&self, batch_id: Uuid) -> Result<Batch, RunnerError> {
        self.update_matching(batch_id, |item| {
            if item.status == JobStatus::Paused {
                item.status = JobStatus::Pending;
                true
            } else {
                false
            }
        })
    }

    pub fn cancel_batch(&self, batch_id: Uuid) -> Result<Batch, RunnerError> {
        self.update_matching(batch_id, |item| {
            if !matches!(item.status, JobStatus::Done | JobStatus::Cancelled) {
                item.status = JobStatus::Cancelled;
                true
            } else {
                false
            }
        })
    }

    fn update_matching<F>(&self, batch_id: Uuid, mut update: F) -> Result<Batch, RunnerError>
    where
        F: FnMut(&mut BatchItem) -> bool,
    {
        let mut batch = self.store.load_batch(batch_id)?;
        for item in &mut batch.items {
            if update(item) {
                self.store.update_item(batch_id, item)?;
            }
        }
        Ok(batch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tempfile::tempdir;

    struct FailAnalyzeOnce {
        failed: bool,
        log: Arc<Mutex<Vec<BatchStage>>>,
    }

    impl StageExecutor for FailAnalyzeOnce {
        fn execute(&mut self, item: &BatchItem) -> Result<(), String> {
            self.log.lock().unwrap().push(item.stage);
            if item.stage == BatchStage::Analyze && !self.failed {
                self.failed = true;
                return Err("temporary model failure".to_string());
            }
            Ok(())
        }
    }

    #[test]
    fn retry_continues_from_failed_stage_instead_of_restarting() {
        let dir = tempdir().unwrap();
        let store = BatchStore::open(dir.path().join("runner.sqlite3")).unwrap();
        let log = Arc::new(Mutex::new(Vec::new()));
        let executor = FailAnalyzeOnce {
            failed: false,
            log: log.clone(),
        };
        let mut runner = AutomationRunner::new(store, executor);
        let batch = runner
            .create_batch("test", vec!["sample.jpg".to_string()])
            .unwrap();

        let after_failure = runner.run_until_idle(batch.id).unwrap();
        assert_eq!(after_failure.items[0].stage, BatchStage::Analyze);
        assert_eq!(after_failure.items[0].status, JobStatus::Failed);
        assert_eq!(after_failure.items[0].attempts, 1);

        runner.retry_failed(batch.id).unwrap();
        let finished = runner.run_until_idle(batch.id).unwrap();
        assert_eq!(finished.items[0].stage, BatchStage::Done);
        assert_eq!(finished.items[0].status, JobStatus::Done);
        assert_eq!(finished.items[0].attempts, 1);

        let stages = log.lock().unwrap();
        assert_eq!(
            stages.iter().filter(|stage| **stage == BatchStage::Import).count(),
            1
        );
        assert_eq!(
            stages.iter().filter(|stage| **stage == BatchStage::Analyze).count(),
            2
        );
    }

    #[test]
    fn pause_and_resume_preserve_stage() {
        let dir = tempdir().unwrap();
        let store = BatchStore::open(dir.path().join("pause.sqlite3")).unwrap();
        let runner = AutomationRunner::new(store, NoopStageExecutor);
        let batch = runner
            .create_batch("test", vec!["sample.jpg".to_string()])
            .unwrap();

        let paused = runner.pause_batch(batch.id).unwrap();
        assert_eq!(paused.items[0].stage, BatchStage::Import);
        assert_eq!(paused.items[0].status, JobStatus::Paused);

        let resumed = runner.resume_batch(batch.id).unwrap();
        assert_eq!(resumed.items[0].stage, BatchStage::Import);
        assert_eq!(resumed.items[0].status, JobStatus::Pending);
    }

    struct SkipPortrait;

    impl StageExecutor for SkipPortrait {
        fn should_execute(&self, item: &BatchItem) -> Result<bool, String> {
            Ok(item.stage != BatchStage::PortraitRetouch)
        }

        fn execute(&mut self, _item: &BatchItem) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn skipped_stage_advances_without_failure() {
        let dir = tempdir().unwrap();
        let store = BatchStore::open(dir.path().join("skip.sqlite3")).unwrap();
        let mut runner = AutomationRunner::new(store, SkipPortrait);
        let batch = runner
            .create_batch("test", vec!["sample.raw".to_string()])
            .unwrap();
        let finished = runner.run_until_idle(batch.id).unwrap();
        assert_eq!(finished.items[0].stage, BatchStage::Done);
        assert_eq!(finished.items[0].status, JobStatus::Done);
    }
}
