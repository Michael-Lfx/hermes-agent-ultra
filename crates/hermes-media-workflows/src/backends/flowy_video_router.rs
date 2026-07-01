//! Routes `video_generate` to long-video workflow when duration exceeds Seedance clip limit.

use std::sync::Arc;

use async_trait::async_trait;
use hermes_core::ToolError;
use hermes_tools::VideoGenerateBackend;
use hermes_tools::tools::video::VideoGenerateRequest;
use tracing::info;

use super::flowy_video::FlowyVideoGenBackend;
use crate::long_video_plan::{
    build_long_video_plan_from_request, resolve_target_duration, video_request_needs_long_pipeline,
    video_tool_response_from_workflow,
};
use crate::progress::report_media_progress;
use crate::workflows::runner::WorkflowRunner;

/// Flowy video backend that transparently runs long-video workflows for >10s targets.
pub struct FlowyVideoGenerateRouter {
    inner: FlowyVideoGenBackend,
    runner: Arc<WorkflowRunner>,
}

impl FlowyVideoGenerateRouter {
    pub fn new(inner: FlowyVideoGenBackend, runner: Arc<WorkflowRunner>) -> Self {
        Self { inner, runner }
    }
}

#[async_trait]
impl VideoGenerateBackend for FlowyVideoGenerateRouter {
    async fn generate_video(&self, request: VideoGenerateRequest) -> Result<String, ToolError> {
        let model = self
            .inner
            .services()
            .resolve_video_model(request.model.as_deref())
            .await?;
        let default_duration = self.inner.services().media.video.default_duration;

        if !video_request_needs_long_pipeline(&request, &model, default_duration) {
            return self.inner.generate_video(request).await;
        }

        let target = resolve_target_duration(request.duration, &request.prompt, default_duration);
        info!(
            target_duration = target,
            model = %model,
            "video_generate routing to long-video workflow"
        );
        report_media_progress(format!(
            "目标时长 {target} 秒超过 Seedance 单次上限，自动分段生成长视频…"
        ));

        let plan = build_long_video_plan_from_request(&request, target, &model)?;
        let record = self.runner.run_plan_sync(&plan).await?;
        video_tool_response_from_workflow(&record)
    }
}
