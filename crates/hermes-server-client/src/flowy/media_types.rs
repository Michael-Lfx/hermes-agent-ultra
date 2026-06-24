//! Flowy image/video generation API types.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `tb_model.category` for image models (`GET .../model/availableListClaw?category=6`).
pub const MODEL_CATEGORY_IMAGE: i32 = 6;

/// `tb_model.category` for video models (`GET .../model/availableListClaw?category=4`).
pub const MODEL_CATEGORY_VIDEO: i32 = 4;

/// Local `tb_video_task.status` — succeeded.
pub const VIDEO_TASK_STATUS_SUCCEEDED: i32 = 4;

/// Local `tb_video_task.status` — failed.
pub const VIDEO_TASK_STATUS_FAILED: i32 = 5;

/// Local `tb_video_task.status` — expired.
pub const VIDEO_TASK_STATUS_EXPIRED: i32 = 6;

/// Local `tb_video_task.status` — cancelled.
pub const VIDEO_TASK_STATUS_CANCELLED: i32 = 3;

#[derive(Debug, Clone, Deserialize)]
pub struct CreateVideoTaskResponse {
    pub id: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VideoTaskRecord {
    pub id: i64,
    #[serde(default)]
    pub task_id: Option<String>,
    pub status: i32,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

impl VideoTaskRecord {
    pub fn video_url(&self) -> Option<String> {
        self.result
            .as_ref()
            .and_then(|r| r.get("content"))
            .and_then(|c| c.get("video_url"))
            .and_then(|u| u.as_str())
            .map(str::to_string)
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            VIDEO_TASK_STATUS_CANCELLED
                | VIDEO_TASK_STATUS_SUCCEEDED
                | VIDEO_TASK_STATUS_FAILED
                | VIDEO_TASK_STATUS_EXPIRED
        )
    }

    pub fn is_success(&self) -> bool {
        self.status == VIDEO_TASK_STATUS_SUCCEEDED
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ImageGenerationRequest {
    pub model: String,
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(flatten)]
    pub extra: Value,
}
