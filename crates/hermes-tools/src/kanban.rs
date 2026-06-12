//! Kanban worker tools — parity with Python `kanban_*` tools.

use serde_json::{Value, json};

pub const KANBAN_TASK_ENV: &str = "HERMES_KANBAN_TASK";

pub fn kanban_task_from_env() -> Option<String> {
    std::env::var(KANBAN_TASK_ENV)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn kanban_block_reason(blocked_on: Option<&str>) -> Value {
    json!({
        "status": "blocked",
        "blocked_on": blocked_on.unwrap_or("iteration_budget_exhausted"),
        "task": kanban_task_from_env(),
    })
}

pub fn kanban_next_payload() -> Value {
    json!({
        "status": "next",
        "task": kanban_task_from_env(),
    })
}

pub fn kanban_complete_payload(summary: &str) -> Value {
    json!({
        "status": "complete",
        "summary": summary,
        "task": kanban_task_from_env(),
    })
}
