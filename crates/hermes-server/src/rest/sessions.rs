use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::{
    error::{ok_json, AppError},
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct ListSessionsQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub min_messages: Option<i64>,
    pub order: Option<String>,
    /// Response format: "legacy" for object wrapping, default is array
    pub format: Option<String>,
    /// Archived filter: "exclude" (default), "include", or "only"
    pub archived: Option<String>,
}

/// GET /api/sessions - List sessions
pub async fn list_sessions(
    State(state): State<AppState>,
    Query(query): Query<ListSessionsQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let persistence = state.session_persistence()?;
    
    // Ensure DB exists
    persistence.ensure_db()?;
    
    let limit = query.limit.unwrap_or(20);
    let offset = query.offset.unwrap_or(0);
    let min_messages = query.min_messages.unwrap_or(0);
    let order_by_last_active = query.order.as_deref() == Some("recent");
    
    let sessions = tokio::task::spawn_blocking(move || {
        persistence.list_sessions_rich(None, &[], limit, offset, min_messages, order_by_last_active)
    })
    .await
    .map_err(|e| AppError::Internal(format!("task error: {}", e)))?
    .map_err(|e| AppError::Internal(format!("db error: {}", e)))?;
    
    // Filter by archived status
    let archived_filter = query.archived.as_deref().unwrap_or("exclude");
    let sessions: Vec<_> = sessions.into_iter().filter(|s| {
        match archived_filter {
            "exclude" => !s.archived,
            "only" => s.archived,
            _ => true, // "include" or any other value
        }
    }).collect();
    
    let session_json: Vec<serde_json::Value> = sessions
        .into_iter()
        .map(|s| {
            json!({
                "id": s.id,
                "source": s.source,
                "model": s.model,
                "title": s.title,
                "started_at": s.started_at,
                "last_active": s.last_active,
                "ended_at": s.ended_at,
                "message_count": s.message_count,
                "parent_session_id": s.parent_session_id,
                "preview": s.preview,
                "archived": s.archived,
            })
        })
        .collect();
    
    // Check if legacy format is requested
    if query.format.as_deref() == Some("legacy") {
        Ok(ok_json(json!({
            "sessions": session_json,
            "total": session_json.len(),
            "limit": limit,
            "offset": offset,
        })))
    } else {
        // Desktop expects array format by default
        Ok(ok_json(json!(session_json)))
    }
}

/// GET /api/sessions/search - Search sessions
#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
    pub limit: Option<usize>,
}

pub async fn search_sessions(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let persistence = state.session_persistence()?;
    persistence.ensure_db()?;
    
    let limit = query.limit.unwrap_or(20);
    let q = query.q.clone();
    
    let results = tokio::task::spawn_blocking(move || {
        persistence.search_messages(&q,
            None, // source_filter
            None, // exclude_sources
            None, // role_filter
            limit,
            0,    // offset
            None, // sort
        )
    })
    .await
    .map_err(|e| AppError::Internal(format!("task error: {}", e)))?
    .map_err(|e| AppError::Internal(format!("db error: {}", e)))?;
    
    let results_json: Vec<serde_json::Value> = results
        .into_iter()
        .map(|r| {
            json!({
                "id": r.id,
                "session_id": r.session_id,
                "role": r.role,
                "snippet": r.snippet,
                "timestamp": r.timestamp,
                "tool_name": r.tool_name,
                "source": r.source,
                "model": r.model,
            })
        })
        .collect();
    
    Ok(ok_json(json!({ "results": results_json })))
}

/// GET /api/sessions/:id - Get session details
pub async fn get_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let persistence = state.session_persistence()?;
    persistence.ensure_db()?;
    
    let sid = session_id.clone();
    let session = tokio::task::spawn_blocking(move || {
        persistence.get_session(&sid)
    })
    .await
    .map_err(|e| AppError::Internal(format!("task error: {}", e)))?
    .map_err(|e| AppError::Internal(format!("db error: {}", e)))?;
    
    match session {
        Some(s) => Ok(ok_json(json!({
            "id": s.id,
            "source": s.source,
            "model": s.model,
            "title": s.title,
            "started_at": s.started_at,
            "last_active": s.last_active,
            "ended_at": s.ended_at,
            "message_count": s.message_count,
            "parent_session_id": s.parent_session_id,
            "system_prompt": s.system_prompt,
            "preview": s.preview,
        }))),
        None => Err(AppError::NotFound(format!("session {} not found", session_id))),
    }
}

/// GET /api/sessions/:id/messages - Get session messages
pub async fn get_session_messages(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let persistence = state.session_persistence()?;
    persistence.ensure_db()?;
    
    let sid = session_id.clone();
    let messages = tokio::task::spawn_blocking(move || {
        persistence.load_session(&sid)
    })
    .await
    .map_err(|e| AppError::Internal(format!("task error: {}", e)))?
    .map_err(|e| AppError::Internal(format!("db error: {}", e)))?;
    
    let messages_json: Vec<serde_json::Value> = messages
        .into_iter()
        .map(|m| {
            json!({
                "role": m.role,
                "content": m.content,
            })
        })
        .collect();
    
    Ok(ok_json(json!({
        "count": messages_json.len(),
        "messages": messages_json,
    })))
}

/// DELETE /api/sessions/:id - End a session
pub async fn delete_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let persistence = state.session_persistence()?;
    persistence.ensure_db()?;
    
    let sid = session_id.clone();
    tokio::task::spawn_blocking(move || {
        persistence.end_session(&sid, "user_deleted")
    })
    .await
    .map_err(|e| AppError::Internal(format!("task error: {}", e)))?
    .map_err(|e| AppError::Internal(format!("db error: {}", e)))?;
    
    Ok(ok_json(json!({ "deleted": session_id })))
}

/// PATCH /api/sessions/:id - Rename/update session
#[derive(Debug, Deserialize)]
pub struct UpdateSession {
    pub title: Option<String>,
    pub archived: Option<bool>,
}

pub async fn update_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(update): Json<UpdateSession>,
) -> Result<Json<serde_json::Value>, AppError> {
    let persistence = state.session_persistence()?;
    persistence.ensure_db()?;
    
    if let Some(title) = update.title {
        let sid = session_id.clone();
        tokio::task::spawn_blocking(move || {
            persistence.set_session_title(&sid, Some(&title))
        })
        .await
        .map_err(|e| AppError::Internal(format!("task error: {}", e)))?
        .map_err(|e| AppError::Internal(format!("db error: {}", e)))?;
    }
    
    if let Some(archived) = update.archived {
        let sid = session_id.clone();
        let persistence2 = state.session_persistence()?;
        persistence2.ensure_db()?;
        tokio::task::spawn_blocking(move || {
            persistence2.archive_session(&sid, archived)
        })
        .await
        .map_err(|e| AppError::Internal(format!("task error: {}", e)))?
        .map_err(|e| AppError::Internal(format!("db error: {}", e)))?;
    }
    
    Ok(ok_json(json!({ "ok": true })))
}

/// GET /api/sessions/:id/export - Export session as JSON
pub async fn export_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let persistence = state.session_persistence()?;
    persistence.ensure_db()?;
    
    let sid = session_id.clone();
    let session = tokio::task::spawn_blocking(move || {
        persistence.get_session(&sid)
    })
    .await
    .map_err(|e| AppError::Internal(format!("task error: {}", e)))?
    .map_err(|e| AppError::Internal(format!("db error: {}", e)))?;
    
    let persistence2 = state.session_persistence()?;
    let messages = tokio::task::spawn_blocking(move || {
        persistence2.load_session(&session_id)
    })
    .await
    .map_err(|e| AppError::Internal(format!("task error: {}", e)))?
    .map_err(|e| AppError::Internal(format!("db error: {}", e)))?;
    
    let export = json!({
        "session": session,
        "messages": messages,
    });
    
    Ok(ok_json(export))
}

/// POST /api/sessions/prune - Prune old sessions
#[derive(Debug, Deserialize)]
pub struct PruneSessions {
    pub older_than_days: Option<u32>,
}

pub async fn prune_sessions(
    State(state): State<AppState>,
    Json(prune): Json<PruneSessions>,
) -> Result<Json<serde_json::Value>, AppError> {
    let persistence = state.session_persistence()?;
    persistence.ensure_db()?;
    
    let days = prune.older_than_days.unwrap_or(90);
    
    let count = tokio::task::spawn_blocking(move || {
        persistence.prune_sessions(days)
    })
    .await
    .map_err(|e| AppError::Internal(format!("task error: {}", e)))?
    .map_err(|e| AppError::Internal(format!("db error: {}", e)))?;
    
    Ok(ok_json(json!({ "pruned": count })))
}

/// GET /api/sessions/stats - Session statistics
pub async fn session_stats(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let persistence = state.session_persistence()?;
    persistence.ensure_db()?;
    
    let (total, today, week) = tokio::task::spawn_blocking(move || {
        persistence.session_stats()
    })
    .await
    .map_err(|e| AppError::Internal(format!("task error: {}", e)))?
    .map_err(|e| AppError::Internal(format!("db error: {}", e)))?;
    
    Ok(ok_json(json!({
        "total": total,
        "today": today,
        "week": week,
    })))
}

/// GET /api/sessions/empty/count - Count empty sessions
pub async fn count_empty_sessions(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let persistence = state.session_persistence()?;
    persistence.ensure_db()?;
    
    let count = tokio::task::spawn_blocking(move || {
        persistence.count_empty_sessions()
    })
    .await
    .map_err(|e| AppError::Internal(format!("task error: {}", e)))?
    .map_err(|e| AppError::Internal(format!("db error: {}", e)))?;
    
    Ok(ok_json(json!({ "count": count })))
}

/// DELETE /api/sessions/empty - Delete empty sessions
pub async fn delete_empty_sessions(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let persistence = state.session_persistence()?;
    persistence.ensure_db()?;
    
    let count = tokio::task::spawn_blocking(move || {
        persistence.delete_empty_sessions()
    })
    .await
    .map_err(|e| AppError::Internal(format!("task error: {}", e)))?
    .map_err(|e| AppError::Internal(format!("db error: {}", e)))?;
    
    Ok(ok_json(json!({ "deleted": count })))
}

/// POST /api/sessions/bulk-delete - Bulk delete sessions
#[derive(Debug, Deserialize)]
pub struct BulkDelete {
    pub session_ids: Vec<String>,
}

pub async fn bulk_delete_sessions(
    State(state): State<AppState>,
    Json(body): Json<BulkDelete>,
) -> Result<Json<serde_json::Value>, AppError> {
    let persistence = state.session_persistence()?;
    persistence.ensure_db()?;
    
    let ids = body.session_ids;
    let count = tokio::task::spawn_blocking(move || {
        persistence.bulk_delete_sessions(&ids)
    })
    .await
    .map_err(|e| AppError::Internal(format!("task error: {}", e)))?
    .map_err(|e| AppError::Internal(format!("db error: {}", e)))?;
    
    Ok(ok_json(json!({ "deleted": count })))
}
