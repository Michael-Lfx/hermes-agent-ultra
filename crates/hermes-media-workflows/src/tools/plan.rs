use async_trait::async_trait;
use serde_json::{Value, json};

use hermes_config::MediaGenConfig;
use hermes_core::{ToolError, ToolHandler};

use crate::workflows::WorkflowPlan;
use crate::workflows::templates::{
    builtin_template, default_template_inputs, list_builtin_templates, suggest_template_id,
};

pub struct MediaWorkflowPlanHandler {
    media_config: MediaGenConfig,
}

impl MediaWorkflowPlanHandler {
    pub fn new(media_config: MediaGenConfig) -> Self {
        Self { media_config }
    }
}

#[async_trait]
impl ToolHandler for MediaWorkflowPlanHandler {
    async fn execute(&self, params: Value) -> Result<String, ToolError> {
        let objective = params
            .get("objective")
            .or_else(|| params.get("prompt"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ToolError::InvalidParams("missing 'objective' or 'prompt'".into()))?;

        let has_image = params
            .get("image_url")
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.trim().is_empty());

        let template_id = params
            .get("workflow_id")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| {
                suggest_template_id(
                    objective,
                    has_image,
                    &self.media_config.workflows.default_templates,
                )
            });

        let def = builtin_template(&template_id).ok_or_else(|| {
            ToolError::InvalidParams(format!(
                "unknown workflow_id '{template_id}' — available: {}",
                list_builtin_templates().join(", ")
            ))
        })?;

        let mut inputs = default_template_inputs(&template_id, objective);
        if let Some(model) = params.get("model") {
            inputs["model"] = model.clone();
        }
        if let Some(url) = params.get("image_url") {
            inputs["image_url"] = url.clone();
        }
        if let Some(duration) = params.get("duration") {
            inputs["duration"] = duration.clone();
        }
        if let Some(ratio) = params.get("aspect_ratio") {
            inputs["aspect_ratio"] = ratio.clone();
        }
        if let Some(resolution) = params.get("resolution") {
            inputs["resolution"] = resolution.clone();
        }
        if let Some(extra) = params.get("inputs").and_then(Value::as_object) {
            for (k, v) in extra {
                inputs[k] = v.clone();
            }
        }

        if template_id == "img2video_direct"
            && inputs
                .get("image_url")
                .and_then(|v| v.as_str())
                .is_none_or(|s| s.trim().is_empty())
        {
            return Err(ToolError::InvalidParams(
                "img2video_direct requires image_url — pass the user's reference image URL".into(),
            ));
        }

        let plan = WorkflowPlan::from_definition(&def, inputs);
        Ok(json!({
            "plan": plan,
            "workflow_id": template_id,
            "available_templates": list_builtin_templates(),
            "next_tool": "media_workflow_run",
            "hint": "Call media_workflow_run with { \"plan\": <plan above> } to execute. Prompts will be refined for rich visual detail and motion."
        })
        .to_string())
    }

    fn schema(&self) -> hermes_core::ToolSchema {
        crate::tool_schemas::media_workflow_plan_schema()
    }
}
