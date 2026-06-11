use std::collections::HashMap;
use std::sync::Arc;
use serde_json::json;
use tokio::sync::{RwLock, oneshot};

use crate::{
    ws::rpc::{JsonRpcError, JsonRpcRequest, JsonRpcResponse},
};

/// Global pending interactions table.
/// Maps request_id to oneshot sender for the response.
pub type PendingInteractions = Arc<RwLock<HashMap<String, oneshot::Sender<String>>>>;

/// approval.respond - Respond to an approval request.
pub async fn handle_approval_respond(
    request: JsonRpcRequest,
    pending: &PendingInteractions,
) -> Option<JsonRpcResponse> {
    let params = request.params.as_ref()?.as_object()?;
    let request_id = params.get("request_id")?.as_str()?;
    let choice = params.get("choice")?.as_str()?;
    
    let sender = {
        let mut guard = pending.write().await;
        guard.remove(request_id)
    };
    
    match sender {
        Some(tx) => {
            let _ = tx.send(choice.to_string());
            Some(JsonRpcResponse::ok(request.id, json!({"ok": true})))
        }
        None => Some(JsonRpcResponse::err(
            request.id,
            JsonRpcError::server_error(4007, "request not found or already handled".to_string()),
        )),
    }
}

/// clarify.respond - Respond to a clarify request.
pub async fn handle_clarify_respond(
    request: JsonRpcRequest,
    pending: &PendingInteractions,
) -> Option<JsonRpcResponse> {
    handle_approval_respond(request, pending).await
}

/// sudo.respond - Respond to a sudo request.
pub async fn handle_sudo_respond(
    request: JsonRpcRequest,
    pending: &PendingInteractions,
) -> Option<JsonRpcResponse> {
    handle_approval_respond(request, pending).await
}

/// secret.respond - Respond to a secret request.
pub async fn handle_secret_respond(
    request: JsonRpcRequest,
    pending: &PendingInteractions,
) -> Option<JsonRpcResponse> {
    handle_approval_respond(request, pending).await
}

/// Create a new empty pending interactions table.
pub fn new_pending_interactions() -> PendingInteractions {
    Arc::new(RwLock::new(HashMap::new()))
}
