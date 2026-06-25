use std::sync::Arc;

use async_trait::async_trait;
use indexmap::IndexMap;
use serde_json::{Value, json};

use hermes_core::{JsonSchema, ToolError, ToolHandler, ToolSchema, tool_schema};

use crate::delivery::workflow_prompt_json;
use crate::workflows::WorkflowPlan;
use crate::workflows::runner::WorkflowRunner;
use crate::workflows::store::WorkflowRunStatus;
use crate::workflows::templates::{builtin_template, default_template_inputs};

pub struct MediaWorkflowRunHandler {
    runner: Arc<WorkflowRunner>,
}

impl MediaWorkflowRunHandler {
    pub fn new(runner: Arc<WorkflowRunner>) -> Self {
        Self { runner }
    }
}

#[async_trait]
impl ToolHandler for MediaWorkflowRunHandler {
    async fn execute(&self, params: Value) -> Result<String, ToolError> {
        let plan: WorkflowPlan = if let Some(plan_val) = params.get("plan") {
            serde_json::from_value(plan_val.clone())
                .map_err(|e| ToolError::InvalidParams(format!("invalid plan: {e}")))?
        } else {
            let workflow_id = params
                .get("workflow_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    ToolError::InvalidParams("provide 'plan' or 'workflow_id' + 'prompt'".into())
                })?;
            let prompt = params
                .get("prompt")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidParams("missing prompt".into()))?;
            let def = builtin_template(workflow_id).ok_or_else(|| {
                ToolError::InvalidParams(format!("unknown workflow_id: {workflow_id}"))
            })?;
            let inputs = default_template_inputs(workflow_id, prompt);
            WorkflowPlan::from_definition(&def, inputs)
        };

        let wait = params
            .get("wait")
            .and_then(|v| v.as_bool())
            .unwrap_or(!self.runner.async_execution_enabled());

        if wait {
            let record = self.runner.run_plan_sync(&plan).await?;
            return Ok(serialize_run_result(&record));
        }

        let workflow_id = plan.workflow_id.clone();
        let run_id = self.runner.spawn_plan(plan)?;
        hermes_core::report_tool_progress(format!(
            "媒体工作流已在后台运行（workflow={workflow_id}，run_id={run_id}），正在优化提示词并生成…"
        ));
        Ok(json!({
            "success": true,
            "run_id": run_id,
            "status": "running",
            "async": true,
            "hint": "Poll media_workflow_status with run_id until status is succeeded or failed"
        })
        .to_string())
    }

    fn schema(&self) -> ToolSchema {
        let mut props = IndexMap::new();
        props.insert(
            "plan".into(),
            json!({"type":"object","description":"Plan object from media_workflow_plan"}),
        );
        props.insert(
            "workflow_id".into(),
            json!({"type":"string","description":"Builtin template id when plan is omitted"}),
        );
        props.insert(
            "prompt".into(),
            json!({"type":"string","description":"Objective when plan is omitted"}),
        );
        props.insert(
            "wait".into(),
            json!({
                "type": "boolean",
                "description": "When true, block until complete. Default false when media.workflows.async_execution is true."
            }),
        );
        tool_schema(
            "media_workflow_run",
            "Execute a media workflow plan (refined prompts, image/video pipeline). Async by default — poll media_workflow_status. Completed runs include user_prompt_block with final API prompts to show the user.",
            JsonSchema::object(props, vec![]),
        )
    }
}

fn serialize_run_result(record: &crate::workflows::store::WorkflowRunRecord) -> String {
    let media_tags: Vec<String> = record
        .artifacts
        .iter()
        .filter_map(|a| a.get("local_path").and_then(|p| p.as_str()))
        .map(|p| format!("MEDIA:{p}"))
        .collect();

    let prompt_payload = workflow_prompt_json(record);
    let user_prompt_block = prompt_payload
        .get("user_prompt_block")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let mut hint_parts = Vec::new();
    if let Some(block) = &user_prompt_block {
        hint_parts.push(format!(
            "Include user_prompt_block in your reply so the user sees the final API prompts:\n{block}"
        ));
    }
    if !media_tags.is_empty() {
        hint_parts.push(format!(
            "Include {} for native media delivery",
            media_tags.join(" ")
        ));
    }

    let mut body = json!({
        "success": record.status == WorkflowRunStatus::Succeeded,
        "run_id": record.run_id,
        "workflow_id": record.workflow_id,
        "status": record.status,
        "error": record.error,
        "artifacts": record.artifacts,
        "step_outputs": record.step_outputs,
        "media_tags": media_tags,
        "manifest_path": format!("~/.hermes/media/workflows/{}/manifest.json", record.run_id),
        "hint": if hint_parts.is_empty() { Value::Null } else { json!(hint_parts.join("\n\n")) },
    });
    if let (Some(obj), Some(prompts)) = (body.as_object_mut(), prompt_payload.as_object()) {
        for (key, value) in prompts {
            obj.insert(key.clone(), value.clone());
        }
    }
    body.to_string()
}
