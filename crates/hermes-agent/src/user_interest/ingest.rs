//! Shared POI ingestion helpers (agent loop + memory plugin).

use std::sync::{Arc, Mutex};

use hermes_config::InterestConfig;
use serde_json::Value;

use super::extract::{
    extract_signals_from_messages, extract_signals_from_text, filter_poi_signals,
};
use super::llm::extract_signals_from_transcript_llm;
use super::store::InterestStore;

/// Agent-injected continuation / nudge user lines — not real user POI.
pub fn is_poi_synthetic_user_text(text: &str) -> bool {
    let t = text.trim();
    t.starts_with("[System:")
        && (t.contains("Continue now")
            || t.contains("incomplete due to generation limits")
            || t.contains("Continue exactly where you left off"))
}

/// Rule-based ingest from a single user message (per-turn).
pub fn ingest_user_message(
    store: &InterestStore,
    user_text: &str,
    weight_scale: f64,
) -> Result<(), String> {
    let trimmed = user_text.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let signals = filter_poi_signals(extract_signals_from_text(trimmed, weight_scale));
    if signals.is_empty() {
        return Ok(());
    }
    store.ingest_signals(&signals)
}

/// Session-end ingest: user-only rules + optional async LLM summary.
pub fn spawn_session_end_ingest(
    store: Arc<Mutex<InterestStore>>,
    config: InterestConfig,
    messages: Vec<Value>,
) {
    if !config.enabled {
        return;
    }
    tokio::spawn(async move {
        let mut all_signals = Vec::new();
        if config.uses_rules() {
            all_signals.extend(extract_signals_from_messages(&messages));
        }
        if config.uses_llm() && config.llm_on_session_end {
            let transcript = format_transcript(&messages);
            all_signals.extend(extract_signals_from_transcript_llm(&transcript).await);
        }
        let all_signals = filter_poi_signals(all_signals);
        if all_signals.is_empty() {
            return;
        }
        if let Ok(guard) = store.lock() {
            let _ = guard.apply_decay();
            let _ = guard.ingest_signals(&all_signals);
        }
    });
}

fn format_transcript(messages: &[Value]) -> String {
    let mut out = String::new();
    for msg in messages {
        let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("unknown");
        let content = message_text_value(msg);
        if content.trim().is_empty() {
            continue;
        }
        out.push_str(role);
        out.push_str(": ");
        out.push_str(content.trim());
        out.push_str("\n\n");
    }
    out
}

fn message_text_value(msg: &Value) -> String {
    if let Some(s) = msg.get("content").and_then(|v| v.as_str()) {
        return s.to_string();
    }
    if let Some(parts) = msg.get("content").and_then(|v| v.as_array()) {
        return parts
            .iter()
            .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join("");
    }
    String::new()
}
