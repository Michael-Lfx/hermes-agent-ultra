use async_trait::async_trait;
use indexmap::IndexMap;
use serde_json::{Value, json};

use hermes_config::MediaGenConfig;
use hermes_core::{JsonSchema, ToolError, ToolHandler, ToolSchema, tool_schema};

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

        let template_id = params
            .get("workflow_id")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| {
                let has_image = params
                    .get("image_url")
                    .and_then(|v| v.as_str())
                    .is_some_and(|s| !s.trim().is_empty());
                suggest_template_id(objective, has_image).to_string()
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
        if let Some(extra) = params.get("inputs").and_then(Value::as_object) {
            for (k, v) in extra {
                inputs[k] = v.clone();
            }
        }

        let plan = WorkflowPlan::from_definition(&def, inputs);
        Ok(json!({
            "plan": plan,
            "workflow_id": template_id,
            "available_templates": list_builtin_templates(),
            "media_config": self.media_config,
            "next_tool": "media_workflow_run",
            "hint": "Call media_workflow_run with { \"plan\": <plan above> } to execute"
        })
        .to_string())
    }

    fn schema(&self) -> ToolSchema {
        let mut props = IndexMap::new();
        props.insert(
            "objective".into(),
            json!({"type":"string","description":"What to generate (image/video description)"}),
        );
        props.insert(
            "workflow_id".into(),
            json!({"type":"string","description":"Optional builtin template id","enum": list_builtin_templates()}),
        );
        props.insert(
            "image_url".into(),
            json!({"type":"string","description":"Optional reference image for img2video workflows"}),
        );
        props.insert(
            "duration".into(),
            json!({"type":"integer","description":"Video duration in seconds"}),
        );
        props.insert(
            "aspect_ratio".into(),
            json!({"type":"string","description":"Output aspect ratio","default":"16:9"}),
        );
        tool_schema(
            "media_workflow_plan",
            "Plan a multi-step image/video workflow (template selection + parameters).",
            JsonSchema::object(props, vec!["objective".into()]),
        )
    }
}
