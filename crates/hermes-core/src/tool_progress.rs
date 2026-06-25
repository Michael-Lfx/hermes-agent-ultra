//! Scoped tool execution progress for gateway / CLI status callbacks.
//!
//! Long-running tools (e.g. Flowy video poll) call [`report_tool_progress`] while the
//! agent has installed a reporter for the current tool batch.
//!
//! Background workflow runs capture the active reporter via [`DetachedToolProgressGuard`]
//! so progress still reaches WeCom/CLI after the spawning tool returns.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};

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

fn detached_reporters() -> MutexGuard<'static, HashMap<String, Reporter>> {
    static DETACHED: LazyLock<Mutex<HashMap<String, Reporter>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    DETACHED.lock().unwrap_or_else(|e| e.into_inner())
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

/// Keeps a captured gateway reporter alive for a background workflow run.
pub struct DetachedToolProgressGuard {
    run_id: String,
}

impl DetachedToolProgressGuard {
    /// Capture the currently installed reporter (if any) for `run_id`.
    pub fn attach(run_id: impl Into<String>) -> Option<Self> {
        let run_id = run_id.into();
        let reporter = slot().reporter.clone()?;
        detached_reporters().insert(run_id.clone(), reporter);
        Some(Self { run_id })
    }
}

impl Drop for DetachedToolProgressGuard {
    fn drop(&mut self) {
        detached_reporters().remove(&self.run_id);
    }
}

/// Clone the active scoped reporter (used when spawning detached work).
pub fn capture_tool_progress_reporter() -> Option<Reporter> {
    slot().reporter.clone()
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
    } else {
        for cb in detached_reporters().values() {
            cb(trimmed);
        }
    }
}

/// Latest detailed progress from a running tool, if any.
pub fn current_tool_progress_detail() -> Option<String> {
    slot().last_detail.clone()
}

#[cfg(test)]
fn reset_tool_progress_for_tests() {
    let mut guard = slot();
    guard.reporter = None;
    guard.last_detail = None;
    detached_reporters().clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn guard_reports_and_clears() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_tool_progress_for_tests();
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

    #[test]
    fn detached_guard_forwards_after_scope_ends() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_tool_progress_for_tests();
        let count = Arc::new(AtomicUsize::new(0));
        let count_cb = Arc::clone(&count);
        let detached = {
            let _g = ToolProgressGuard::install(Arc::new(move |_| {
                count_cb.fetch_add(1, Ordering::SeqCst);
            }));
            DetachedToolProgressGuard::attach("run-1").expect("attach")
        };
        report_tool_progress("后台工作流：正在生成图片");
        assert_eq!(count.load(Ordering::SeqCst), 1);
        drop(detached);
        report_tool_progress("不应再转发");
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }
}
