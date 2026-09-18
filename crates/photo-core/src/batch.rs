use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BatchStage {
    Import,
    Analyze,
    ApplyPreset,
    PortraitRetouch,
    Qa,
    Export,
    Done,
}

impl BatchStage {
    pub fn next(self) -> Self {
        match self {
            // New RAW batches use the runner only for source preparation and
            // reusable local analysis. Group/cull/reference/edit/output are
            // higher-level photographer workflows, not fake per-photo stages.
            Self::Import => Self::Analyze,
            Self::Analyze => Self::Done,

            // Keep legacy stages loadable for existing project databases and
            // explicit adapters such as direct export, but do not route new
            // imports into this deprecated automatic chain.
            Self::ApplyPreset => Self::PortraitRetouch,
            Self::PortraitRetouch => Self::Qa,
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
    pub asset_id: Option<Uuid>,
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
            asset_id: None,
            source_path: source_path.into(),
            stage: BatchStage::Import,
            status: JobStatus::Pending,
            attempts: 0,
            last_error: None,
        }
    }

    pub fn imported(source_path: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            asset_id: None,
            source_path: source_path.into(),
            stage: BatchStage::Analyze,
            status: JobStatus::Pending,
            attempts: 0,
            last_error: None,
        }
    }

    pub fn imported_asset(asset_id: Uuid, source_path: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            asset_id: Some(asset_id),
            source_path: source_path.into(),
            stage: BatchStage::Analyze,
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

    pub fn from_imported(name: impl Into<String>, paths: impl IntoIterator<Item = String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            items: paths.into_iter().map(BatchItem::imported).collect(),
            auto_qa: true,
            stop_on_error: false,
        }
    }

    pub fn from_imported_assets(
        name: impl Into<String>,
        assets: impl IntoIterator<Item = (Uuid, String)>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            items: assets
                .into_iter()
                .map(|(asset_id, path)| BatchItem::imported_asset(asset_id, path))
                .collect(),
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

    #[test]
    fn analyzed_import_becomes_ready_without_fake_edit_export_stages() {
        let mut item = BatchItem::imported_asset(Uuid::new_v4(), "sample.cr3");
        assert_eq!(item.stage, BatchStage::Analyze);
        item.start();
        item.complete_stage();
        assert_eq!(item.stage, BatchStage::Done);
        assert_eq!(item.status, JobStatus::Done);
    }

    #[test]
    fn imported_asset_keeps_catalog_identity() {
        let asset_id = Uuid::new_v4();
        let batch = Batch::from_imported_assets(
            "raw",
            vec![(asset_id, "sample.cr3".to_string())],
        );
        assert_eq!(batch.items[0].asset_id, Some(asset_id));
        assert_eq!(batch.items[0].stage, BatchStage::Analyze);
    }
}
