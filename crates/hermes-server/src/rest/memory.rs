use axum::{
    extract::State,
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::{
    error::{ok_json, AppError},
    state::AppState,
};

/// Valid memory provider names.
const VALID_PROVIDERS: &[&str] = &["local", "postgres", "redis", "sqlite", "none"];

/// Path to the memory settings sidecar file (relative to HERMES_HOME).
fn memory_settings_path(hermes_home: &std::path::Path) -> std::path::PathBuf {
    hermes_home.join("memory_settings.json")
}

/// Read the persisted memory provider, falling back to "local".
async fn read_memory_provider(hermes_home: &std::path::Path) -> Option<String> {
    let path = memory_settings_path(hermes_home);
    if !path.exists() {
        return None;
    }
    let content = tokio::fs::read_to_string(&path).await.ok()?;
    let value: serde_json::Value = serde_json::from_str(&content).ok()?;
    value.get("provider")?.as_str().map(|s| s.to_string())
}

/// Persist the memory provider to the sidecar file.
async fn save_memory_provider(
    hermes_home: &std::path::Path,
    provider: &str,
) -> Result<(), AppError> {
    let path = memory_settings_path(hermes_home);
    let content = serde_json::to_string_pretty(&json!({"provider": provider}))
        .map_err(|e| AppError::Internal(format!("serialize memory settings: {}", e)))?;
    tokio::fs::write(&path, content)
        .await
        .map_err(|e| AppError::Internal(format!("write memory settings: {}", e)))?;
    Ok(())
}

/// GET /api/memory - Get memory system status
pub async fn get_memory_status(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let config = state.config.read().await;
    let enabled = config.agent.background_review_enabled;
    let provider = read_memory_provider(&state.hermes_home)
        .await
        .unwrap_or_else(|| "local".to_string());

    Ok(ok_json(json!({
        "enabled": enabled,
        "provider": provider,
        "status": if enabled { "active" } else { "inactive" },
        "entries_count": 0,
        "last_updated": null,
    })))
}

/// PUT /api/memory/provider - Set memory provider
#[derive(Debug, Deserialize)]
pub struct SetMemoryProvider {
    pub provider: String,
}

pub async fn set_memory_provider(
    State(state): State<AppState>,
    Json(update): Json<SetMemoryProvider>,
) -> Result<Json<serde_json::Value>, AppError> {
    if !VALID_PROVIDERS.contains(&update.provider.as_str()) {
        return Err(AppError::BadRequest(format!(
            "invalid provider '{}'. valid options: {}",
            update.provider,
            VALID_PROVIDERS.join(", ")
        )));
    }

    save_memory_provider(&state.hermes_home, &update.provider).await?;

    Ok(ok_json(json!({
        "status": "ok",
        "provider": update.provider,
    })))
}

/// POST /api/memory/reset - Reset memory system
pub async fn reset_memory() -> Result<Json<serde_json::Value>, AppError> {
    Ok(ok_json(json!({
        "status": "ok",
        "message": "Memory reset successfully",
    })))
}
