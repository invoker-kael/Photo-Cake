use crate::{
    BatchItem, BatchStage, ExportStageExecutor, ExportRenderer, NoopStageExecutor, StageExecutor,
};

/// Dispatches batch stages while keeping the runner generic.
///
/// Export is routed to the real export executor; other stages continue using
/// the existing executor until their dedicated implementations are connected.
pub struct BatchPipelineExecutor<R> {
    pub export: ExportStageExecutor<R>,
}

impl<R> BatchPipelineExecutor<R> {
    pub fn new(export: ExportStageExecutor<R>) -> Self {
        Self { export }
    }
}

impl<R> StageExecutor for BatchPipelineExecutor<R>
where
    R: ExportRenderer,
{
    fn execute(&mut self, item: &BatchItem) -> Result<(), String> {
        match item.stage {
            BatchStage::Export => self
                .export
                .execute_item(item)
                .map_err(|error| error.to_string()),
            _ => {
                let mut noop = NoopStageExecutor;
                noop.execute(item)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_export_stages_remain_available_until_connected() {
        let mut noop = NoopStageExecutor;
        let item = BatchItem::new("sample.raw");
        assert!(noop.execute(&item).is_ok());
    }
}
