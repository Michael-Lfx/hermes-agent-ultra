//! DAG workflow executor.

use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use hermes_core::{ToolError, ToolHandler};
use hermes_tools::{
    ImageGenBackend, ImageGenerateHandler, VideoGenerateBackend, VideoGenerateHandler,
};

use super::definition::{WorkflowDefinition, WorkflowPlan, WorkflowStep};
use super::store::{WorkflowRunRecord, WorkflowRunStatus, WorkflowRunStore};
use crate::backends::FlowyMediaServices;
use crate::llm_refine::{plan_storyboard, refine_with_llm_or_template};
use crate::progress::{
    prompt_refine_working, report_media_progress, storyboard_planning, storyboard_shot_image,
    storyboard_shot_video, workflow_started, workflow_step_progress,
};
use crate::prompt_refine::RefineInput;
use crate::qa::{qa_check_image, qa_check_video};

pub struct WorkflowExecutor {
    services: FlowyMediaServices,
    image_backend: Arc<dyn ImageGenBackend>,
    video_backend: Arc<dyn VideoGenerateBackend>,
    store: Arc<WorkflowRunStore>,
    max_retries: u32,
}

impl WorkflowExecutor {
    pub fn new(
        services: FlowyMediaServices,
        store: Arc<WorkflowRunStore>,
        max_retries: u32,
    ) -> Self {
        let image_backend = Arc::new(crate::backends::flowy_image::FlowyImageGenBackend::new(
            services.clone(),
        ));
        let video_backend = Arc::new(crate::backends::flowy_video::FlowyVideoGenBackend::new(
            services.clone(),
        ));
        Self {
            services,
            image_backend,
            video_backend,
            store,
            max_retries: max_retries.clamp(1, 5),
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
        let record = self.store.create_run(&def.id, def.inputs.clone());
        self.run_definition_existing(&record.run_id, def).await
    }

    pub async fn run_definition_existing(
        &self,
        run_id: &str,
        def: &WorkflowDefinition,
    ) -> Result<WorkflowRunRecord, ToolError> {
        let Some(mut record) = self.store.get(run_id) else {
            return Err(ToolError::ExecutionFailed(format!(
                "workflow run not found: {run_id}"
            )));
        };
        record.status = WorkflowRunStatus::Running;
        self.store.save(&record);

        record.status = WorkflowRunStatus::Running;
        self.store.save(&record);

        let order = topo_sort(&def.steps)?;
        let step_total = order.len();
        report_media_progress(workflow_started(&def.id, step_total));

        let mut ctx: HashMap<String, Value> = HashMap::new();
        ctx.insert("inputs".into(), def.inputs.clone());

        for (step_idx, step_id) in order.iter().enumerate() {
            let step = def
                .steps
                .iter()
                .find(|s| s.id == *step_id)
                .ok_or_else(|| ToolError::ExecutionFailed(format!("missing step {step_id}")))?;

            record.current_step = Some(step_id.clone());
            self.store.save(&record);

            let resolved_input = resolve_value(&step.input, &ctx);
            let medium = resolved_input.get("medium").and_then(|v| v.as_str());
            report_media_progress(workflow_step_progress(
                &def.id,
                step_idx + 1,
                step_total,
                &step.kind,
                step_id,
                medium,
            ));

            let output = match self.run_step_with_retry(step, &resolved_input).await {
                Ok(output) => output,
                Err(err) => {
                    record.status = WorkflowRunStatus::Failed;
                    record.error = Some(err.to_string());
                    record.current_step = None;
                    self.store.save(&record);
                    return Err(err);
                }
            };

            ctx.insert(format!("steps.{step_id}"), output.clone());
            record.step_outputs.insert(step_id.clone(), output);
            self.store.save(&record);
        }

        record.status = WorkflowRunStatus::Succeeded;
        record.current_step = None;
        record.artifacts = collect_artifacts(&record.step_outputs);
        self.store.save(&record);
        Ok(record)
    }

    async fn run_step_with_retry(
        &self,
        step: &WorkflowStep,
        input: &Value,
    ) -> Result<Value, ToolError> {
        let mut last_err = None;
        for attempt in 0..self.max_retries {
            match self.run_step(step, input).await {
                Ok(v) => return Ok(v),
                Err(err) => {
                    let retryable = is_retryable_error(&err);
                    tracing::warn!(
                        step = %step.id,
                        attempt = attempt + 1,
                        retryable,
                        error = %err,
                        "workflow step failed"
                    );
                    last_err = Some(err);
                    if !retryable || attempt + 1 >= self.max_retries {
                        break;
                    }
                    tokio::time::sleep(tokio::time::Duration::from_secs(2_u64.pow(attempt))).await;
                }
            }
        }
        Err(last_err.unwrap_or_else(|| {
            ToolError::ExecutionFailed("workflow step failed without error".into())
        }))
    }

    async fn run_step(&self, step: &WorkflowStep, input: &Value) -> Result<Value, ToolError> {
        match step.kind.as_str() {
            "image_generate" => self.run_image_step(input).await,
            "video_generate" => self.run_video_step(input).await,
            "prompt_refine" => self.run_prompt_refine(input).await,
            "storyboard_multi" => self.run_storyboard_multi(input).await,
            "qa_check" => self.run_qa_check(input).await,
            other => Err(ToolError::ExecutionFailed(format!(
                "unsupported workflow step kind: {other}"
            ))),
        }
    }

    async fn run_image_step(&self, input: &Value) -> Result<Value, ToolError> {
        let prompt = input
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("image step missing prompt".into()))?;
        let handler = ImageGenerateHandler::new(self.image_backend.clone());
        let mut body = json!({
            "prompt": prompt,
            "model": input.get("model"),
            "image_url": input.get("image_url"),
            "size": input.get("size"),
            "n": input.get("n"),
        });
        if let Some(extra) = input.get("extra") {
            body["extra"] = extra.clone();
        }
        let raw = handler.execute(body).await?;
        let parsed: Value = serde_json::from_str(&raw)
            .map_err(|e| ToolError::ExecutionFailed(format!("image step JSON: {e}")))?;
        let best_url = parsed
            .get("assets")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|a| a.get("url").or_else(|| a.get("local_path")))
            .or_else(|| {
                parsed
                    .get("images")
                    .and_then(|v| v.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|img| img.get("url").or_else(|| img.get("local_path")))
            })
            .cloned()
            .unwrap_or(Value::Null);
        Ok(json!({
            "raw": parsed,
            "api_prompt": prompt,
            "negative_prompt": input.get("negative_prompt").and_then(|v| v.as_str()),
            "best_url": best_url,
            "output": parsed.get("assets").or_else(|| parsed.get("images")).cloned().unwrap_or(Value::Null),
        }))
    }

    async fn run_video_step(&self, input: &Value) -> Result<Value, ToolError> {
        let prompt = input
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("video step missing prompt".into()))?;
        let reference_image_urls: Vec<String> = input
            .get("reference_image_urls")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();

        let handler = VideoGenerateHandler::new(self.video_backend.clone());
        let raw = handler
            .execute(json!({
                "prompt": prompt,
                "model": input.get("model"),
                "image_url": input.get("image_url"),
                "reference_image_urls": reference_image_urls,
                "duration": input.get("duration"),
                "aspect_ratio": input
                    .get("aspect_ratio")
                    .cloned()
                    .unwrap_or(json!("16:9")),
                "resolution": input
                    .get("resolution")
                    .cloned()
                    .unwrap_or(json!("720p")),
                "negative_prompt": input.get("negative_prompt"),
                "seed": input.get("seed"),
                "last_frame_url": input.get("last_frame_url"),
                "reference_video_url": input.get("reference_video_url"),
                "reference_audio_url": input.get("reference_audio_url"),
                "generate_audio": input.get("generate_audio"),
            }))
            .await?;
        let parsed: Value = serde_json::from_str(&raw)
            .map_err(|e| ToolError::ExecutionFailed(format!("video step JSON: {e}")))?;
        Ok(json!({
            "raw": parsed,
            "api_prompt": prompt,
            "negative_prompt": input.get("negative_prompt").and_then(|v| v.as_str()),
            "motion_prompt": input.get("motion_prompt").and_then(|v| v.as_str()),
            "video_url": parsed.get("video"),
            "local_path": parsed.pointer("/assets/0/local_path").or_else(|| parsed.get("local_path")),
            "output": parsed,
        }))
    }

    async fn run_prompt_refine(&self, input: &Value) -> Result<Value, ToolError> {
        let prompt = input.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
        let medium = input
            .get("medium")
            .and_then(|v| v.as_str())
            .unwrap_or("image");
        let aspect_ratio = input.get("aspect_ratio").and_then(|v| v.as_str());
        let has_reference_image = input
            .get("has_reference_image")
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| {
                input
                    .get("has_reference_image")
                    .and_then(|v| v.as_str())
                    .is_some_and(|s| s == "true")
            });

        let refined = {
            report_media_progress(prompt_refine_working(medium));
            refine_with_llm_or_template(
                &self.services,
                &RefineInput {
                    prompt,
                    medium,
                    aspect_ratio,
                    has_reference_image,
                },
            )
            .await
        };

        Ok(json!({
            "output": refined.output,
            "image_prompt": refined.image_prompt,
            "video_prompt": refined.video_prompt,
            "motion_prompt": refined.motion_prompt,
            "negative_prompt": refined.negative_prompt,
        }))
    }

    async fn run_storyboard_multi(&self, input: &Value) -> Result<Value, ToolError> {
        let prompt = input
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("storyboard missing prompt".into()))?;
        let aspect_ratio = input
            .get("aspect_ratio")
            .and_then(|v| v.as_str())
            .unwrap_or("16:9");
        let resolution = input
            .get("resolution")
            .and_then(|v| v.as_str())
            .unwrap_or("720p");
        let max_shots = input
            .get("max_shots")
            .and_then(|v| v.as_u64())
            .map(|n| n as u32)
            .unwrap_or(self.services.media.workflows.storyboard_max_shots)
            .clamp(1, 5);

        let plan = {
            report_media_progress(storyboard_planning());
            plan_storyboard(&self.services, prompt, max_shots).await
        };
        let shot_total = plan.shots.len();
        let mut shot_outputs = Vec::new();
        let mut artifacts = Vec::new();

        for (idx, shot) in plan.shots.iter().enumerate() {
            let shot_no = idx + 1;
            report_media_progress(storyboard_shot_image(shot_no, shot_total));
            let image_out = self
                .run_image_step(&json!({
                    "prompt": shot.scene_prompt,
                }))
                .await?;
            let best_url = image_out.get("best_url").cloned().unwrap_or(Value::Null);
            report_media_progress(storyboard_shot_video(
                shot_no,
                shot_total,
                shot.duration_secs,
            ));
            let video_out = self
                .run_video_step(&json!({
                    "prompt": shot.motion_prompt,
                    "image_url": best_url,
                    "duration": shot.duration_secs,
                    "aspect_ratio": aspect_ratio,
                    "resolution": resolution,
                    "negative_prompt": plan.negative_prompt,
                }))
                .await?;
            if let Some(path) = video_out
                .pointer("/raw/assets/0/local_path")
                .or_else(|| video_out.get("local_path"))
                .and_then(|p| p.as_str())
            {
                artifacts.push(json!({
                    "shot": idx + 1,
                    "local_path": path,
                    "kind": "video",
                }));
            }
            shot_outputs.push(json!({
                "shot": idx + 1,
                "image": image_out,
                "video": video_out,
            }));
        }

        Ok(json!({
            "output": shot_outputs,
            "shots": shot_outputs,
            "artifacts": artifacts,
            "negative_prompt": plan.negative_prompt,
        }))
    }

    async fn run_qa_check(&self, input: &Value) -> Result<Value, ToolError> {
        let kind = input
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("image");
        let target = input
            .get("target_step")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("qa_check missing target_step".into()))?;
        let step_output = input
            .get("step_output")
            .cloned()
            .ok_or_else(|| ToolError::InvalidParams("qa_check missing step_output".into()))?;

        let local_path = step_output
            .pointer("/raw/assets/0/local_path")
            .or_else(|| step_output.pointer("/assets/0/local_path"))
            .or_else(|| step_output.get("local_path"))
            .and_then(|v| v.as_str());

        let Some(path_str) = local_path else {
            return Ok(json!({
                "passed": true,
                "skipped": true,
                "reason": "no local_path to QA"
            }));
        };

        let path = std::path::PathBuf::from(path_str);
        let report = if kind == "video" {
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            qa_check_video(&path, size)
        } else {
            let bytes = std::fs::read(&path).unwrap_or_default();
            qa_check_image(&path, &bytes)
        };

        if !report.passed {
            let issues = report.issues.clone();
            report.into_result(&format!("{target} {kind}"))?;
            return Ok(json!({
                "passed": false,
                "target_step": target,
                "issues": issues,
            }));
        }
        Ok(json!({
            "passed": true,
            "target_step": target,
            "issues": report.issues,
        }))
    }
}

fn is_retryable_error(err: &ToolError) -> bool {
    let msg = err.to_string().to_ascii_lowercase();
    msg.contains("timeout")
        || msg.contains("rate")
        || msg.contains("429")
        || msg.contains("502")
        || msg.contains("503")
        || msg.contains("504")
        || msg.contains("temporarily")
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
        if let Some(embedded) = output.get("artifacts").and_then(|v| v.as_array()) {
            for item in embedded {
                if let Some(path) = item.get("local_path").and_then(|p| p.as_str()) {
                    let kind = item.get("kind").and_then(|k| k.as_str()).unwrap_or("video");
                    artifacts.push(json!({ "step": step, "local_path": path, "kind": kind }));
                }
            }
        }
        if let Some(assets) = output.pointer("/raw/assets").and_then(|v| v.as_array()) {
            for asset in assets {
                if let Some(path) = asset.get("local_path").and_then(|p| p.as_str()) {
                    let kind = asset
                        .get("kind")
                        .and_then(|k| k.as_str())
                        .unwrap_or("media");
                    artifacts.push(json!({ "step": step, "local_path": path, "kind": kind }));
                }
            }
        }
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

    #[test]
    fn retryable_errors_detected() {
        assert!(is_retryable_error(&ToolError::ExecutionFailed(
            "HTTP 503 temporarily unavailable".into()
        )));
        assert!(!is_retryable_error(&ToolError::InvalidParams("bad".into())));
    }
}
