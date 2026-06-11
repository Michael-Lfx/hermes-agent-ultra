use axum::{extract::{Path, State}, Json};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;

use crate::{error::AppError, state::AppState};

/// Toolset configuration persisted to disk.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ToolsetConfig {
    enabled: bool,
    provider: Option<String>,
    #[serde(default)]
    tools: Vec<String>,
}

/// Read toolset configuration from disk.
fn read_toolsets_config(hermes_home: &std::path::Path) -> HashMap<String, ToolsetConfig> {
    let path = hermes_home.join("toolsets.json");
    if let Ok(content) = std::fs::read_to_string(&path) {
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        // Return default "all" toolset
        let mut default = HashMap::new();
        default.insert("all".to_string(), ToolsetConfig {
            enabled: true,
            provider: None,
            tools: vec![],
        });
        default
    }
}

/// Write toolset configuration to disk.
async fn write_toolsets_config(
    hermes_home: &std::path::Path,
    config: &HashMap<String, ToolsetConfig>,
) -> Result<(), AppError> {
    let path = hermes_home.join("toolsets.json");
    let content = serde_json::to_string_pretty(config)
        .map_err(|e| AppError::Internal(format!("serialize toolsets: {}", e)))?;
    tokio::fs::write(&path, content)
        .await
        .map_err(|e| AppError::Internal(format!("write toolsets: {}", e)))?;
    Ok(())
}

/// GET /api/tools/toolsets - List all toolsets
///
/// Returns toolsets from persisted configuration, falling back to default "all" toolset.
pub async fn list_toolsets(State(state): State<AppState>) -> Result<Json<serde_json::Value>, AppError> {
    let registry = state.tool_registry.clone();
    let schemas = registry.schemas();
    let tool_names: Vec<String> = schemas.iter().map(|s| s.name.clone()).collect();
    
    let config = read_toolsets_config(&state.hermes_home);
    
    // Build toolsets list from config
    let mut toolsets = Vec::new();
    for (name, ts_config) in config {
        let tools = if name == "all" { tool_names.clone() } else { ts_config.tools };
        toolsets.push(json!({
            "name": name,
            "enabled": ts_config.enabled,
            "provider": ts_config.provider,
            "tools": tools,
        }));
    }
    
    // Ensure "all" toolset exists
    if !toolsets.iter().any(|t| t["name"] == "all") {
        toolsets.push(json!({
            "name": "all",
            "enabled": true,
            "provider": null,
            "tools": tool_names,
        }));
    }
    
    Ok(Json(json!({
        "toolsets": toolsets,
    })))
}

/// PUT /api/tools/toolsets/{name} - Enable/disable a toolset
///
/// Updates the toolset configuration in toolsets.json.
pub async fn toggle_toolset(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    let enabled = payload["enabled"].as_bool()
        .ok_or_else(|| AppError::BadRequest("Missing enabled flag".to_string()))?;

    let mut config = read_toolsets_config(&state.hermes_home);
    
    let entry = config.entry(name.clone()).or_insert_with(|| ToolsetConfig {
        enabled: true,
        provider: None,
        tools: vec![],
    });
    entry.enabled = enabled;
    
    write_toolsets_config(&state.hermes_home, &config).await?;
    
    tracing::info!(toolset = %name, enabled = enabled, "toolset toggled successfully");
    
    Ok(Json(json!({
        "status": "ok",
        "toolset": name,
        "enabled": enabled,
    })))
}

/// GET /api/tools/toolsets/{name}/config - Get toolset configuration
///
/// Returns tool schemas for the tools in this toolset.
pub async fn get_toolset_config(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let registry = state.tool_registry.clone();
    let schemas = registry.schemas();
    
    // Filter schemas by toolset (currently returns all schemas)
    let filtered: Vec<_> = if name == "all" {
        schemas.iter().collect()
    } else {
        // TODO: Filter by actual toolset membership when ToolsetManager is integrated
        schemas.iter().collect()
    };
    
    Ok(Json(json!({
        "toolset": name,
        "config": {
            "tools": filtered.len(),
            "schemas": filtered,
        }
    })))
}

/// PUT /api/tools/toolsets/{name}/provider - Set toolset provider
///
/// Updates the toolset provider in toolsets.json.
pub async fn set_toolset_provider(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    let provider = payload["provider"].as_str()
        .ok_or_else(|| AppError::BadRequest("Missing provider".to_string()))?;

    let mut config = read_toolsets_config(&state.hermes_home);
    
    let entry = config.entry(name.clone()).or_insert_with(|| ToolsetConfig {
        enabled: true,
        provider: None,
        tools: vec![],
    });
    entry.provider = Some(provider.to_string());
    
    write_toolsets_config(&state.hermes_home, &config).await?;
    
    tracing::info!(toolset = %name, provider = %provider, "toolset provider set successfully");
    
    Ok(Json(json!({
        "status": "ok",
        "toolset": name,
        "provider": provider,
    })))
}

/// POST /api/tools/toolsets/{name}/post-setup - Run post-setup for a toolset
///
/// Currently a no-op.
pub async fn run_post_setup(
    State(_state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    tracing::info!(toolset = %name, "run post-setup (mock)");
    
    Ok(Json(json!({
        "status": "ok",
        "toolset": name,
        "message": "Post-setup completed successfully",
    })))
}
