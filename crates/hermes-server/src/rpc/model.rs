use serde_json::json;

use crate::{
    state::AppState,
    ws::rpc::{JsonRpcError, JsonRpcRequest, JsonRpcResponse},
};

/// model.options - Get available model options.
pub async fn handle_model_options(
    request: JsonRpcRequest,
    state: &AppState,
) -> Option<JsonRpcResponse> {
    let config = state.config.read().await;
    
    // Build model options from configured providers
    let mut options = Vec::new();
    
    for (provider_name, provider_cfg) in &config.llm_providers {
        if let Some(ref model) = provider_cfg.model {
            options.push(json!({
                "id": format!("{}:{}", provider_name, model),
                "provider": provider_name,
                "model": model,
            }));
        }
    }
    
    // Add default if configured
    if let Some(ref model) = config.model {
        options.push(json!({
            "id": format!("default:{}", model),
            "provider": "default",
            "model": model,
        }));
    }
    
    Some(JsonRpcResponse::ok(request.id, json!({"options": options})))
}

/// model.save_key - Save an API key for a provider.
pub async fn handle_model_save_key(
    request: JsonRpcRequest,
    state: &AppState,
) -> Option<JsonRpcResponse> {
    let params = request.params.as_ref()?.as_object()?;
    let provider = params.get("provider")?.as_str()?;
    let api_key = params.get("api_key")?.as_str()?;
    
    {
        let mut config = state.config.write().await;
        
        if let Some(provider_cfg) = config.llm_providers.get_mut(provider) {
            provider_cfg.api_key = Some(api_key.to_string());
        } else {
            // Create new provider config
            let mut new_cfg = hermes_config::LlmProviderConfig::default();
            new_cfg.api_key = Some(api_key.to_string());
            config.llm_providers.insert(provider.to_string(), new_cfg);
        }
    }
    
    // Save to file
    let config_path = state.config_path();
    let config = state.config.read().await.clone();
    if let Err(e) = hermes_config::save_config_yaml(&config_path, &config) {
        return Some(JsonRpcResponse::err(
            request.id,
            JsonRpcError::internal_error(format!("config write error: {}", e)),
        ));
    }
    
    Some(JsonRpcResponse::ok(request.id, json!({"ok": true})))
}

/// model.disconnect - Disconnect from a provider (clear API key).
pub async fn handle_model_disconnect(
    request: JsonRpcRequest,
    state: &AppState,
) -> Option<JsonRpcResponse> {
    let params = request.params.as_ref()?.as_object()?;
    let provider = params.get("provider")?.as_str()?;
    
    {
        let mut config = state.config.write().await;
        
        if let Some(provider_cfg) = config.llm_providers.get_mut(provider) {
            provider_cfg.api_key = None;
        }
    }
    
    // Save to file
    let config_path = state.config_path();
    let config = state.config.read().await.clone();
    if let Err(e) = hermes_config::save_config_yaml(&config_path, &config) {
        return Some(JsonRpcResponse::err(
            request.id,
            JsonRpcError::internal_error(format!("config write error: {}", e)),
        ));
    }
    
    Some(JsonRpcResponse::ok(request.id, json!({"ok": true})))
}
