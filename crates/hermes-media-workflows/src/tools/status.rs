use std::sync::Arc;

use async_trait::async_trait;
use indexmap::IndexMap;
use serde_json::json;

use hermes_core::{JsonSchema, ToolError, ToolHandler, ToolSchema, tool_schema};

use crate::delivery::workflow_prompt_json;
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

        let mut out = json!({
            "run": record,
            "manifest_hint": format!("~/.hermes/media/workflows/{}/manifest.json", record.run_id),
        });
        let prompt_payload = workflow_prompt_json(&record);
        if let (Some(obj), Some(prompts)) = (out.as_object_mut(), prompt_payload.as_object()) {
            for (key, value) in prompts {
                obj.insert(key.clone(), value.clone());
            }
        }
        Ok(out.to_string())
    }

    fn schema(&self) -> ToolSchema {
        let mut props = IndexMap::new();
        props.insert(
            "run_id".into(),
            json!({"type":"string","description":"Workflow run id from media_workflow_run"}),
        );
        tool_schema(
            "media_workflow_status",
            "Query status and artifacts for a media workflow run. Succeeded runs include user_prompt_block with final API prompts.",
            JsonSchema::object(props, vec!["run_id".into()]),
        )
    }
}
