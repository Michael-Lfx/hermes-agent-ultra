//! Scoped tool execution progress for gateway / CLI status callbacks.
//!
//! Long-running tools (e.g. Flowy video poll) call [`report_tool_progress`] while the
//! agent has installed a reporter for the current tool batch.

use std::sync::{Arc, Mutex, MutexGuard};

type Reporter = Arc<dyn Fn(&str) + Send + Sync>;

struct Slot {
    reporter: Option<Reporter>,
    last_detail: Option<String>,
}

fn slot() -> MutexGuard<'static, Slot> {
    static SLOT: Mutex<Slot> = Mutex::new(Slot {
        reporter: None,
        last_detail: None,
    });
    SLOT.lock().unwrap_or_else(|e| e.into_inner())
}

/// Install a progress reporter for the current tool batch; cleared on drop.
pub struct ToolProgressGuard {
    active: bool,
}

impl ToolProgressGuard {
    pub fn install(reporter: Reporter) -> Self {
        let mut guard = slot();
        guard.reporter = Some(reporter);
        guard.last_detail = None;
        Self { active: true }
    }
}

impl Drop for ToolProgressGuard {
    fn drop(&mut self) {
        if self.active {
            let mut guard = slot();
            guard.reporter = None;
            guard.last_detail = None;
        }
    }
}

/// Report a user-visible progress line (also stored for generic watchdog fallback).
pub fn report_tool_progress(message: impl Into<String>) {
    let message = message.into();
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return;
    }
    let reporter = {
        let mut guard = slot();
        if guard.last_detail.as_deref() == Some(trimmed) {
            return;
        }
        guard.last_detail = Some(trimmed.to_string());
        guard.reporter.clone()
    };
    if let Some(cb) = reporter {
        cb(trimmed);
    }
}

/// Latest detailed progress from a running tool, if any.
pub fn current_tool_progress_detail() -> Option<String> {
    slot().last_detail.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn guard_reports_and_clears() {
        let count = Arc::new(AtomicUsize::new(0));
        let count_cb = Arc::clone(&count);
        {
            let _g = ToolProgressGuard::install(Arc::new(move |msg| {
                assert_eq!(msg, "正在渲染视频");
                count_cb.fetch_add(1, Ordering::SeqCst);
            }));
            report_tool_progress("正在渲染视频");
            report_tool_progress("正在渲染视频");
            assert_eq!(count.load(Ordering::SeqCst), 1);
            assert_eq!(
                current_tool_progress_detail().as_deref(),
                Some("正在渲染视频")
            );
        }
        assert!(current_tool_progress_detail().is_none());
    }
}
