use std::sync::Arc;

use async_trait::async_trait;
use indexmap::IndexMap;
use serde_json::json;

use hermes_core::{JsonSchema, ToolError, ToolHandler, ToolSchema, tool_schema};

use crate::workflows::store::WorkflowRunStore;

pub struct MediaWorkflowStatusHandler {
    store: Arc<WorkflowRunStore>,
}

impl MediaWorkflowStatusHandler {
    pub fn new(store: Arc<WorkflowRunStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl ToolHandler for MediaWorkflowStatusHandler {
    async fn execute(&self, params: serde_json::Value) -> Result<String, ToolError> {
        let run_id = params
            .get("run_id")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ToolError::InvalidParams("missing 'run_id'".into()))?;

        let record = self.store.get(run_id).ok_or_else(|| {
            ToolError::ExecutionFailed(format!("workflow run not found: {run_id}"))
        })?;

        Ok(
            serde_json::to_string(&record)
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?,
        )
    }

    fn schema(&self) -> ToolSchema {
        let mut props = IndexMap::new();
        props.insert(
            "run_id".into(),
            json!({"type":"string","description":"Workflow run id from media_workflow_run"}),
        );
        tool_schema(
            "media_workflow_status",
            "Query status and artifacts for a media workflow run.",
            JsonSchema::object(props, vec!["run_id".into()]),
        )
    }
}
