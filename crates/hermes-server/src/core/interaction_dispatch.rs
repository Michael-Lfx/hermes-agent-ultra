//! Async tool dispatch with interaction blocking for Desktop approval/clarify/sudo/secret.

use std::sync::Arc;
use std::time::Duration;

use hermes_agent::AgentLoop;
use hermes_core::ToolError;
use serde_json::{json, Value};

use crate::{
    rpc::interaction::PendingInteractions,
    ws::{
        rpc::JsonRpcEvent,
        transport::{ReplaceableTransport, Transport},
    },
};

/// Create an async tool dispatch that intercepts interactive tools and blocks
/// waiting for user response via the Desktop WebSocket.
///
/// Interactive tools (clarify, approval, sudo, secret) will:
/// 1. Send a JSON-RPC event to the Desktop client
/// 2. Insert a oneshot sender into the pending interactions table
/// 3. Await the user's response (with 60-second timeout)
///
/// Non-interactive tools are dispatched through the full hermes_tools registry
/// (all 83 builtin tools with real implementations).
pub fn create_interaction_dispatch(
    transport: ReplaceableTransport,
    pending: PendingInteractions,
    tools_registry: Arc<hermes_tools::ToolRegistry>,
    agent_registry: Arc<hermes_agent::ToolRegistry>,
) -> hermes_agent::AsyncToolDispatch {
    Arc::new(move |tool_name: String, params: Value| {
        let transport = transport.clone();
        let pending = pending.clone();
        let full_registry = Arc::clone(&tools_registry);
        let agent_reg = Arc::clone(&agent_registry);
        Box::pin(async move {
            match tool_name.as_str() {
                "clarify" => handle_clarify(params, transport, pending).await,
                "approval" => handle_approval(params, transport, pending).await,
                "sudo" => handle_sudo(params, transport, pending).await,
                "secret" => handle_secret(params, transport, pending).await,
                _ => {
                    // Dispatch through the full hermes_tools registry (all builtin tools)
                    // dispatch_async returns a JSON string directly (errors embedded as {"error":...})
                    let result = full_registry.dispatch_async(&tool_name, params.clone()).await;
                    // Check if result looks like an error
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&result) {
                        if val.get("error").is_some() {
                            // Error from hermes_tools — check if tool wasn't found
                            let error_msg = val["error"].as_str().unwrap_or("");
                            if error_msg.contains("not found") || error_msg.contains("Unknown") {
                                // Fallback to agent registry sync handler
                                if let Some(entry) = agent_reg.get(&tool_name) {
                                    let handler = Arc::clone(&entry.handler);
                                    match tokio::task::spawn_blocking(move || handler(params)).await {
                                        Ok(r) => r,
                                        Err(e) => Err(ToolError::ExecutionFailed(format!(
                                            "Tool dispatch failed: {}", e
                                        ))),
                                    }
                                } else {
                                    Err(ToolError::ExecutionFailed(format!(
                                        "Unknown tool '{}'", tool_name
                                    )))
                                }
                            } else {
                                Err(ToolError::ExecutionFailed(error_msg.to_string()))
                            }
                        } else {
                            Ok(result)
                        }
                    } else {
                        Ok(result)
                    }
                }
            }
        })
    })
}

/// Handle a clarify interaction: ask the user a multiple-choice question.
async fn handle_clarify(
    params: Value,
    transport: ReplaceableTransport,
    pending: PendingInteractions,
) -> Result<String, ToolError> {
    let interaction_id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = tokio::sync::oneshot::channel::<String>();

    // Insert into pending table
    {
        let mut guard = pending.write().await;
        guard.insert(interaction_id.clone(), tx);
    }

    // Send event to Desktop
    let event = JsonRpcEvent::new(
        crate::ws::events::types::CLARIFY_REQUEST,
        None,
        Some(json!({
            "interaction_id": interaction_id,
            "question": params.get("question").cloned().unwrap_or(Value::Null),
            "choices": params.get("choices").cloned().unwrap_or(Value::Null),
        })),
    );
    if let Ok(val) = serde_json::to_value(&event) {
        let _ = transport.write(&val);
    }

    // Wait for user response (60s timeout)
    match tokio::time::timeout(Duration::from_secs(60), rx).await {
        Ok(Ok(response)) => Ok(response),
        Ok(Err(_)) => {
            tracing::warn!("clarify interaction cancelled");
            Err(ToolError::ExecutionFailed("User cancelled".to_string()))
        }
        Err(_) => {
            tracing::warn!("clarify interaction timed out — auto-selecting first choice");
            let mut guard = pending.write().await;
            guard.remove(&interaction_id);
            // Auto-select first choice on timeout so agent can continue
            let first_choice = params
                .get("choices")
                .and_then(|c| c.as_array())
                .and_then(|arr| arr.first())
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Ok(first_choice)
        }
    }
}

/// Handle an approval interaction: ask the user to approve a tool execution.
async fn handle_approval(
    params: Value,
    transport: ReplaceableTransport,
    pending: PendingInteractions,
) -> Result<String, ToolError> {
    let interaction_id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = tokio::sync::oneshot::channel::<String>();

    // Insert into pending table
    {
        let mut guard = pending.write().await;
        guard.insert(interaction_id.clone(), tx);
    }

    // Send event to Desktop
    let event = JsonRpcEvent::new(
        crate::ws::events::types::APPROVAL_REQUEST,
        None,
        Some(json!({
            "interaction_id": interaction_id,
            "tool": params.get("tool").cloned().unwrap_or(Value::Null),
            "message": params.get("message").cloned().unwrap_or(json!("The agent wants to execute a tool. Approve?")),
        })),
    );
    if let Ok(val) = serde_json::to_value(&event) {
        let _ = transport.write(&val);
    }

    // Wait for user response (60s timeout)
    match tokio::time::timeout(Duration::from_secs(60), rx).await {
        Ok(Ok(response)) => Ok(response),
        Ok(Err(_)) => {
            tracing::warn!("approval interaction cancelled");
            Err(ToolError::ExecutionFailed("User cancelled".to_string()))
        }
        Err(_) => {
            tracing::warn!("approval interaction timed out — auto-deny");
            let mut guard = pending.write().await;
            guard.remove(&interaction_id);
            // Auto-deny on timeout so agent can continue without the tool
            Ok("deny".to_string())
        }
    }
}

/// Handle a sudo interaction: ask the user for sudo password.
async fn handle_sudo(
    params: Value,
    transport: ReplaceableTransport,
    pending: PendingInteractions,
) -> Result<String, ToolError> {
    let interaction_id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = tokio::sync::oneshot::channel::<String>();

    // Insert into pending table
    {
        let mut guard = pending.write().await;
        guard.insert(interaction_id.clone(), tx);
    }

    // Send event to Desktop
    let event = JsonRpcEvent::new(
        crate::ws::events::types::SUDO_REQUEST,
        None,
        Some(json!({
            "interaction_id": interaction_id,
            "message": params.get("message").cloned().unwrap_or(json!("Sudo password required")),
        })),
    );
    if let Ok(val) = serde_json::to_value(&event) {
        let _ = transport.write(&val);
    }

    // Wait for user response (60s timeout)
    match tokio::time::timeout(Duration::from_secs(60), rx).await {
        Ok(Ok(response)) => Ok(response),
        Ok(Err(_)) => {
            tracing::warn!("sudo interaction cancelled");
            Err(ToolError::ExecutionFailed("User cancelled".to_string()))
        }
        Err(_) => {
            tracing::warn!("sudo interaction timed out — returning empty password");
            let mut guard = pending.write().await;
            guard.remove(&interaction_id);
            // Return empty string so tool fails gracefully but agent continues
            Ok("".to_string())
        }
    }
}

/// Handle a secret interaction: ask the user for a secret/key.
async fn handle_secret(
    params: Value,
    transport: ReplaceableTransport,
    pending: PendingInteractions,
) -> Result<String, ToolError> {
    let interaction_id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = tokio::sync::oneshot::channel::<String>();

    // Insert into pending table
    {
        let mut guard = pending.write().await;
        guard.insert(interaction_id.clone(), tx);
    }

    // Send event to Desktop
    let event = JsonRpcEvent::new(
        crate::ws::events::types::SECRET_REQUEST,
        None,
        Some(json!({
            "interaction_id": interaction_id,
            "key_name": params.get("key_name").cloned().unwrap_or(Value::Null),
            "message": params.get("message").cloned().unwrap_or(json!("Secret required")),
        })),
    );
    if let Ok(val) = serde_json::to_value(&event) {
        let _ = transport.write(&val);
    }

    // Wait for user response (60s timeout)
    match tokio::time::timeout(Duration::from_secs(60), rx).await {
        Ok(Ok(response)) => Ok(response),
        Ok(Err(_)) => {
            tracing::warn!("secret interaction cancelled");
            Err(ToolError::ExecutionFailed("User cancelled".to_string()))
        }
        Err(_) => {
            tracing::warn!("secret interaction timed out — returning empty secret");
            let mut guard = pending.write().await;
            guard.remove(&interaction_id);
            // Return empty string so tool fails gracefully but agent continues
            Ok("".to_string())
        }
    }
}
