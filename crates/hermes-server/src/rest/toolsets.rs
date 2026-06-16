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
/// Groups tools by their toolset category using hermes_tools registry.
pub async fn list_toolsets(State(state): State<AppState>) -> Result<Json<serde_json::Value>, AppError> {
    let tools_registry = state.tools_registry.clone();
    let toolset_names = tools_registry.list_toolsets();
    
    let mut toolsets: Vec<serde_json::Value> = toolset_names
        .into_iter()
        .map(|name| {
            let tools = tools_registry.tool_names_for_toolset(&name, false);
            json!({
                "name": name,
                "enabled": true,
                "provider": serde_json::Value::Null,
                "tools": tools,
            })
        })
        .collect();
    
    // Add "all" toolset as aggregate
    let all_tools = tools_registry.tool_names_for_toolset("all", false);
    if !toolsets.iter().any(|t| t["name"] == "all") {
        toolsets.insert(0, json!({
            "name": "all",
            "enabled": true,
            "provider": serde_json::Value::Null,
            "tools": all_tools,
        }));
    }
    
    Ok(Json(json!(toolsets)))
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
/// Returns toolset config with provider options (matching ToolsetConfig TypeScript type).
pub async fn get_toolset_config(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let config = read_toolsets_config(&state.hermes_home);
    let toolset = config.get(&name).cloned().unwrap_or_default();
    
    let providers: Vec<serde_json::Value> = state.tool_registry.schemas().iter()
        .map(|s| {
            json!({
                "name": s.name,
                "badge": "",
                "tag": "",
                "env_vars": [],
                "post_setup": serde_json::Value::Null,
                "requires_nous_auth": false,
                "is_active": toolset.tools.is_empty() || toolset.tools.contains(&s.name),
            })
        })
        .collect();
    
    Ok(Json(json!({
        "name": name,
        "has_category": false,
        "providers": providers,
        "active_provider": toolset.provider,
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
