use axum::{
    extract::State,
    Json,
};
use serde_json::json;
use tokio::sync::RwLock;

use crate::{
    error::{ok_json, AppError},
    state::AppState,
};

/// Find the hermes-agent-ultra binary path.
fn find_gateway_binary() -> Option<std::path::PathBuf> {
    // 1. Same directory as the current executable
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("hermes-agent-ultra.exe");
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    // 2. Look in PATH
    if let Ok(output) = std::process::Command::new("where")
        .arg("hermes-agent-ultra")
        .output()
    {
        if output.status.success() {
            let path_str = String::from_utf8_lossy(&output.stdout);
            if let Some(first_line) = path_str.lines().next() {
                let p = std::path::PathBuf::from(first_line.trim());
                if p.exists() {
                    return Some(p);
                }
            }
        }
    }
    None
}

/// POST /api/gateway/start - Start the gateway subprocess.
pub async fn start_gateway(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Check if already running
    {
        let guard = state.gateway_process.read().await;
        if guard.is_some() {
            return Ok(ok_json(json!({
                "status": "already_running",
                "message": "Gateway is already running",
            })));
        }
    }

    // Find the gateway binary
    let binary = find_gateway_binary()
        .ok_or_else(|| AppError::Internal("hermes-agent-ultra binary not found".to_string()))?;

    let profile = state.active_profile_name().await;
    let hermes_home = state.hermes_home.clone();

    let mut cmd = tokio::process::Command::new(&binary);
    cmd.arg("gateway").arg("run");
    if profile != "default" {
        cmd.arg("--profile").arg(&profile);
    }
    cmd.env("HERMES_HOME", &hermes_home);
    cmd.kill_on_drop(true);

    let child = cmd.spawn()
        .map_err(|e| AppError::Internal(format!("Failed to start gateway: {}", e)))?;

    let pid = child.id();
    tracing::info!(pid = pid, profile = %profile, "Gateway subprocess started");

    {
        let mut guard = state.gateway_process.write().await;
        *guard = Some(child);
    }

    Ok(ok_json(json!({
        "status": "started",
        "message": format!("Gateway started (PID: {:?})", pid),
    })))
}

/// POST /api/gateway/stop - Stop the gateway subprocess.
pub async fn stop_gateway(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let mut guard = state.gateway_process.write().await;
    if let Some(ref mut child) = *guard {
        let pid = child.id();
        tracing::info!(pid = pid, "Stopping gateway subprocess");

        // Send SIGTERM (on Windows, kill)
        child.kill().await
            .map_err(|e| AppError::Internal(format!("Failed to stop gateway: {}", e)))?;

        // Wait for it to exit
        let _ = child.wait().await;
        *guard = None;

        Ok(ok_json(json!({
            "status": "stopped",
            "message": format!("Gateway stopped (was PID: {:?})", pid),
        })))
    } else {
        Ok(ok_json(json!({
            "status": "not_running",
            "message": "Gateway was not running",
        })))
    }
}

/// POST /api/gateway/restart - Restart the gateway subprocess.
pub async fn restart_gateway(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Stop first
    {
        let mut guard = state.gateway_process.write().await;
        if let Some(ref mut child) = *guard {
            child.kill().await.ok();
            let _ = child.wait().await;
            *guard = None;
        }
    }

    // Then start
    start_gateway(State(state)).await
}

/// Check if the gateway subprocess is currently running.
pub async fn is_gateway_running(state: &AppState) -> bool {
    let guard = state.gateway_process.read().await;
    guard.is_some()
}
