use serde_json::json;

use crate::{
    state::{AppState, HandoffState},
    ws::rpc::{JsonRpcError, JsonRpcRequest, JsonRpcResponse},
};

/// handoff.request - Request transferring a session to a messaging platform.
///
/// Params: `{ session_id, platform }`
/// Returns: `{ ok: true, state: "pending" }`
pub async fn handle_handoff_request(
    request: JsonRpcRequest,
    state: &AppState,
) -> Option<JsonRpcResponse> {
    let params = request.params.as_ref()?.as_object()?;
    let session_id = params.get("session_id")?.as_str()?;
    let platform = params.get("platform")?.as_str()?;

    let handoff = HandoffState {
        state: "pending".to_string(),
        platform: platform.to_string(),
        started_at: chrono::Utc::now().timestamp() as f64,
        completed_at: None,
        error: None,
    };

    {
        let mut states = state.handoff_states.write().await;
        states.insert(session_id.to_string(), handoff.clone());
    }

    // TODO: Start async task to actually perform the handoff via messaging platform APIs.
    // For now, simulate success after a short delay (to be implemented in Phase 6).
    let states_clone = state.handoff_states.clone();
    let sid = session_id.to_string();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        let mut states = states_clone.write().await;
        if let Some(h) = states.get_mut(&sid) {
            h.state = "completed".to_string();
            h.completed_at = Some(chrono::Utc::now().timestamp() as f64);
        }
    });

    Some(JsonRpcResponse::ok(request.id, json!({
        "ok": true,
        "state": "pending",
    })))
}

/// handoff.state - Query the current handoff state for a session.
///
/// Params: `{ session_id }`
/// Returns: `{ state, platform, error? }`
pub async fn handle_handoff_state(
    request: JsonRpcRequest,
    state: &AppState,
) -> Option<JsonRpcResponse> {
    let params = request.params.as_ref()?.as_object()?;
    let session_id = params.get("session_id")?.as_str()?;

    let states = state.handoff_states.read().await;
    let handoff = match states.get(session_id) {
        Some(h) => h,
        None => {
            return Some(JsonRpcResponse::err(
                request.id,
                JsonRpcError::server_error(4004, "No handoff found for session".into()),
            ));
        }
    };

    Some(JsonRpcResponse::ok(request.id, json!({
        "state": handoff.state,
        "platform": handoff.platform,
        "error": handoff.error,
    })))
}

/// handoff.fail - Mark a handoff as failed.
///
/// Params: `{ session_id, error }`
/// Returns: `{ ok: true }`
pub async fn handle_handoff_fail(
    request: JsonRpcRequest,
    state: &AppState,
) -> Option<JsonRpcResponse> {
    let params = request.params.as_ref()?.as_object()?;
    let session_id = params.get("session_id")?.as_str()?;
    let error = params.get("error").and_then(|v| v.as_str()).unwrap_or("unknown");

    {
        let mut states = state.handoff_states.write().await;
        if let Some(handoff) = states.get_mut(session_id) {
            handoff.state = "failed".to_string();
            handoff.error = Some(error.to_string());
        }
    }

    Some(JsonRpcResponse::ok(request.id, json!({ "ok": true })))
}
