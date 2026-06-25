//! Unified tool response shape for image/video generation.

use serde::Serialize;
use serde_json::{Value, json};

use crate::assets::MediaArtifact;

#[derive(Debug, Clone, Serialize)]
pub struct MediaAssetDelivery {
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_hint: Option<String>,
    pub mime: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<f32>,
    pub provider: String,
    pub model: String,
    pub job_id: String,
}

impl From<&MediaArtifact> for MediaAssetDelivery {
    fn from(a: &MediaArtifact) -> Self {
        let local = (!a.local_path.as_os_str().is_empty())
            .then(|| a.local_path.to_string_lossy().into_owned());
        Self {
            kind: if a.mime.starts_with("video/") {
                "video"
            } else {
                "image"
            },
            url: a.remote_url.clone(),
            local_path: local.clone(),
            media_hint: local.as_ref().map(|p| format!("MEDIA:{p}")),
            mime: a.mime.clone(),
            width: a.width,
            height: a.height,
            duration_secs: a.duration_secs,
            provider: a.provider.clone(),
            model: a.model.clone(),
            job_id: a.job_id.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct MediaProvenance {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refined_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub negative_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
}

/// Build a standard image generation JSON response.
pub fn image_generation_response(
    model: &str,
    artifacts: &[MediaArtifact],
    upstream: &Value,
    provenance: MediaProvenance,
) -> String {
    let assets: Vec<MediaAssetDelivery> = artifacts.iter().map(MediaAssetDelivery::from).collect();
    let media_hint = assets
        .iter()
        .find_map(|a| a.media_hint.clone())
        .map(Value::String)
        .unwrap_or(Value::Null);
    let images: Vec<Value> = assets
        .iter()
        .map(|a| {
            json!({
                "url": a.url,
                "local_path": a.local_path,
                "media_hint": a.media_hint,
                "mime": a.mime,
                "width": a.width,
                "height": a.height,
                "provider": a.provider,
                "model": a.model,
                "job_id": a.job_id,
            })
        })
        .collect();
    json!({
        "success": true,
        "kind": "image",
        "assets": assets,
        "images": images,
        "transport": "flowy",
        "model": model,
        "upstream": upstream,
        "provenance": provenance,
        "media_hint": media_hint,
    })
    .to_string()
}

/// Build a standard video generation JSON response.
pub fn video_generation_response(
    model: &str,
    video_url: &str,
    artifact: Option<&MediaArtifact>,
    task: &VideoTaskMeta,
    provenance: MediaProvenance,
    persist_warning: Option<&str>,
) -> String {
    let assets: Vec<MediaAssetDelivery> = artifact
        .map(|a| vec![MediaAssetDelivery::from(a)])
        .unwrap_or_else(|| {
            vec![MediaAssetDelivery {
                kind: "video",
                url: Some(video_url.to_string()),
                local_path: None,
                media_hint: None,
                mime: "video/mp4".into(),
                width: None,
                height: None,
                duration_secs: None,
                provider: "flowy".into(),
                model: model.to_string(),
                job_id: task.task_id.clone(),
            }]
        });
    let media_hint = assets
        .first()
        .and_then(|a| a.media_hint.clone())
        .map(Value::String)
        .unwrap_or(Value::Null);
    json!({
        "success": true,
        "kind": "video",
        "assets": assets,
        "video": video_url,
        "local_path": assets.first().and_then(|a| a.local_path.clone()),
        "provider": "flowy",
        "model": model,
        "task_id": task.local_id,
        "upstream_task_id": task.task_id,
        "status": task.status,
        "provenance": provenance,
        "persist_warning": persist_warning,
        "media_hint": media_hint,
        "delivery_note": if media_hint.is_null() {
            json!("Video URL is available; share the link or retry download if MEDIA: path is needed.")
        } else {
            Value::Null
        },
    })
    .to_string()
}

pub struct VideoTaskMeta {
    pub local_id: String,
    pub task_id: String,
    pub status: i32,
}
