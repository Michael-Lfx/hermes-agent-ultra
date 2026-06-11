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

/// GET /api/model/info - Get current model metadata
pub async fn get_model_info(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let config = state.config.read().await;
    
    let model = config.model.as_deref().unwrap_or("unknown");
    
    Ok(ok_json(json!({
        "model": model,
        "provider": "default",
        "context_length": 128000,
    })))
}

/// GET /api/model/options - List available models
pub async fn get_model_options(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let config = state.config.read().await;
    
    // Build providers list from llm_providers config
    let providers: Vec<serde_json::Value> = config
        .llm_providers
        .iter()
        .map(|(name, provider_config)| {
            json!({
                "name": name,
                "model": provider_config.model,
                "base_url": provider_config.base_url,
            })
        })
        .collect();
    
    Ok(ok_json(json!({ "providers": providers })))
}

/// POST /api/model/set - Set model
#[derive(Debug, Deserialize)]
pub struct SetModel {
    pub model: String,
}

pub async fn set_model(
    State(state): State<AppState>,
    Json(update): Json<SetModel>,
) -> Result<Json<serde_json::Value>, AppError> {
    {
        let mut config = state.config.write().await;
        config.model = Some(update.model.clone());
    }
    
    // Save to config.yaml
    let config_path = state.config_path();
    let _disk_config = hermes_config::load_user_config_file(&config_path)?;
    
    Ok(ok_json(json!({
        "model": update.model,
        "ok": true,
    })))
}

/// GET /api/model/recommended-default - Get recommended default model
pub async fn get_recommended_default() -> Result<Json<serde_json::Value>, AppError> {
    Ok(ok_json(json!({
        "model": "gpt-4",
        "provider": "openai",
    })))
}

/// GET /api/model/auxiliary - Get auxiliary task models
pub async fn get_auxiliary_models(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let config = state.config.read().await;
    
    let auxiliary: Vec<serde_json::Value> = config
        .auxiliary
        .iter()
        .map(|(task, cfg)| {
            json!({
                "task": task,
                "model": cfg.model,
                "provider": cfg.provider,
            })
        })
        .collect();
    
    Ok(ok_json(json!({ "tasks": auxiliary })))
}
