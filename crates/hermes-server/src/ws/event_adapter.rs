use hermes_core::StreamChunk;
use serde_json::json;

use crate::ws::rpc::JsonRpcEvent;

/// Convert a StreamChunk to zero or more JSON-RPC server-push events.
///
/// Note: tool.start/complete and thinking.delta events are pushed via
/// AgentCallbacks bound in agent_builder.rs, not through the stream callback.
pub fn stream_chunk_to_events(chunk: &StreamChunk) -> Vec<JsonRpcEvent> {
    let mut events = Vec::new();

    // message.start (first text delta of a new message)
    if chunk.delta.is_some() && chunk.finish_reason.is_none() {
        events.push(JsonRpcEvent::new(
            crate::ws::events::types::MESSAGE_START,
            None,
            Some(json!({})),
        ));
    }

    // message.delta (text content)
    if let Some(ref delta) = chunk.delta {
        if let Some(ref content) = delta.content {
            events.push(JsonRpcEvent::new(
                crate::ws::events::types::MESSAGE_DELTA,
                None,
                Some(json!({
                    "content": content,
                })),
            ));
        }
    }

    // message.complete
    if let Some(ref reason) = chunk.finish_reason {
        events.push(JsonRpcEvent::new(
            crate::ws::events::types::MESSAGE_COMPLETE,
            None,
            Some(json!({
                "reason": reason,
            })),
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
