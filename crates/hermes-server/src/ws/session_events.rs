//! Session event helpers for emitting `session.info` and related events.

use serde_json::json;

use crate::{
    core::session::SessionState,
    ws::{
        events::types::{SESSION_INFO, STATUS_UPDATE},
        rpc::JsonRpcEvent,
        transport::Transport,
    },
};

/// Emit a `session.info` event to the Desktop client.
///
/// This is the most frequently emitted event in the Python backend.
/// Desktop uses it to update the sidebar with session metadata.
pub fn emit_session_info(
    transport: &crate::ws::transport::ReplaceableTransport,
    session: &SessionState,
    message_count: i64,
    title: Option<String>,
    model: Option<String>,
) {
    let event = JsonRpcEvent::new(
        SESSION_INFO,
        Some(session.session_id.clone()),
        Some(json!({
            "session_id": session.session_id,
            "session_key": session.session_key,
            "message_count": message_count,
            "title": title,
            "model": model,
            "cwd": session.cwd.to_string_lossy(),
            "cols": session.cols,
            "running": session.is_running(),
            "branch": false,
            "lazy": true,
            "desktop_contract": 2,
            "profile_name": session.profile,
            "tools": [],
            "skills": [],
            "close_on_disconnect": session.close_on_disconnect,
        })),
    );

    if let Ok(val) = serde_json::to_value(&event) {
        let _ = transport.write(&val);
    }
}

/// Emit a `session.info` event with default values (0 messages, no title).
/// Useful when creating a new session.
pub fn emit_session_info_default(
    transport: &crate::ws::transport::ReplaceableTransport,
    session: &SessionState,
) {
    emit_session_info(transport, session, 0, None, None);
}

/// Emit a `status.update` event to the Desktop client.
///
/// Desktop uses this to show the current agent status in the status bar
/// (e.g. "Thinking...", "Running tool: xyz", or ready state).
pub fn emit_status_update(
    transport: &crate::ws::transport::ReplaceableTransport,
    session: &SessionState,
    kind: &str,
    text: &str,
) {
    let event = JsonRpcEvent::new(
        STATUS_UPDATE,
        Some(session.session_id.clone()),
        Some(json!({
            "kind": kind,
            "text": text,
        })),
    );

    if let Ok(val) = serde_json::to_value(&event) {
        let _ = transport.write(&val);
    }
}
