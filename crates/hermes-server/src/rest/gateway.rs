use axum::{
    extract::State,
    Json,
};
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::{
    error::{ok_json, AppError},
    state::AppState,
};

/// Global gateway state.
/// In a single-process model, the gateway is always "running" when the server is up.
/// These endpoints provide compatibility with the Python API.
static GATEWAY_RUNNING: AtomicBool = AtomicBool::new(true);

/// POST /api/gateway/start - Start the gateway.
/// 
/// In the Rust implementation, the gateway is always running as part of hermes-server.
/// This endpoint returns success if already running.
pub async fn start_gateway(
    _state: State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    if GATEWAY_RUNNING.load(Ordering::SeqCst) {
        Ok(ok_json(json!({
            "status": "already_running",
            "message": "Gateway is already running",
        })))
    } else {
        GATEWAY_RUNNING.store(true, Ordering::SeqCst);
        Ok(ok_json(json!({
            "status": "started",
            "message": "Gateway started",
        })))
    }
}

/// POST /api/gateway/stop - Stop the gateway.
/// 
/// Sets the gateway state to stopped. The actual process termination
/// should be handled by the process manager (e.g., Desktop or systemd).
pub async fn stop_gateway(
    _state: State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    GATEWAY_RUNNING.store(false, Ordering::SeqCst);
    Ok(ok_json(json!({
        "status": "stopped",
        "message": "Gateway stop requested. The process will exit shortly.",
    })))
}

/// POST /api/gateway/restart - Restart the gateway.
/// 
/// In the Rust implementation, restart requires the process manager
/// (e.g., Desktop or systemd) to kill and respawn the process.
pub async fn restart_gateway(
    _state: State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(ok_json(json!({
        "status": "restart_requested",
        "message": "Gateway restart requested. Please restart the hermes-ultra process.",
    })))
}

/// Check if the gateway is currently marked as running.
pub fn is_gateway_running() -> bool {
    GATEWAY_RUNNING.load(Ordering::SeqCst)
}
