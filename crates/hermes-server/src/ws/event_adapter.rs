use hermes_core::StreamChunk;
use serde_json::json;

use crate::ws::rpc::JsonRpcEvent;

/// Convert a StreamChunk to zero or more JSON-RPC server-push events.
///
/// `session_id` is attached to every event so the frontend can route by session.
/// `start_sent` tracks whether `message.start` has been emitted for this turn
/// (to avoid sending it on every chunk).
/// `accumulated_text` — if provided, included in `message.complete` payload so
/// the frontend sees the full response text at completion time.
///
/// Note: tool.start/complete and thinking.delta events are pushed via
/// AgentCallbacks bound in agent_builder.rs, not through the stream callback.
pub fn stream_chunk_to_events(
    chunk: &StreamChunk,
    session_id: Option<String>,
    start_sent: &std::sync::atomic::AtomicBool,
    accumulated_text: Option<&str>,
) -> Vec<JsonRpcEvent> {
    let mut events = Vec::new();

    // message.start — only on the first text-chunk of a turn
    if !start_sent.load(std::sync::atomic::Ordering::SeqCst)
        && chunk.delta.is_some()
        && chunk.finish_reason.is_none()
    {
        start_sent.store(true, std::sync::atomic::Ordering::SeqCst);
        events.push(JsonRpcEvent::new(
            crate::ws::events::types::MESSAGE_START,
            session_id.clone(),
            Some(json!({})),
        ));
    }

    // message.delta — SKIPPED here; on_stream_delta callback in agent_builder.rs
    // emits MESSAGE_DELTA directly so text arrives even without a StreamChunk wrapper.

    // message.complete
    if let Some(ref reason) = chunk.finish_reason {
        let status = match reason.as_str() {
            "stop" | "end_turn" => "complete",
            "length" | "max_tokens" => "interrupted",
            "content_filter" | "error" => "error",
            _ => "complete",
        };
        let mut payload = json!({ "status": status });
        if let Some(text) = accumulated_text {
            payload["text"] = json!(text);
        }
        events.push(JsonRpcEvent::new(
            crate::ws::events::types::MESSAGE_COMPLETE,
            session_id,
            Some(payload),
        ));
    }

    events
}

/// Build a generic error event.
pub fn error_event(message: &str) -> JsonRpcEvent {
    JsonRpcEvent::new(
        crate::ws::events::types::ERROR,
        None,
        Some(json!({ "message": message })),
    )
}
