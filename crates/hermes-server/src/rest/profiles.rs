use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;

use crate::{
    error::{ok_json, AppError},
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct ProfileSessionsQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub min_messages: Option<i64>,
    pub order: Option<String>,
    pub profile: Option<String>,
    pub source: Option<String>,
    pub exclude_sources: Option<String>,
    pub archived: Option<String>,
}

/// GET /api/profiles/sessions - Aggregate sessions across profiles
pub async fn get_profiles_sessions(
    State(state): State<AppState>,
    Query(query): Query<ProfileSessionsQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let limit = query.limit.unwrap_or(20);
    let offset = query.offset.unwrap_or(0);
    let min_messages = query.min_messages.unwrap_or(0);
    let order_by_last_active = query.order.as_deref() == Some("recent");
    let include_archived = query.archived.as_deref() == Some("include");
    let archived_only = query.archived.as_deref() == Some("only");
    
    // Determine which profiles to query
    let mut targets: Vec<(String, std::path::PathBuf)> = Vec::new();
    
    if let Some(ref profile_name) = query.profile {
        if profile_name != "all" {
            let home = state.profile_home(Some(profile_name));
            targets.push((profile_name.clone(), home));
        }
    }
    
    if targets.is_empty() {
        // Add default profile
        targets.push(("default".to_string(), state.hermes_home.clone()));
        
        // Scan profiles directory
        let profiles_dir = state.hermes_home.join("profiles");
        if let Ok(entries) = std::fs::read_dir(&profiles_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name != "default" && entry.path().is_dir() {
                    targets.push((name.clone(), entry.path()));
                }
            }
        }
    }
    
    let per_profile_limit = (limit + offset).max(limit).min(500);
    let mut merged: Vec<serde_json::Value> = Vec::new();
    let mut total = 0i64;
    let mut profile_totals: HashMap<String, i64> = HashMap::new();
    let mut errors: Vec<serde_json::Value> = Vec::new();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    
    for (name, home) in targets {
        let db_path = home.join("state.db");
        if !db_path.exists() {
            continue;
        }
        
        {
            let source_filter = query.source.clone();
            let exclude_list = query.exclude_sources.as_ref()
                .map(|s| s.split(',').map(|s| s.to_string()).collect::<Vec<_>>())
                .unwrap_or_default();
            
            match tokio::task::spawn_blocking({
                    let home = home.clone();
                    move || {
                        let persistence = hermes_agent::session_persistence::SessionPersistence::new(&home);
                        persistence.ensure_db()?;
                        let sessions = persistence.list_sessions_rich(
                            source_filter.as_deref(),
                            &exclude_list.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                            per_profile_limit,
                            0,
                            min_messages,
                            order_by_last_active,
                        )?;
                        let count = sessions.len() as i64;
                        Ok::<_, hermes_core::AgentError>((sessions, count))
                    }
                }).await {
                    Ok(Ok((sessions, count))) => {
                        total += count;
                        profile_totals.insert(name.clone(), count);
                        
                        for s in sessions {
                            let is_active = s.ended_at.is_none() && 
                                s.last_active.map(|t| now - (t as i64) < 300).unwrap_or(false);
                            
                            merged.push(json!({
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
                                "profile": name.clone(),
                                "is_default_profile": name == "default",
                                "is_active": is_active,
                                "archived": s.archived,
                            }));
                        }
                    }
                    Ok(Err(e)) => {
                        errors.push(json!({"profile": name, "error": e.to_string()}));
                    }
                    Err(e) => {
                        errors.push(json!({"profile": name, "error": format!("task: {}", e)}));
                    }
                }
            }
    }
    
    // Sort by last_active or started_at (descending)
    merged.sort_by(|a, b| {
        let a_val = a["last_active"].as_i64().or(a["started_at"].as_i64()).unwrap_or(0);
        let b_val = b["last_active"].as_i64().or(b["started_at"].as_i64()).unwrap_or(0);
        b_val.cmp(&a_val)
    });
    
    // Apply pagination
    let window: Vec<_> = merged.into_iter().skip(offset).take(limit).collect();
    
    Ok(ok_json(json!({
        "sessions": window,
        "total": total,
        "profile_totals": profile_totals,
        "limit": limit,
        "offset": offset,
        "errors": errors,
    })))
}

/// GET /api/profiles - List all profiles
pub async fn list_profiles(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let mut profiles = Vec::new();
    
    // Default profile is always present
    profiles.push(json!({
        "name": "default",
        "path": state.hermes_home.to_string_lossy(),
        "is_default": true,
    }));
    
    // Scan profiles directory
    let profiles_dir = state.hermes_home.join("profiles");
    if let Ok(entries) = std::fs::read_dir(&profiles_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name != "default" && entry.path().is_dir() {
                profiles.push(json!({
                    "name": name,
                    "path": entry.path().to_string_lossy(),
                    "is_default": false,
                }));
            }
        }
    }
    
    Ok(ok_json(json!(profiles)))
}

/// GET /api/profiles/active - Get active profile
pub async fn get_active_profile(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let profile = state.active_profile.read().await.clone();
    Ok(ok_json(json!({"profile": profile})))
}

#[derive(Debug, Deserialize)]
pub struct SetProfileRequest {
    pub profile: String,
}

/// POST /api/profiles/active - Set active profile
pub async fn set_active_profile(
    State(state): State<AppState>,
    Json(body): Json<SetProfileRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Validate profile name
    if !is_valid_profile_name(&body.profile) {
        return Err(AppError::BadRequest(format!(
            "Invalid profile name: {}",
            body.profile
        )));
    }
    
    // Ensure profile directory exists
    let profile_home = state.profile_home(Some(&body.profile));
    if !profile_home.exists() {
        std::fs::create_dir_all(&profile_home)
            .map_err(|e| AppError::Internal(format!("create profile dir: {}", e)))?;
    }
    
    // Update active profile
    *state.active_profile.write().await = body.profile.clone();
    
    // Invalidate agent caches
    state.invalidate_agent_caches().await;
    
    Ok(ok_json(json!({"ok": true, "profile": body.profile})))
}

fn is_valid_profile_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 64 {
        return false;
    }
    
    let first = name.chars().next().unwrap();
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return false;
    }
    
    name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

/// POST /api/profiles - Create a new profile
pub async fn create_profile(
    State(state): State<AppState>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    let name = payload["name"].as_str()
        .ok_or_else(|| AppError::BadRequest("Missing profile name".to_string()))?;
    
    if !is_valid_profile_name(name) {
        return Err(AppError::BadRequest(format!("Invalid profile name: {}", name)));
    }
    
    let profile_home = state.profile_home(Some(name));
    if profile_home.exists() {
        return Err(AppError::BadRequest(format!("Profile '{}' already exists", name)));
    }
    
    tokio::fs::create_dir_all(&profile_home).await
        .map_err(|e| AppError::Internal(format!("create profile dir: {}", e)))?;
    
    Ok(ok_json(json!({
        "status": "ok",
        "profile": name,
    })))
}

/// PATCH /api/profiles/{name} - Rename a profile
pub async fn rename_profile(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    let new_name = payload["name"].as_str()
        .ok_or_else(|| AppError::BadRequest("Missing new name".to_string()))?;
    
    if !is_valid_profile_name(new_name) {
        return Err(AppError::BadRequest(format!("Invalid profile name: {}", new_name)));
    }
    
    let old_path = state.profile_home(Some(&name));
    let new_path = state.profile_home(Some(new_name));
    
    if !old_path.exists() {
        return Err(AppError::NotFound(format!("Profile '{}' not found", name)));
    }
    
    if new_path.exists() {
        return Err(AppError::BadRequest(format!("Profile '{}' already exists", new_name)));
    }
    
    tokio::fs::rename(&old_path, &new_path).await
        .map_err(|e| AppError::Internal(format!("rename profile: {}", e)))?;
    
    Ok(ok_json(json!({
        "status": "ok",
        "old_name": name,
        "new_name": new_name,
    })))
}

/// DELETE /api/profiles/{name} - Delete a profile
pub async fn delete_profile(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    if name == "default" {
        return Err(AppError::BadRequest("Cannot delete default profile".to_string()));
    }
    
    let profile_path = state.profile_home(Some(&name));
    
    if !profile_path.exists() {
        return Err(AppError::NotFound(format!("Profile '{}' not found", name)));
    }
    
    tokio::fs::remove_dir_all(&profile_path).await
        .map_err(|e| AppError::Internal(format!("delete profile: {}", e)))?;
    
    Ok(ok_json(json!({
        "status": "ok",
        "profile": name,
    })))
}

/// GET /api/profiles/{name}/soul - Get profile soul configuration.
pub async fn get_profile_soul(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let soul_path = state.profile_home(Some(&name)).join("soul.md");
    let content = if soul_path.exists() {
        tokio::fs::read_to_string(&soul_path).await.unwrap_or_default()
    } else {
        String::new()
    };
    Ok(ok_json(json!({ "content": content })))
}

/// PUT /api/profiles/{name}/soul - Update profile soul configuration.
pub async fn put_profile_soul(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    let content = body.get("content").and_then(|v| v.as_str()).unwrap_or("");
    let soul_path = state.profile_home(Some(&name)).join("soul.md");
    tokio::fs::write(&soul_path, content).await
        .map_err(|e| AppError::Internal(format!("write soul: {}", e)))?;
    Ok(ok_json(json!({ "ok": true })))
}

/// GET /api/profiles/{name}/setup-command - Get profile setup command.
pub async fn get_profile_setup_command(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let setup_path = state.profile_home(Some(&name)).join(".setup_command");
    let command = if setup_path.exists() {
        tokio::fs::read_to_string(&setup_path).await.unwrap_or_default().trim().to_string()
    } else {
        String::new()
    };
    Ok(ok_json(json!({ "command": command })))
}
