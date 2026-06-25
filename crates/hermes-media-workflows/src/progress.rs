//! User-facing progress messages for Flowy media tools.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use hermes_core::tool_progress::report_tool_progress;
use tokio::task::JoinHandle;

pub fn report_media_progress(message: impl Into<String>) {
    report_tool_progress(message);
}

/// Periodic progress while a long-running media operation blocks.
pub struct MediaProgressHeartbeat {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl MediaProgressHeartbeat {
    /// Emit `message(elapsed_secs)` every `interval_secs` until stopped.
    pub fn start(interval_secs: u64, message: impl Fn(u64) -> String + Send + 'static) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_worker = Arc::clone(&stop);
        let interval = Duration::from_secs(interval_secs.max(3));
        let handle = tokio::spawn(async move {
            let mut elapsed = 0u64;
            loop {
                tokio::time::sleep(interval).await;
                if stop_worker.load(Ordering::Acquire) {
                    break;
                }
                elapsed = elapsed.saturating_add(interval.as_secs());
                report_media_progress(message(elapsed));
            }
        });
        Self {
            stop,
            handle: Some(handle),
        }
    }

    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

/// Workflow step label with template id and position.
pub fn workflow_step_progress(
    workflow_id: &str,
    step_no: usize,
    step_total: usize,
    kind: &str,
    step_id: &str,
    medium: Option<&str>,
) -> String {
    let prefix = format!("[{workflow_id}] 步骤 {step_no}/{step_total}");
    match kind {
        "prompt_refine" => {
            let what = match medium {
                Some("video") => "视频场景与运动描述",
                Some("motion") => "图生视频运动描述",
                _ => "图片描述",
            };
            format!("{prefix}：正在用 AI 优化{what}（{step_id}）")
        }
        "image_generate" => {
            let label = if medium == Some("motion") || step_id.contains("keyframe") {
                "关键帧图片"
            } else {
                "图片"
            };
            format!("{prefix}：正在生成{label}（{step_id}）")
        }
        "video_generate" => format!("{prefix}：正在生成视频片段（{step_id}）"),
        "storyboard_multi" => {
            format!("{prefix}：正在规划分镜并依次生成各镜头（{step_id}）")
        }
        "qa_check" => format!("{prefix}：正在检查生成质量（{step_id}）"),
        other => format!("{prefix}：正在执行 {step_id}（{other}）"),
    }
}

pub fn workflow_started(workflow_id: &str, step_total: usize) -> String {
    format!("[{workflow_id}] 工作流已开始，共 {step_total} 个步骤")
}

pub fn prompt_refine_working(medium: &str) -> &'static str {
    match medium {
        "video" => "正在用 AI 细化视频画面与镜头运动…",
        "motion" => "正在用 AI 细化图生视频的运动描述…",
        _ => "正在用 AI 细化图片描述与画面细节…",
    }
}

pub fn storyboard_planning() -> &'static str {
    "正在用 AI 规划分镜脚本（场景 + 运动）…"
}

pub fn storyboard_shot_image(shot: usize, total: usize) -> String {
    format!("分镜 {shot}/{total}：正在生成该镜头关键帧图片…")
}

pub fn storyboard_shot_video(shot: usize, total: usize, duration_secs: u32) -> String {
    format!("分镜 {shot}/{total}：正在将该镜头转为约 {duration_secs} 秒视频…")
}

pub fn image_credits_check() -> &'static str {
    "正在检查图片生成积分余额…"
}

pub fn image_resolving_model() -> &'static str {
    "正在选择图片模型…"
}

pub fn image_submitting() -> &'static str {
    "正在向云端提交图片生成请求…"
}

pub fn image_waiting_upstream(elapsed_secs: u64) -> String {
    format!("正在等待云端绘图（已等待 {elapsed_secs} 秒）…")
}

pub fn image_persisting() -> &'static str {
    "图片已生成，正在下载并保存到本地…"
}

/// Opening message when `video_generate` starts.
pub fn video_generate_started(has_image: bool, duration_secs: u32) -> String {
    if has_image {
        format!("已提交图生视频任务（约 {duration_secs} 秒成片），正在连接云端…")
    } else {
        format!("已提交文生视频任务（约 {duration_secs} 秒成片），正在连接云端…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workflow_step_labels_differ_by_medium() {
        let img = workflow_step_progress("txt2img", 1, 3, "prompt_refine", "refine", Some("image"));
        assert!(img.contains("图片描述"));
        let vid =
            workflow_step_progress("txt2video", 1, 2, "prompt_refine", "refine", Some("video"));
        assert!(vid.contains("视频场景"));
    }
}
