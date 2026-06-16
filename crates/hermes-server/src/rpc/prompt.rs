use std::sync::{Arc, Mutex};

use hermes_agent::conversation_loop::RunConversationParams;
use hermes_core::StreamChunk;
use serde_json::json;

use crate::{
    state::AppState,
    ws::rpc::{JsonRpcError, JsonRpcRequest, JsonRpcResponse},
    ws::session_events,
    ws::transport::Transport,
};

/// prompt.submit - Submit a user message and start an agent turn.
pub async fn handle_submit(
    request: JsonRpcRequest,
    state: &AppState,
) -> Option<JsonRpcResponse> {
    let params = request.params.as_ref()?.as_object()?;
    let session_id = params.get("session_id")?.as_str()?;
    let text = params.get("text")?.as_str()?;
    
    // Get session and check state
    let (transport, agent, final_message) = {
        let sessions = state.sessions.read().await;
        let session = match sessions.get(session_id) {
            Some(s) => s,
            None => {
                return Some(JsonRpcResponse::err(
                    request.id,
                    JsonRpcError::server_error(4001, "no active session".into()),
                ));
            }
        };
        
        if session.is_running() {
            return Some(JsonRpcResponse::err(
                request.id,
                JsonRpcError::server_error(4002, "session already running".into()),
            ));
        }
        
        let config = state.config.read().await.clone();
        let agent = match session.get_or_build_agent(&config, &state.hermes_home, &state.pending_interactions, state.tool_registry.clone(), state.tools_registry.clone()) {
            Some(a) => a,
            None => {
                return Some(JsonRpcResponse::err(
                    request.id,
                    JsonRpcError::internal_error("failed to build agent".into()),
                ));
            }
        };
        
        session.set_running(true);
        
        // Emit session.info to show "running" status in Desktop sidebar
        session_events::emit_session_info(
            &session.transport,
            session,
            0, // message count will be updated after completion
            None,
            None,
        );
        
        // Emit status.update to show "Thinking..." in Desktop status bar
        session_events::emit_status_update(
            &session.transport,
            session,
            "processing",
            "Thinking...",
        );
        
        // Collect any pending steer texts before spawning
        let steer_texts = session.drain_steer();
        let final_message = if steer_texts.is_empty() {
            text.to_string()
        } else {
            format!("{}\n\n[User guidance: {}]", text, steer_texts.join("; "))
        };
        
        (session.transport.clone(), agent, final_message)
    };
    
    // Resolve @file: references in the message
    let resolved_message = crate::rpc::file_resolver::resolve_file_refs(
        &final_message,
        session_id,
        &state.hermes_home,
    )
    .await;

    // Load conversation history from DB
    let history = match crate::core::agent_builder::load_history(session_id, &state.hermes_home) {
        Ok(msgs) => msgs,
        Err(e) => {
            tracing::warn!("Failed to load history for session {}: {}", session_id, e);
            Vec::new()
        }
    };

    // Pass tool schemas so the LLM knows which tools are available
    let tool_schemas = agent.tool_registry.schemas().to_vec();

    // Spawn the agent turn in a background task so we can return immediately
    let session_id_owned = session_id.to_string();
    let text_owned = resolved_message;
    let transport_err = transport.clone();
    let sessions_arc = state.sessions.clone();
    tokio::spawn(async move {
        let session_id_cb = session_id_owned.clone();
        let start_sent = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let accumulated = Arc::new(Mutex::new(String::new()));
        let acc = accumulated.clone();
        let stream_callback: Box<dyn Fn(StreamChunk) + Send + Sync> = Box::new({
            let start_sent = start_sent.clone();
            move |chunk| {
                if let Some(ref delta) = chunk.delta {
                    if let Some(ref content) = delta.content {
                        if let Ok(mut g) = acc.lock() {
                            g.push_str(content);
                        }
                    }
                }
                let acc_text = acc
                    .lock()
                    .ok()
                    .map(|g| g.clone())
                    .unwrap_or_default();
                for event in crate::ws::event_adapter::stream_chunk_to_events(
                    &chunk,
                    Some(session_id_cb.clone()),
                    &start_sent,
                    chunk.finish_reason.is_some().then_some(acc_text.as_str()),
                ) {
                    let _ = transport.write(&serde_json::to_value(event).unwrap_or_default());
                }
            }
        });
        
        let params = RunConversationParams {
            user_message: text_owned,
            conversation_history: history,
            task_id: None,
            stream_callback: Some(stream_callback),
            persist_user_message: None,
            tools: Some(tool_schemas.clone()),
            persist_session: true,
        };
        
        let result = agent.run_conversation(params).await;
        
        // Always mark session as no longer running
        {
            let sessions = sessions_arc.read().await;
            if let Some(session) = sessions.get(&session_id_owned) {
                session.set_running(false);
                
                // Emit session.info to update Desktop sidebar (turn completed)
                session_events::emit_session_info(
                    &session.transport,
                    session,
                    0, // TODO: query actual message count from DB
                    None,
                    None,
                );
                
                // Emit status.update to clear "Thinking..." in Desktop status bar
                session_events::emit_status_update(
                    &session.transport,
                    session,
                    "ready",
                    "",
                );
            }
        }
        
        match result {
            Ok(_) => {
                tracing::info!("Agent turn completed for session {}", session_id_owned);
            }
            Err(e) => {
                tracing::error!("Agent turn failed for session {}: {}", session_id_owned, e);
                let err_event = crate::ws::event_adapter::error_event(&format!("Agent error: {}", e));
                let _ = transport_err.write(&serde_json::to_value(err_event).unwrap_or_default());
            }
        }
    });
    
    // Return immediately to client
    let result = json!({
        "status": "started",
        "session_id": session_id,
    });
    
    Some(JsonRpcResponse::ok(request.id, result))
}

/// session.steer - Send real-time guidance to an ongoing agent turn.
///
/// Params: `{ session_id, text }`
/// Returns: `{ accepted: true }`
pub async fn handle_steer(
    request: JsonRpcRequest,
    state: &AppState,
) -> Option<JsonRpcResponse> {
    let params = request.params.as_ref()?.as_object()?;
    let session_id = params.get("session_id")?.as_str()?;
    let text = params.get("text")?.as_str()?;

    let sessions = state.sessions.read().await;
    let session = match sessions.get(session_id) {
        Some(s) => s,
        None => {
            return Some(JsonRpcResponse::err(
                request.id,
                JsonRpcError::server_error(4001, "no active session".into()),
            ));
        }
    };

    if !session.is_running() {
        return Some(JsonRpcResponse::err(
            request.id,
            JsonRpcError::server_error(4003, "session not running".into()),
        ));
    }

    session.push_steer(text.to_string());

    Some(JsonRpcResponse::ok(request.id, json!({ "accepted": true })))
}

/// prompt.background - Submit a background task (placeholder for Phase 4).
pub async fn handle_background(
    request: JsonRpcRequest,
    _state: &AppState,
) -> Option<JsonRpcResponse> {
    let result = json!({
        "task_id": "placeholder",
        "status": "queued",
    });
    
    Some(JsonRpcResponse::ok(request.id, result))
}
