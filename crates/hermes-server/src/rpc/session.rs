use serde_json::json;
use std::path::PathBuf;

use chrono::{Datelike, Timelike};
use uuid::Uuid;

use crate::{
    core::session::SessionState,
    state::AppState,
    ws::{
        rpc::{JsonRpcError, JsonRpcRequest, JsonRpcResponse},
        transport::{NullTransport, ReplaceableTransport, Transport},
    },
};

/// session.create - Create a new session.
pub async fn handle_create(
    request: JsonRpcRequest,
    state: &AppState,
) -> Option<JsonRpcResponse> {
    let params = request.params.as_ref()?.as_object()?;
    
    let session_id = Uuid::new_v4().to_string()[..8].to_string();
    let now = chrono::Local::now();
    let session_key = format!(
        "{:04}{:02}{:02}_{:02}{:02}{:02}_{}",
        now.year(),
        now.month(),
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
        Uuid::new_v4().to_string()[..6].to_string()
    );
    
    let cwd = params
        .get("cwd")
        .and_then(|v| v.as_str())
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    
    let cols = params
        .get("cols")
        .and_then(|v| v.as_u64())
        .unwrap_or(120) as u32;
    
    let close_on_disconnect = params
        .get("close_on_disconnect")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    
    // Create session in DB
    let persistence = match state.session_persistence() {
        Ok(p) => p,
        Err(e) => {
            return Some(JsonRpcResponse::err(
                request.id,
                JsonRpcError::internal_error(format!("db error: {}", e)),
            ));
        }
    };
    
    if let Err(e) = persistence.ensure_db() {
        return Some(JsonRpcResponse::err(
            request.id,
            JsonRpcError::internal_error(format!("db error: {}", e)),
        ));
    }
    
    let source = "desktop";
    let sid = session_id.clone();
    let _ = tokio::task::spawn_blocking(move || {
        let _ = persistence.create_session(&sid, source, None);
    })
    .await;
    
    // Create session state (no transport yet - will be bound on WS connect)
    let transport = ReplaceableTransport::new(NullTransport);
    let session_state = SessionState::new(
        session_id.clone(),
        session_key.clone(),
        transport,
        cwd.clone(),
        cols,
    );
    
    {
        let mut sessions = state.sessions.write().await;
        sessions.insert(session_id.clone(), session_state);
    }
    
    // Emit session.info event for Desktop sidebar update
    {
        let sessions = state.sessions.read().await;
        if let Some(session) = sessions.get(&session_id) {
            crate::ws::session_events::emit_session_info_default(
                &session.transport,
                session,
            );
        }
    }
    
    // Return session info
    let result = json!({
        "session_id": session_id,
        "session_key": session_key,
        "message_count": 0,
        "messages": [],
        "info": {
            "model": null,
            "tools": [],
            "skills": [],
            "cwd": cwd.to_string_lossy(),
            "branch": false,
            "lazy": true,
            "desktop_contract": 2,
            "profile_name": null,
        }
    });
    
    Some(JsonRpcResponse::ok(request.id, result))
}

/// session.list - List active sessions.
pub async fn handle_list(
    request: JsonRpcRequest,
    state: &AppState,
) -> Option<JsonRpcResponse> {
    let sessions = state.sessions.read().await;
    
    let session_list: Vec<serde_json::Value> = sessions
        .values()
        .map(|s| {
            json!({
                "id": s.session_id,
                "session_key": s.session_key,
                "cwd": s.cwd.to_string_lossy(),
                "cols": s.cols,
                "running": s.is_running(),
            })
        })
        .collect();
    
    let result = json!({
        "sessions": session_list,
    });
    
    Some(JsonRpcResponse::ok(request.id, result))
}

/// session.resume - Resume an existing session.
pub async fn handle_resume(
    request: JsonRpcRequest,
    state: &AppState,
) -> Option<JsonRpcResponse> {
    let params = request.params.as_ref()?.as_object()?;
    let session_id = params.get("session_id")?.as_str()?;
    
    // Check if session is already active
    {
        let sessions = state.sessions.read().await;
        if let Some(session) = sessions.get(session_id) {
            // Already active, return info
            let result = json!({
                "session_id": session_id,
                "resumed": true,
                "message_count": 0,
                "messages": [],
                "info": {
                    "model": null,
                    "tools": [],
                    "skills": [],
                    "cwd": session.cwd.to_string_lossy(),
                }
            });
            return Some(JsonRpcResponse::ok(request.id, result));
        }
    }
    
    // Try to load from DB
    let persistence = state.session_persistence().ok()?;
    persistence.ensure_db().ok()?;
    
    let sid = session_id.to_string();
    let db_session = match tokio::task::spawn_blocking(move || {
        persistence.get_session(&sid)
    })
    .await
    .ok()
    .and_then(|r| r.ok())
    .flatten() {
        Some(s) => s,
        None => {
            return Some(JsonRpcResponse::err(
                request.id,
                JsonRpcError::server_error(4007, format!("session {} not found", session_id)),
            ));
        }
    };
    
    // Create in-memory session
    let transport = ReplaceableTransport::new(NullTransport);
    let session_state = SessionState::new(
        session_id.to_string(),
        db_session.id.clone(),
        transport,
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        120,
    );
    
    {
        let mut sessions = state.sessions.write().await;
        sessions.insert(session_id.to_string(), session_state);
    }
    
    // Emit session.info event for Desktop sidebar update
    {
        let sessions = state.sessions.read().await;
        if let Some(session) = sessions.get(session_id) {
            crate::ws::session_events::emit_session_info(
                &session.transport,
                session,
                db_session.message_count as i64,
                None,
                db_session.model.clone(),
            );
        }
    }
    
    let result = json!({
        "session_id": session_id,
        "resumed": true,
        "message_count": db_session.message_count,
        "messages": [],
        "info": {
            "model": db_session.model,
            "tools": [],
            "skills": [],
            "cwd": std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).to_string_lossy(),
        }
    });
    
    Some(JsonRpcResponse::ok(request.id, result))
}

/// session.close - Close a session.
pub async fn handle_close(
    request: JsonRpcRequest,
    state: &AppState,
) -> Option<JsonRpcResponse> {
    let params = request.params.as_ref()?.as_object()?;
    let session_id = params.get("session_id")?.as_str()?;
    
    {
        let mut sessions = state.sessions.write().await;
        if let Some(session) = sessions.remove(session_id) {
            session.transport.close();
        }
    }
    
    // End in DB
    let persistence = state.session_persistence().ok()?;
    let sid = session_id.to_string();
    tokio::task::spawn_blocking(move || {
        persistence.end_session(&sid, "user_closed")
    })
    .await
    .ok();
    
    let result = json!({ "closed": true });
    Some(JsonRpcResponse::ok(request.id, result))
}

/// session.history - Get session message history.
pub async fn handle_history(
    request: JsonRpcRequest,
    state: &AppState,
) -> Option<JsonRpcResponse> {
    let params = request.params.as_ref()?.as_object()?;
    let session_id = params.get("session_id")?.as_str()?;
    
    let persistence = state.session_persistence().ok()?;
    persistence.ensure_db().ok()?;
    
    let sid = session_id.to_string();
    let messages = match tokio::task::spawn_blocking(move || {
        persistence.load_session(&sid)
    })
    .await
    .ok()
    .and_then(|r| r.ok()) {
        Some(msgs) => msgs,
        None => {
            return Some(JsonRpcResponse::err(
                request.id,
                JsonRpcError::server_error(4007, format!("session {} not found", session_id)),
            ));
        }
    };
    
    let messages_json: Vec<serde_json::Value> = messages
        .into_iter()
        .map(|m| {
            json!({
                "role": m.role,
                "content": m.content,
            })
        })
        .collect();
    
    let result = json!({
        "count": messages_json.len(),
        "messages": messages_json,
    });
    
    Some(JsonRpcResponse::ok(request.id, result))
}

/// session.interrupt - Interrupt a running session.
pub async fn handle_interrupt(
    request: JsonRpcRequest,
    state: &AppState,
) -> Option<JsonRpcResponse> {
    let params = request.params.as_ref()?.as_object()?;
    let session_id = params.get("session_id")?.as_str()?;
    
    {
        let sessions = state.sessions.read().await;
        if let Some(session) = sessions.get(session_id) {
            session.set_running(false);
            crate::ws::session_events::emit_status_update(
                &session.transport,
                session,
                "ready",
                "Interrupted",
            );
        }
    }
    
    let result = json!({ "status": "interrupted" });
    Some(JsonRpcResponse::ok(request.id, result))
}

/// session.title - Get/set session title.
pub async fn handle_title(
    request: JsonRpcRequest,
    state: &AppState,
) -> Option<JsonRpcResponse> {
    let params = request.params.as_ref()?.as_object()?;
    let session_id = params.get("session_id")?.as_str()?;
    
    // If title provided, set it
    if let Some(title) = params.get("title").and_then(|v| v.as_str()) {
        let persistence = state.session_persistence().ok()?;
        let sid = session_id.to_string();
        let t = title.to_string();
        tokio::task::spawn_blocking(move || {
            persistence.set_session_title(&sid, Some(&t))
        })
        .await
        .ok();
    }
    
    // Get current title
    let persistence = state.session_persistence().ok()?;
    let sid = session_id.to_string();
    let title = tokio::task::spawn_blocking(move || {
        persistence.get_session_title(&sid)
    })
    .await
    .ok()
    .and_then(|r| r.ok())
    .flatten();
    
    // Emit session.info with updated title
    {
        let sessions = state.sessions.read().await;
        if let Some(session) = sessions.get(session_id) {
            crate::ws::session_events::emit_session_info(
                &session.transport,
                session,
                0, // message count not critical for title update
                title.clone(),
                None,
            );
        }
    }
    
    let result = json!({
        "title": title,
        "session_id": session_id,
    });
    
    Some(JsonRpcResponse::ok(request.id, result))
}

/// session.usage - Get session token usage.
pub async fn handle_usage(
    request: JsonRpcRequest,
    _state: &AppState,
) -> Option<JsonRpcResponse> {
    let result = json!({
        "input_tokens": 0,
        "output_tokens": 0,
        "total_tokens": 0,
        "calls": 0,
    });
    
    Some(JsonRpcResponse::ok(request.id, result))
}

/// session.delete - Delete a session from DB.
pub async fn handle_delete(
    request: JsonRpcRequest,
    state: &AppState,
) -> Option<JsonRpcResponse> {
    let params = request.params.as_ref()?.as_object()?;
    let session_id = params.get("session_id")?.as_str()?;
    
    // Remove from active sessions
    {
        let mut sessions = state.sessions.write().await;
        sessions.remove(session_id);
    }
    
    // End in DB (soft delete)
    let persistence = state.session_persistence().ok()?;
    let sid = session_id.to_string();
    tokio::task::spawn_blocking(move || {
        persistence.end_session(&sid, "user_deleted")
    })
    .await
    .ok();
    
    let result = json!({ "deleted": session_id });
    Some(JsonRpcResponse::ok(request.id, result))
}
