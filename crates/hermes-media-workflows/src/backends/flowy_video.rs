//! Flowy Seedance video generation backend.

use async_trait::async_trait;
use serde_json::json;

use hermes_core::ToolError;
use hermes_server_client::flowy::video_task_failure_message;
use hermes_tools::VideoGenerateBackend;
use hermes_tools::tools::video::VideoGenerateRequest;

use super::{FlowyMediaServices, map_server_err};
use crate::assets::persist_from_url;

pub struct FlowyVideoGenBackend {
    services: FlowyMediaServices,
}

impl FlowyVideoGenBackend {
    pub fn new(services: FlowyMediaServices) -> Self {
        Self { services }
    }

    pub async fn is_configured(services: &FlowyMediaServices) -> bool {
        services.is_authenticated().await
    }
}

#[async_trait]
impl VideoGenerateBackend for FlowyVideoGenBackend {
    async fn generate_video(&self, request: VideoGenerateRequest) -> Result<String, ToolError> {
        self.services.require_token().await?;

        let model = if request.model_explicit {
            request.model.clone().unwrap_or_default()
        } else {
            match request.model.as_deref() {
                Some(m) if !m.trim().is_empty() => m.trim().to_string(),
                _ => self.services.default_video_model().await?,
            }
        };

        if model.trim().is_empty() {
            return Err(ToolError::InvalidParams("missing video model".into()));
        }

        let image_url = request
            .image_url
            .or_else(|| request.reference_image_urls.first().cloned());

        let duration = request
            .duration
            .or(Some(self.services.media.video.default_duration));

        let body = hermes_server_client::FlowyApiClient::build_video_create_body(
            &model,
            &request.prompt,
            image_url.as_deref(),
            duration,
            if request.aspect_ratio.trim().is_empty() {
                self.services.media.video.default_aspect_ratio.as_str()
            } else {
                request.aspect_ratio.as_str()
            },
            Some(if request.resolution.trim().is_empty() {
                self.services.media.video.default_resolution.as_str()
            } else {
                request.resolution.as_str()
            }),
            request.negative_prompt.as_deref(),
            request.seed,
            false,
        );

        let record = self
            .services
            .api
            .generate_video(&self.services.session, body)
            .await
            .map_err(map_server_err)?;

        if !record.is_success() {
            return Err(ToolError::ExecutionFailed(video_task_failure_message(
                &record,
            )));
        }

        let video_url = record.video_url().ok_or_else(|| {
            ToolError::ExecutionFailed("video task succeeded but no video_url in result".into())
        })?;

        let mut local_path = String::new();
        if self.services.media.video.save_locally {
            let artifact = persist_from_url(&video_url, "flowy", &model).await?;
            local_path = artifact.local_path.to_string_lossy().to_string();
        }

        Ok(json!({
            "success": true,
            "video": video_url,
            "local_path": if local_path.is_empty() { serde_json::Value::Null } else { json!(local_path) },
            "provider": "flowy",
            "model": model,
            "task_id": record.id,
            "upstream_task_id": record.task_id,
            "status": record.status,
            "media_hint": if local_path.is_empty() { serde_json::Value::Null } else { json!(format!("MEDIA:{local_path}")) },
        })
        .to_string())
    }
}
