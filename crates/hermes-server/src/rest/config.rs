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

/// GET /api/config - Get current configuration
pub async fn get_config(State(state): State<AppState>) -> Result<Json<serde_json::Value>, AppError> {
    let config = state.config.read().await;
    let config_json = serde_json::to_value(&*config)
        .map_err(|e| AppError::Internal(format!("serialize config: {}", e)))?;
    Ok(ok_json(config_json))
}

/// PUT /api/config - Save configuration
#[derive(Debug, Deserialize)]
pub struct ConfigUpdate {
    pub config: serde_json::Value,
}

pub async fn put_config(
    State(state): State<AppState>,
    Json(update): Json<ConfigUpdate>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Deserialize the new config
    let new_config: hermes_config::GatewayConfig = serde_json::from_value(update.config)
        .map_err(|e| AppError::BadRequest(format!("invalid config: {}", e)))?;
    
    // Validate
    hermes_config::validate_config(&new_config)
        .map_err(|e| AppError::Config(format!("validation failed: {}", e)))?;
    
    // Save to file
    let config_path = state.config_path();
    hermes_config::save_config_yaml(&config_path, &new_config)?;
    
    // Update in-memory state
    {
        let mut config = state.config.write().await;
        *config = new_config;
    }
    
    // Invalidate cached AgentLoop instances so they are rebuilt with new config
    state.invalidate_agent_caches().await;
    
    Ok(ok_json(json!({ "ok": true })))
}

/// GET /api/config/defaults - Get default configuration
pub async fn get_defaults() -> Result<Json<serde_json::Value>, AppError> {
    let default_config = hermes_config::GatewayConfig::default();
    let config_json = serde_json::to_value(default_config)
        .map_err(|e| AppError::Internal(format!("serialize defaults: {}", e)))?;
    Ok(ok_json(config_json))
}

/// GET /api/config/schema - Get configuration schema
/// For now, returns a hardcoded schema. Future versions will use schemars.
pub async fn get_schema() -> Result<Json<serde_json::Value>, AppError> {
    Ok(ok_json(json!({
        "schema": "config",
        "fields": [
            { "key": "model", "type": "string", "description": "Default model name" },
            { "key": "personality", "type": "string", "description": "Personality setting" },
            { "key": "max_turns", "type": "integer", "description": "Maximum conversation turns" },
            { "key": "tools", "type": "array", "description": "Enabled tools" },
            { "key": "platforms", "type": "object", "description": "Platform configurations" },
        ]
    })))
}

/// GET /api/config/raw - Get raw YAML configuration
pub async fn get_raw_config(State(state): State<AppState>) -> Result<Json<serde_json::Value>, AppError> {
    let config_path = state.config_path();
    let yaml_content = tokio::fs::read_to_string(&config_path)
        .await
        .map_err(|e| AppError::Internal(format!("read config: {}", e)))?;
    
    Ok(ok_json(json!({
        "yaml": yaml_content,
    })))
}

/// PUT /api/config/raw - Save raw YAML configuration
#[derive(Debug, Deserialize)]
pub struct RawConfigUpdate {
    pub yaml: String,
}

pub async fn put_raw_config(
    State(state): State<AppState>,
    Json(update): Json<RawConfigUpdate>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Parse to validate
    let new_config: hermes_config::GatewayConfig = serde_yaml::from_str(&update.yaml)?;
    
    // Validate
    hermes_config::validate_config(&new_config)?;
    
    // Save raw YAML
    let config_path = state.config_path();
    tokio::fs::write(&config_path, update.yaml)
        .await
        .map_err(|e| AppError::Internal(format!("write config: {}", e)))?;
    
    // Update in-memory state
    {
        let mut config = state.config.write().await;
        *config = new_config;
    }
    
    // Invalidate cached AgentLoop instances so they are rebuilt with new config
    state.invalidate_agent_caches().await;
    
    Ok(ok_json(json!({ "ok": true })))
}
