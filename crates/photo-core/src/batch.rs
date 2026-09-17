use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BatchStage {
    Import,
    Analyze,
    ApplyPreset,
    Retouch,
    Qa,
    Export,
    Done,
}

impl BatchStage {
    pub fn next(self) -> Self {
        match self {
            Self::Import => Self::Analyze,
            Self::Analyze => Self::ApplyPreset,
            Self::ApplyPreset => Self::Retouch,
            Self::Retouch => Self::Qa,
            Self::Qa => Self::Export,
            Self::Export | Self::Done => Self::Done,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum JobStatus {
    Pending,
    Running,
    Paused,
    Failed,
    Cancelled,
    Done,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchItem {
    pub id: Uuid,
    pub source_path: String,
    pub stage: BatchStage,
    pub status: JobStatus,
    pub attempts: u32,
    pub last_error: Option<String>,
}

impl BatchItem {
    pub fn new(source_path: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            source_path: source_path.into(),
            stage: BatchStage::Import,
            status: JobStatus::Pending,
            attempts: 0,
            last_error: None,
        }
    }

    pub fn start(&mut self) {
        if !matches!(self.status, JobStatus::Cancelled | JobStatus::Done) {
            self.status = JobStatus::Running;
        }
    }

    pub fn complete_stage(&mut self) {
        self.stage = self.stage.next();
        self.last_error = None;
        self.status = if self.stage == BatchStage::Done {
            JobStatus::Done
        } else {
            JobStatus::Pending
        };
    }

    pub fn fail(&mut self, error: impl Into<String>) {
        self.attempts += 1;
        self.last_error = Some(error.into());
        self.status = JobStatus::Failed;
    }

    pub fn retry(&mut self) {
        if self.status == JobStatus::Failed {
            self.last_error = None;
            self.status = JobStatus::Pending;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Batch {
    pub id: Uuid,
    pub name: String,
    pub items: Vec<BatchItem>,
    pub auto_qa: bool,
    pub stop_on_error: bool,
}

impl Batch {
    pub fn new(name: impl Into<String>, paths: impl IntoIterator<Item = String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            items: paths.into_iter().map(BatchItem::new).collect(),
            auto_qa: true,
            stop_on_error: false,
        }
    }

    pub fn progress(&self) -> f32 {
        if self.items.is_empty() {
            return 0.0;
        }
        let done = self.items.iter().filter(|item| item.status == JobStatus::Done).count();
        done as f32 / self.items.len() as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_can_resume_from_failed_stage() {
        let mut item = BatchItem::new("sample.jpg");
        item.start();
        item.complete_stage();
        assert_eq!(item.stage, BatchStage::Analyze);

        item.start();
        item.fail("model unavailable");
        assert_eq!(item.status, JobStatus::Failed);
        assert_eq!(item.stage, BatchStage::Analyze);

        item.retry();
        assert_eq!(item.status, JobStatus::Pending);
        assert_eq!(item.stage, BatchStage::Analyze);
    }
}
