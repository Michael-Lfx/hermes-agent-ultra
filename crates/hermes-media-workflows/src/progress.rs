//! User-facing progress messages for Flowy media tools.

use hermes_core::tool_progress::report_tool_progress;

pub fn report_media_progress(message: impl Into<String>) {
    report_tool_progress(message);
}

/// Human-readable label for a workflow step kind.
pub fn workflow_step_progress(kind: &str, step_id: &str) -> String {
    match kind {
        "prompt_refine" => format!("正在优化提示词（步骤 {step_id}）"),
        "image_generate" => format!("正在生成关键帧图片（步骤 {step_id}）"),
        "video_generate" => format!("正在生成视频（步骤 {step_id}）"),
        "storyboard_multi" => "正在规划分镜并依次生成各镜头…".into(),
        "qa_check" => format!("正在检查生成质量（步骤 {step_id}）"),
        other => format!("正在执行工作流步骤 {step_id}（{other}）"),
    }
}

/// Opening message when `video_generate` starts.
pub fn video_generate_started(has_image: bool, duration_secs: u32) -> String {
    if has_image {
        format!("已提交图生视频任务（约 {duration_secs} 秒成片），正在连接云端…")
    } else {
        format!("已提交文生视频任务（约 {duration_secs} 秒成片），正在连接云端…")
    }
}
