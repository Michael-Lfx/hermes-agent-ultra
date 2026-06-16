use std::collections::HashMap;

use axum::{
    extract::State,
    Json,
};
use serde_json::{json, Value};

use crate::{
    error::{ok_json, AppError},
    state::AppState,
};

/// GET /api/status - Service status
pub async fn handler(State(state): State<AppState>) -> Result<Json<serde_json::Value>, AppError> {
    let active_sessions = state.sessions.read().await.len();
    
    let gateway_running = super::gateway::is_gateway_running(&state).await;
    let gateway_pid = {
        let guard = state.gateway_process.read().await;
        guard.as_ref().and_then(|c| c.id())
    };

    Ok(ok_json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "release_date": "2026-06-10",
        "hermes_home": state.hermes_home.to_string_lossy(),
        "config_path": state.config_path().to_string_lossy(),
        "env_path": state.env_path().to_string_lossy(),
        "config_version": 0,
        "latest_config_version": 0,
        "gateway_running": gateway_running,
        "gateway_pid": gateway_pid,
        "gateway_health_url": Value::Null,
        "gateway_state": if gateway_running { "running" } else { "stopped" },
        "gateway_platforms": json!({}),
        "gateway_exit_reason": Value::Null,
        "gateway_updated_at": Value::Null,
        "active_sessions": active_sessions,
        "auth_required": false,
    })))
}

/// GET /api/system/stats - System information
pub async fn system_stats(State(_state): State<AppState>) -> Result<Json<serde_json::Value>, AppError> {
    Ok(ok_json(json!({
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
    })))
}

/// GET /health - Simple health check
pub async fn health() -> Json<serde_json::Value> {
    ok_json(json!({
        "status": "ok",
        "timestamp": chrono::Utc::now().to_rfc3339(),
    }))
}
