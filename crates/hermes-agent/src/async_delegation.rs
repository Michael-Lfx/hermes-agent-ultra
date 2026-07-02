//! Async (background) delegation registry.
//!
//! Backs `delegate_task(background=true)`: the parent agent dispatches a
//! subagent that runs on a background tokio task and returns a handle
//! immediately, so the user and the model can keep working while the child runs.
//!
//! When the child finishes, a completion event is pushed onto the internal
//! completion channel. The CLI / gateway drains this channel between turns
//! and forges a fresh user/internal turn from each event — the result
//! re-enters the conversation as a new message when the subagent is done.
//!
//! This module owns ONLY the async lifecycle. The actual child build + run is
//! delegated back to the caller via an injected runner future, so all the
//! credential leasing, timeout, and result-shaping logic stays in the
//! orchestrator.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use hermes_core::UsageStats;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::interrupt::InterruptController;

/// Default concurrency cap for background delegations.
pub const DEFAULT_MAX_ASYNC_CHILDREN: usize = 3;

/// How many completed records to retain for status queries before pruning.
const MAX_RETAINED_COMPLETED: usize = 50;

/// Boxed send future used for the runner.
pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

/// Status of an async delegation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DelegationStatus {
    Running,
    Completed,
    Error,
    Interrupted,
}

/// A completion event pushed onto the queue when a background delegation finishes.
///
/// The CLI / gateway drains these and injects them as new turns so the
/// subagent's result re-enters the conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsyncDelegationEvent {
    pub delegation_id: String,
    /// Gateway session key (empty => CLI single-session path).
    pub session_key: String,
    /// The original task goal.
    pub goal: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub toolset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// "completed", "error", "interrupted", "timeout"
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default)]
    pub total_turns: u32,
    #[serde(default)]
    pub api_calls: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_cost_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub_agent_id: Option<String>,
    pub duration_seconds: f64,
    pub dispatched_at: f64,
    pub completed_at: f64,
}

/// Result produced by the runner future. The orchestrator builds and runs the
/// child, then packages the outcome into this struct.
#[derive(Debug, Clone)]
pub struct BackgroundDelegationResult {
    /// "completed", "error", "cancelled", "timeout"
    pub status: String,
    pub summary: Option<String>,
    pub error: Option<String>,
    pub total_turns: u32,
    pub api_calls: u32,
    pub usage: Option<UsageStats>,
    pub sub_agent_id: String,
}

/// Parameters captured at dispatch time for the completion event.
#[derive(Debug, Clone)]
pub struct DispatchParams {
    pub goal: String,
    pub context: Option<String>,
    pub toolset: Option<String>,
    pub model: Option<String>,
    /// Gateway session key for routing the completion back to the originating
    /// session. Empty string => CLI (single-session) path.
    pub session_key: String,
}

/// Outcome of a dispatch attempt.
#[derive(Debug)]
pub enum DispatchOutcome {
    /// The delegation was accepted and is running in the background.
    Dispatched {
        delegation_id: String,
    },
    /// The async pool is at capacity — caller should fall back to sync.
    Rejected {
        error: String,
    },
}

/// Snapshot of a delegation record for status queries (excludes the
/// non-serialisable interrupt handle).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationInfo {
    pub delegation_id: String,
    pub goal: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub toolset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub session_key: String,
    pub status: String,
    pub dispatched_at: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<f64>,
}

/// Internal record for each async delegation.
struct DelegationRecord {
    delegation_id: String,
    goal: String,
    context: Option<String>,
    toolset: Option<String>,
    model: Option<String>,
    session_key: String,
    status: DelegationStatus,
    dispatched_at: Instant,
    dispatched_ts: f64,
    completed_at: Option<Instant>,
    /// Interrupt handle for signalling the background child to stop.
    interrupt: InterruptController,
}

/// The async delegation registry — manages background delegation lifecycle.
///
/// Always used as `Arc<AsyncDelegationRegistry>` so the spawned background
/// task can update records and push completion events.
pub struct AsyncDelegationRegistry {
    max_async_children: usize,
    records: Mutex<HashMap<String, DelegationRecord>>,
    completion_tx: mpsc::UnboundedSender<AsyncDelegationEvent>,
    completion_rx: Mutex<mpsc::UnboundedReceiver<AsyncDelegationEvent>>,
}

impl AsyncDelegationRegistry {
    /// Create a new registry with the default concurrency cap.
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_MAX_ASYNC_CHILDREN)
    }

    /// Create a new registry with a custom concurrency cap.
    pub fn with_capacity(max_async_children: usize) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            max_async_children: max_async_children.max(1),
            records: Mutex::new(HashMap::new()),
            completion_tx: tx,
            completion_rx: Mutex::new(rx),
        }
    }

    /// Number of async delegations currently running.
    pub fn active_count(&self) -> usize {
        let records = self.records.lock().unwrap_or_else(|e| e.into_inner());
        records
            .values()
            .filter(|r| r.status == DelegationStatus::Running)
            .count()
    }

    /// Dispatch a background delegation.
    ///
    /// `make_runner` receives a clone of the delegation's [`InterruptController`]
    /// so it can wire up a watcher that forwards `interrupt_all()` signals to
    /// the child agent. The returned future is spawned on a tokio task and
    /// this method returns immediately with a [`DispatchOutcome::Dispatched`]
    /// handle.
    ///
    /// Returns [`DispatchOutcome::Rejected`] when at capacity.
    pub fn dispatch<F, Fut>(
        self: &Arc<Self>,
        params: DispatchParams,
        make_runner: F,
    ) -> DispatchOutcome
    where
        F: FnOnce(InterruptController) -> Fut + Send + 'static,
        Fut: Future<Output = BackgroundDelegationResult> + Send + 'static,
    {
        let delegation_id = format!(
            "deleg_{}",
            &uuid::Uuid::new_v4().to_string()[..8]
        );
        let interrupt = InterruptController::new();
        let dispatched_at = Instant::now();
        let dispatched_ts = chrono::Utc::now().timestamp() as f64;

        // Capacity check + record insert under one lock hold — checking
        // active_count() separately would let two concurrent dispatches both
        // pass the check and exceed the cap.
        {
            let mut records = self.records.lock().unwrap_or_else(|e| e.into_inner());
            let running = records
                .values()
                .filter(|r| r.status == DelegationStatus::Running)
                .count();
            if running >= self.max_async_children {
                return DispatchOutcome::Rejected {
                    error: format!(
                        "Async delegation capacity reached ({} running, cap {}). \
                         Wait for one to finish (its result will re-enter the chat), \
                         or run this task synchronously (background=false).",
                        running, self.max_async_children
                    ),
                };
            }
            records.insert(
                delegation_id.clone(),
                DelegationRecord {
                    delegation_id: delegation_id.clone(),
                    goal: params.goal.clone(),
                    context: params.context.clone(),
                    toolset: params.toolset.clone(),
                    model: params.model.clone(),
                    session_key: params.session_key.clone(),
                    status: DelegationStatus::Running,
                    dispatched_at,
                    dispatched_ts,
                    completed_at: None,
                    interrupt: interrupt.clone(),
                },
            );
        }

        let registry = Arc::clone(self);
        let did = delegation_id.clone();
        let runner_interrupt = interrupt.clone();

        tokio::spawn(async move {
            let result = make_runner(runner_interrupt).await;
            registry.finalize(&did, result);
        });

        info!(
            delegation_id = %delegation_id,
            session_key = %params.session_key,
            goal = %(&params.goal[..params.goal.len().min(80)]),
            "Dispatched async delegation"
        );

        DispatchOutcome::Dispatched { delegation_id }
    }

    /// Mark a record complete and push the completion event onto the queue.
    fn finalize(
        &self,
        delegation_id: &str,
        result: BackgroundDelegationResult,
    ) {
        let event = {
            let mut records = self.records.lock().unwrap_or_else(|e| e.into_inner());
            let Some(record) = records.get_mut(delegation_id) else {
                return;
            };
            let completed_ts = chrono::Utc::now().timestamp() as f64;
            let duration = record.dispatched_at.elapsed().as_secs_f64();

            let status = match result.status.as_str() {
                "completed" | "success" => DelegationStatus::Completed,
                "cancelled" | "interrupted" => DelegationStatus::Interrupted,
                _ => DelegationStatus::Error,
            };
            record.status = status;
            record.completed_at = Some(Instant::now());

            let event = AsyncDelegationEvent {
                delegation_id: record.delegation_id.clone(),
                session_key: record.session_key.clone(),
                goal: record.goal.clone(),
                context: record.context.clone(),
                toolset: record.toolset.clone(),
                model: record.model.clone(),
                status: match status {
                    DelegationStatus::Completed => "completed",
                    DelegationStatus::Error => "error",
                    DelegationStatus::Interrupted => "interrupted",
                    DelegationStatus::Running => "completed", // shouldn't happen
                }
                .to_string(),
                summary: result.summary,
                error: result.error,
                total_turns: result.total_turns,
                api_calls: result.api_calls,
                prompt_tokens: result.usage.as_ref().map(|u| u.prompt_tokens),
                completion_tokens: result.usage.as_ref().map(|u| u.completion_tokens),
                estimated_cost_usd: result.usage.as_ref().and_then(|u| u.estimated_cost),
                sub_agent_id: Some(result.sub_agent_id),
                duration_seconds: (duration * 100.0).round() / 100.0,
                dispatched_at: record.dispatched_ts,
                completed_at: completed_ts,
            };

            // Prune old completed records beyond the retention cap.
            prune_completed_locked(&mut records);

            event
        };

        // Push the completion event (best-effort — if the channel is closed
        // the result is silently lost, but that only happens on shutdown).
        if let Err(e) = self.completion_tx.send(event) {
            warn!(
                error = %e,
                "Failed to enqueue async delegation completion event; result lost"
            );
        }
    }

    /// Try to receive a completion event (non-blocking).
    ///
    /// Returns `None` if no event is available. The CLI / gateway should poll
    /// this between turns.
    pub fn try_recv_event(&self) -> Option<AsyncDelegationEvent> {
        let mut rx = self.completion_rx.lock().unwrap_or_else(|e| e.into_inner());
        rx.try_recv().ok()
    }

    /// Signal every running async delegation to stop. Returns how many were
    /// signalled.
    ///
    /// Used on `/stop` and shutdown so a dangling background subagent can't
    /// keep burning tokens. The child still emits a completion event
    /// (status='interrupted') via the normal finalize path.
    pub fn interrupt_all(&self, reason: &str) -> usize {
        let targets: Vec<InterruptController> = {
            let records = self.records.lock().unwrap_or_else(|e| e.into_inner());
            records
                .values()
                .filter(|r| r.status == DelegationStatus::Running)
                .map(|r| r.interrupt.clone())
                .collect()
        };
        let count = targets.len();
        for interrupt in targets {
            interrupt.interrupt(Some(reason.to_string()));
        }
        if count > 0 {
            info!("Interrupted {} async delegation(s) ({})", count, reason);
        }
        count
    }

    /// Snapshot of async delegations (running + recently completed) for status
    /// queries (e.g. the `/agents` command in the TUI).
    pub fn list_delegations(&self) -> Vec<DelegationInfo> {
        let records = self.records.lock().unwrap_or_else(|e| e.into_inner());
        let mut infos: Vec<DelegationInfo> = records
            .values()
            .map(|r| DelegationInfo {
                delegation_id: r.delegation_id.clone(),
                goal: r.goal.clone(),
                context: r.context.clone(),
                toolset: r.toolset.clone(),
                model: r.model.clone(),
                session_key: r.session_key.clone(),
                status: match r.status {
                    DelegationStatus::Running => "running",
                    DelegationStatus::Completed => "completed",
                    DelegationStatus::Error => "error",
                    DelegationStatus::Interrupted => "interrupted",
                }
                .to_string(),
                dispatched_at: r.dispatched_ts,
                completed_at: r.completed_at.map(|_| {
                    // We don't store the completion timestamp separately in the
                    // record's Instant, but the event carries it. Use now as
                    // a fallback — this field is informational only.
                    chrono::Utc::now().timestamp() as f64
                }),
            })
            .collect();
        // Sort by dispatch time descending (most recent first).
        infos.sort_by(|a, b| b.dispatched_at.partial_cmp(&a.dispatched_at).unwrap_or(std::cmp::Ordering::Equal));
        infos
    }
}

impl Default for AsyncDelegationRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Drop the oldest completed records beyond the retention cap.
///
/// Caller must hold the records lock.
fn prune_completed_locked(records: &mut HashMap<String, DelegationRecord>) {
    let completed: Vec<(String, f64)> = records
        .iter()
        .filter(|(_, r)| r.status != DelegationStatus::Running)
        .map(|(id, r)| (id.clone(), r.dispatched_ts))
        .collect();
    if completed.len() <= MAX_RETAINED_COMPLETED {
        return;
    }
    // Sort oldest-first by dispatch time.
    let mut sorted = completed;
    sorted.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    let to_remove = sorted.len().saturating_sub(MAX_RETAINED_COMPLETED);
    for (id, _) in sorted.into_iter().take(to_remove) {
        records.remove(&id);
    }
}

/// Format a completion event into a rich, self-contained re-injection block.
///
/// When the result re-enters the conversation the parent may be deep in
/// unrelated context and won't remember why the subagent existed; the block
/// is written to stand entirely on its own — enough to use the result OR
/// re-dispatch if the world has moved on.
pub fn format_async_delegation_notification(evt: &AsyncDelegationEvent) -> String {
    let deleg_id = &evt.delegation_id;
    let goal = &evt.goal;
    let status = evt.status.as_str();
    let summary = evt.summary.as_deref();
    let error = evt.error.as_deref();
    let api_calls = evt.api_calls;
    let duration = evt.duration_seconds;
    let model = evt.model.as_deref().unwrap_or("?");

    let mut lines = vec![
        format!("[ASYNC DELEGATION COMPLETE — {}]", deleg_id),
        "A background subagent you dispatched earlier has finished. You may \
         have moved on since dispatching it; the full task source is below so \
         you can act on the result or re-dispatch if things have changed."
            .to_string(),
        String::new(),
        format!("Original goal: {}", goal),
    ];

    if let Some(ctx) = &evt.context {
        lines.push(format!("Context you provided: {}", ctx));
    }
    if let Some(ts) = &evt.toolset {
        lines.push(format!("Toolset: {}", ts));
    }
    lines.push(format!("Model: {}", model));
    lines.push(format!(
        "Status: {}   API calls: {}   Duration: {:.1}s",
        status, api_calls, duration
    ));
    if let Some(turns) = (evt.total_turns > 0).then_some(evt.total_turns) {
        lines.push(format!("Total turns: {}", turns));
    }
    if let Some(cost) = evt.estimated_cost_usd {
        lines.push(format!("Estimated cost: ${:.4}", cost));
    }

    lines.push("--- RESULT ---".to_string());
    match status {
        "completed" | "success" => {
            if let Some(s) = summary {
                lines.push(s.to_string());
            } else {
                lines.push("(no summary returned)".to_string());
            }
        }
        "interrupted" => {
            let mut msg = "The subagent was interrupted before completing".to_string();
            if let Some(e) = error {
                msg.push_str(&format!(": {}", e));
            } else {
                msg.push('.');
            }
            lines.push(msg);
            if let Some(s) = summary {
                lines.push("Partial output:".to_string());
                lines.push(s.to_string());
            }
        }
        _ => {
            let mut msg = format!(
                "The subagent did not complete successfully (status={}).",
                status
            );
            if let Some(e) = error {
                msg.push_str(&format!("\n{}", e));
            }
            lines.push(msg);
            if let Some(s) = summary {
                lines.push("Partial output:".to_string());
                lines.push(s.to_string());
            }
        }
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn make_params(goal: &str) -> DispatchParams {
        DispatchParams {
            goal: goal.to_string(),
            context: None,
            toolset: None,
            model: Some("test-model".to_string()),
            session_key: String::new(),
        }
    }

    fn ok_runner(sub_agent_id: &str, summary: &str) -> impl FnOnce(InterruptController) -> std::pin::Pin<Box<dyn Future<Output = BackgroundDelegationResult> + Send>> {
        let sub_agent_id = sub_agent_id.to_string();
        let summary = summary.to_string();
        move |_interrupt: InterruptController| {
            Box::pin(async move {
                // Simulate some work.
                tokio::time::sleep(Duration::from_millis(10)).await;
                BackgroundDelegationResult {
                    status: "completed".to_string(),
                    summary: Some(summary),
                    error: None,
                    total_turns: 3,
                    api_calls: 2,
                    usage: None,
                    sub_agent_id,
                }
            })
        }
    }

    fn err_runner(sub_agent_id: &str, error: &str) -> impl FnOnce(InterruptController) -> std::pin::Pin<Box<dyn Future<Output = BackgroundDelegationResult> + Send>> {
        let sub_agent_id = sub_agent_id.to_string();
        let error = error.to_string();
        move |_interrupt: InterruptController| {
            Box::pin(async move {
                BackgroundDelegationResult {
                    status: "error".to_string(),
                    summary: None,
                    error: Some(error),
                    total_turns: 0,
                    api_calls: 0,
                    usage: None,
                    sub_agent_id,
                }
            })
        }
    }

    #[tokio::test]
    async fn dispatch_returns_immediately_without_blocking() {
        let registry = Arc::new(AsyncDelegationRegistry::new());
        let t0 = Instant::now();
        let outcome = registry.dispatch(
            make_params("test task"),
            ok_runner("sub-1", "done"),
        );
        let elapsed = t0.elapsed();

        assert!(matches!(outcome, DispatchOutcome::Dispatched { .. }));
        // Non-blocking: dispatch should return well before the 10ms runner finishes.
        assert!(elapsed < Duration::from_millis(50));
        assert_eq!(registry.active_count(), 1);

        // Wait for the runner to complete.
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(registry.active_count(), 0);
    }

    #[tokio::test]
    async fn completion_event_lands_on_queue() {
        let registry = Arc::new(AsyncDelegationRegistry::new());
        registry.dispatch(
            make_params("compute X"),
            ok_runner("sub-2", "the result"),
        );

        // Wait for completion.
        let evt = wait_for_event(&registry, Duration::from_secs(2)).await;
        let evt = evt.expect("should receive a completion event");
        assert_eq!(evt.status, "completed");
        assert_eq!(evt.summary.as_deref(), Some("the result"));
        assert_eq!(evt.goal, "compute X");
        assert_eq!(evt.sub_agent_id.as_deref(), Some("sub-2"));
    }

    #[tokio::test]
    async fn crashed_runner_produces_error_completion() {
        let registry = Arc::new(AsyncDelegationRegistry::new());
        registry.dispatch(
            make_params("risky"),
            err_runner("sub-3", "subagent exploded"),
        );

        let evt = wait_for_event(&registry, Duration::from_secs(2)).await;
        let evt = evt.expect("should receive a completion event");
        assert_eq!(evt.status, "error");
        assert_eq!(evt.error.as_deref(), Some("subagent exploded"));
    }

    #[tokio::test]
    async fn dispatch_rejected_at_capacity() {
        let registry = Arc::new(AsyncDelegationRegistry::with_capacity(1));

        // First dispatch succeeds.
        let slow = move |_interrupt: InterruptController| {
            Box::pin(async move {
                tokio::time::sleep(Duration::from_millis(200)).await;
                BackgroundDelegationResult {
                    status: "completed".to_string(),
                    summary: Some("ok".to_string()),
                    error: None,
                    total_turns: 1,
                    api_calls: 1,
                    usage: None,
                    sub_agent_id: "slow-sub".to_string(),
                }
            })
        };
        let outcome1 = registry.dispatch(make_params("task1"), slow);
        assert!(matches!(outcome1, DispatchOutcome::Dispatched { .. }));

        // Second dispatch should be rejected (capacity=1, one running).
        let fast = move |_interrupt: InterruptController| {
            Box::pin(async move {
                BackgroundDelegationResult {
                    status: "completed".to_string(),
                    summary: Some("fast".to_string()),
                    error: None,
                    total_turns: 1,
                    api_calls: 1,
                    usage: None,
                    sub_agent_id: "fast-sub".to_string(),
                }
            })
        };
        let outcome2 = registry.dispatch(make_params("task2"), fast);
        assert!(matches!(outcome2, DispatchOutcome::Rejected { .. }));
    }

    #[tokio::test]
    async fn interrupt_all_signals_running_children() {
        let registry = Arc::new(AsyncDelegationRegistry::new());

        // We can't easily test the actual interrupt propagation here (it
        // depends on the orchestrator wiring), but we can verify interrupt_all
        // doesn't panic and returns the right count.
        let slow = move |_interrupt: InterruptController| {
            Box::pin(async move {
                tokio::time::sleep(Duration::from_millis(300)).await;
                BackgroundDelegationResult {
                    status: "completed".to_string(),
                    summary: Some("ok".to_string()),
                    error: None,
                    total_turns: 1,
                    api_calls: 1,
                    usage: None,
                    sub_agent_id: "slow".to_string(),
                }
            })
        };
        registry.dispatch(make_params("long task"), slow);

        assert_eq!(registry.active_count(), 1);
        let n = registry.interrupt_all("test");
        assert_eq!(n, 1);

        // Let it finish.
        tokio::time::sleep(Duration::from_millis(400)).await;
    }

    #[test]
    fn notification_format_is_self_contained() {
        let evt = AsyncDelegationEvent {
            delegation_id: "deleg_test01".to_string(),
            session_key: String::new(),
            goal: "Investigate flaky test".to_string(),
            context: Some("repo /tmp/p".to_string()),
            toolset: Some("terminal".to_string()),
            model: Some("test-model".to_string()),
            status: "completed".to_string(),
            summary: Some("Found the bug in test_foo".to_string()),
            error: None,
            total_turns: 4,
            api_calls: 3,
            prompt_tokens: None,
            completion_tokens: None,
            estimated_cost_usd: Some(0.05),
            sub_agent_id: Some("sub-abc".to_string()),
            duration_seconds: 12.0,
            dispatched_at: 1000.0,
            completed_at: 1012.0,
        };
        let text = format_async_delegation_notification(&evt);
        for needle in [
            "ASYNC DELEGATION COMPLETE",
            "Investigate flaky test",
            "repo /tmp/p",
            "terminal",
            "Found the bug in test_foo",
            "completed",
        ] {
            assert!(text.contains(needle), "missing {:?} in notification", needle);
        }
    }

    #[test]
    fn notification_error_includes_error_message() {
        let evt = AsyncDelegationEvent {
            delegation_id: "deleg_err".to_string(),
            session_key: String::new(),
            goal: "risky task".to_string(),
            context: None,
            toolset: None,
            model: None,
            status: "error".to_string(),
            summary: None,
            error: Some("subagent exploded".to_string()),
            total_turns: 0,
            api_calls: 0,
            prompt_tokens: None,
            completion_tokens: None,
            estimated_cost_usd: None,
            sub_agent_id: Some("sub-err".to_string()),
            duration_seconds: 0.5,
            dispatched_at: 1000.0,
            completed_at: 1000.5,
        };
        let text = format_async_delegation_notification(&evt);
        assert!(text.contains("did not complete successfully"));
        assert!(text.contains("subagent exploded"));
    }

    // --- helpers ---

    async fn wait_for_event(
        registry: &AsyncDelegationRegistry,
        timeout: Duration,
    ) -> Option<AsyncDelegationEvent> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Some(evt) = registry.try_recv_event() {
                return Some(evt);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        None
    }
}
