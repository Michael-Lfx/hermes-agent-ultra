//! Turn-end task resource cleanup (Python `_cleanup_task_resources`).

use tracing::{debug, warn};

/// Best-effort release of browser/VM resources scoped to a turn `task_id`.
pub fn cleanup_task_resources(task_id: &str) {
    let task_id = task_id.trim();
    if task_id.is_empty() {
        return;
    }
    debug!(task_id = %task_id, "turn-end task resource cleanup");
    if let Ok(backend) = crate::backends::agent_browser::AgentBrowserBackend::new() {
        backend.release_task_session(task_id);
    }
    if std::env::var("HERMES_VM_TASK_CLEANUP")
        .ok()
        .map(|v| matches!(v.trim(), "1" | "true" | "yes"))
        .unwrap_or(false)
    {
        warn!(
            task_id = %task_id,
            "HERMES_VM_TASK_CLEANUP is set but VM teardown is not wired in Rust yet"
        );
    }
}
