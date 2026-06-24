//! DAG workflow executor.

use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use hermes_core::{ToolError, ToolHandler};
use hermes_tools::tools::image_gen::ImageGenRequest;
use hermes_tools::tools::video::VideoGenerateRequest;
use hermes_tools::{
    ImageGenBackend, ImageGenerateHandler, VideoGenerateBackend, VideoGenerateHandler,
};

use super::definition::{WorkflowDefinition, WorkflowPlan, WorkflowStep};
use super::store::{WorkflowRunRecord, WorkflowRunStatus, WorkflowRunStore};
use crate::backends::FlowyMediaServices;

pub struct WorkflowExecutor {
    image_backend: Arc<dyn ImageGenBackend>,
    video_backend: Arc<dyn VideoGenerateBackend>,
    store: Arc<WorkflowRunStore>,
}

impl WorkflowExecutor {
    pub fn new(services: FlowyMediaServices, store: Arc<WorkflowRunStore>) -> Self {
        let image_backend = Arc::new(crate::backends::flowy_image::FlowyImageGenBackend::new(
            services.clone(),
        ));
        let video_backend = Arc::new(crate::backends::flowy_video::FlowyVideoGenBackend::new(
            services.clone(),
        ));
        Self {
            image_backend,
            video_backend,
            store,
        }
    }

    pub async fn run_plan(&self, plan: &WorkflowPlan) -> Result<WorkflowRunRecord, ToolError> {
        let def = WorkflowDefinition {
            id: plan.workflow_id.clone(),
            version: plan.template_version,
            description: String::new(),
            inputs: plan.inputs.clone(),
            steps: plan.steps.clone(),
        };
        self.run_definition(&def).await
    }

    pub async fn run_definition(
        &self,
        def: &WorkflowDefinition,
    ) -> Result<WorkflowRunRecord, ToolError> {
        let mut record = self.store.create_run(&def.id, def.inputs.clone());
        record.status = WorkflowRunStatus::Running;
        self.store.save(&record);

        let order = topo_sort(&def.steps)?;
        let mut ctx: HashMap<String, Value> = HashMap::new();
        ctx.insert("inputs".into(), def.inputs.clone());

        for step_id in order {
            let step = def
                .steps
                .iter()
                .find(|s| s.id == step_id)
                .ok_or_else(|| ToolError::ExecutionFailed(format!("missing step {step_id}")))?;

            record.current_step = Some(step_id.clone());
            self.store.save(&record);

            let resolved_input = resolve_value(&step.input, &ctx);
            let output = match step.kind.as_str() {
                "image_generate" => self.run_image_step(&resolved_input).await?,
                "video_generate" => self.run_video_step(&resolved_input).await?,
                "prompt_refine" => self.run_prompt_refine(&resolved_input).await?,
                other => {
                    return Err(ToolError::ExecutionFailed(format!(
                        "unsupported workflow step kind: {other}"
                    )));
                }
            };

            ctx.insert(format!("steps.{step_id}"), output.clone());
            record.step_outputs.insert(step_id, output);
            self.store.save(&record);
        }

        record.status = WorkflowRunStatus::Succeeded;
        record.current_step = None;
        record.artifacts = collect_artifacts(&record.step_outputs);
        self.store.save(&record);
        Ok(record)
    }

    async fn run_image_step(&self, input: &Value) -> Result<Value, ToolError> {
        let prompt = input
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("image step missing prompt".into()))?;
        let request = ImageGenRequest {
            prompt: prompt.to_string(),
            model: input
                .get("model")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            image_url: input
                .get("image_url")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            size: input
                .get("size")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            style: None,
            n: input.get("n").and_then(|v| v.as_u64()).map(|n| n as u32),
            extra: None,
        };
        let handler = ImageGenerateHandler::new(self.image_backend.clone());
        let raw = handler
            .execute(json!({
                "prompt": request.prompt,
                "model": request.model,
                "image_url": request.image_url,
                "size": request.size,
                "n": request.n,
            }))
            .await?;
        let parsed: Value = serde_json::from_str(&raw)
            .map_err(|e| ToolError::ExecutionFailed(format!("image step JSON: {e}")))?;
        let best_url = parsed
            .get("images")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|img| img.get("url").or_else(|| img.get("local_path")))
            .cloned()
            .unwrap_or(Value::Null);
        Ok(json!({
            "raw": parsed,
            "best_url": best_url,
            "output": parsed.get("images").cloned().unwrap_or(Value::Null),
        }))
    }

    async fn run_video_step(&self, input: &Value) -> Result<Value, ToolError> {
        let prompt = input
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("video step missing prompt".into()))?;
        let request = VideoGenerateRequest {
            prompt: prompt.to_string(),
            model: input
                .get("model")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            model_explicit: input.get("model").is_some(),
            image_url: input
                .get("image_url")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            reference_image_urls: vec![],
            duration: input
                .get("duration")
                .and_then(|v| v.as_u64())
                .map(|n| n as u32),
            aspect_ratio: input
                .get("aspect_ratio")
                .and_then(|v| v.as_str())
                .unwrap_or("16:9")
                .to_string(),
            resolution: input
                .get("resolution")
                .and_then(|v| v.as_str())
                .unwrap_or("720p")
                .to_string(),
            negative_prompt: None,
            audio: None,
            seed: None,
        };
        let handler = VideoGenerateHandler::new(self.video_backend.clone());
        let raw = handler
            .execute(json!({
                "prompt": request.prompt,
                "model": request.model,
                "image_url": request.image_url,
                "duration": request.duration,
                "aspect_ratio": request.aspect_ratio,
                "resolution": request.resolution,
            }))
            .await?;
        let parsed: Value = serde_json::from_str(&raw)
            .map_err(|e| ToolError::ExecutionFailed(format!("video step JSON: {e}")))?;
        Ok(json!({
            "raw": parsed,
            "video_url": parsed.get("video"),
            "local_path": parsed.get("local_path"),
            "output": parsed,
        }))
    }

    async fn run_prompt_refine(&self, input: &Value) -> Result<Value, ToolError> {
        let prompt = input.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
        let medium = input
            .get("medium")
            .and_then(|v| v.as_str())
            .unwrap_or("image");
        // Lightweight local refinement — avoids extra LLM round-trip when auxiliary is unset.
        let refined = if medium == "video" {
            format!(
                "Cinematic {}, high detail, stable composition, natural motion, 4K quality",
                prompt.trim()
            )
        } else {
            format!(
                "Highly detailed {}, professional lighting, sharp focus, masterpiece quality",
                prompt.trim()
            )
        };
        Ok(json!({ "output": refined }))
    }
}

fn topo_sort(steps: &[WorkflowStep]) -> Result<Vec<String>, ToolError> {
    let mut deps: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut ids = HashSet::new();
    for step in steps {
        ids.insert(step.id.as_str());
        deps.insert(
            step.id.as_str(),
            step.depends_on.iter().map(String::as_str).collect(),
        );
    }
    let mut order = Vec::new();
    let mut visited = HashSet::new();
    let mut temp = HashSet::new();

    fn visit<'a>(
        id: &'a str,
        deps: &HashMap<&'a str, Vec<&'a str>>,
        visited: &mut HashSet<&'a str>,
        temp: &mut HashSet<&'a str>,
        order: &mut Vec<String>,
    ) -> Result<(), ToolError> {
        if visited.contains(id) {
            return Ok(());
        }
        if !temp.insert(id) {
            return Err(ToolError::ExecutionFailed(format!(
                "workflow cycle detected at step {id}"
            )));
        }
        if let Some(step_deps) = deps.get(id) {
            for dep in step_deps {
                visit(dep, deps, visited, temp, order)?;
            }
        }
        temp.remove(id);
        visited.insert(id);
        order.push(id.to_string());
        Ok(())
    }

    for id in ids {
        visit(id, &deps, &mut visited, &mut temp, &mut order)?;
    }
    Ok(order)
}

fn resolve_value(template: &Value, ctx: &HashMap<String, Value>) -> Value {
    match template {
        Value::String(s) if s.starts_with('$') => resolve_ref(s, ctx).unwrap_or(Value::Null),
        Value::Array(arr) => Value::Array(arr.iter().map(|v| resolve_value(v, ctx)).collect()),
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                out.insert(k.clone(), resolve_value(v, ctx));
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}

fn resolve_ref(path: &str, ctx: &HashMap<String, Value>) -> Option<Value> {
    let path = path.trim_start_matches('$');
    let parts: Vec<&str> = path.split('.').collect();
    if parts.is_empty() {
        return None;
    }
    let root = ctx.get(parts[0])?;
    let mut cur = root;
    for part in parts.iter().skip(1) {
        cur = cur.get(*part)?;
    }
    Some(cur.clone())
}

fn collect_artifacts(outputs: &HashMap<String, Value>) -> Vec<Value> {
    let mut artifacts = Vec::new();
    for (step, output) in outputs {
        if let Some(local) = output
            .pointer("/raw/local_path")
            .or_else(|| output.get("local_path"))
            && local.is_string()
        {
            artifacts.push(json!({ "step": step, "local_path": local, "kind": "video" }));
        }
        if let Some(images) = output.pointer("/raw/images").and_then(|v| v.as_array()) {
            for img in images {
                if let Some(path) = img.get("local_path") {
                    artifacts.push(json!({ "step": step, "local_path": path, "kind": "image" }));
                }
            }
        }
    }
    artifacts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topo_sort_respects_dependencies() {
        let steps = vec![
            WorkflowStep {
                id: "b".into(),
                kind: "image_generate".into(),
                depends_on: vec!["a".into()],
                input: json!({}),
                on_fail: None,
            },
            WorkflowStep {
                id: "a".into(),
                kind: "prompt_refine".into(),
                depends_on: vec![],
                input: json!({}),
                on_fail: None,
            },
        ];
        let order = topo_sort(&steps).expect("sort");
        assert_eq!(order, vec!["a", "b"]);
    }
}
