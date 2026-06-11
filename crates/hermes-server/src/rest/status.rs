use axum::{
    extract::State,
    Json,
};
use serde_json::json;

use crate::{
    error::{ok_json, AppError},
    state::AppState,
};

/// GET /api/status - Service status
pub async fn handler(State(state): State<AppState>) -> Result<Json<serde_json::Value>, AppError> {
    let _config = state.config.read().await;
    
    Ok(ok_json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "release_date": "2026-06-10",
        "hermes_home": state.hermes_home.to_string_lossy(),
        "config_path": state.config_path().to_string_lossy(),
        "env_path": state.env_path().to_string_lossy(),
        "gateway_running": true,
        "gateway_pid": std::process::id(),
        "active_sessions": 0,
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
