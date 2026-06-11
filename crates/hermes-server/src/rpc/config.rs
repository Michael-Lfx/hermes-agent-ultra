use serde_json::json;

use crate::{
    state::AppState,
    ws::rpc::{JsonRpcError, JsonRpcRequest, JsonRpcResponse},
};

/// config.get - Get a config value by key.
pub async fn handle_config_get(
    request: JsonRpcRequest,
    state: &AppState,
) -> Option<JsonRpcResponse> {
    let params = request.params.as_ref()?.as_object()?;
    let key = params.get("key")?.as_str()?;
    
    let config = state.config.read().await;
    
    let value = match key {
        "model" => json!(config.model),
        "personality" => json!(config.personality),
        "max_turns" => json!(config.max_turns),
        "system_prompt" => json!(config.system_prompt),
        "tools" => json!(config.tools),
        "budget" => json!(config.budget),
        _ => {
            return Some(JsonRpcResponse::err(
                request.id,
                JsonRpcError::server_error(4002, format!("unknown config key: {}", key)),
            ));
        }
    };
    
    Some(JsonRpcResponse::ok(request.id, value))
}

/// config.set - Set a config value by key.
pub async fn handle_config_set(
    request: JsonRpcRequest,
    state: &AppState,
) -> Option<JsonRpcResponse> {
    let params = request.params.as_ref()?.as_object()?;
    let key = params.get("key")?.as_str()?;
    let value = params.get("value")?;
    
    {
        let mut config = state.config.write().await;
        match key {
            "model" => {
                config.model = Some(value.as_str()?.to_string());
            }
            "personality" => {
                config.personality = Some(value.as_str()?.to_string());
            }
            "max_turns" => {
                config.max_turns = value.as_u64()? as u32;
            }
            "system_prompt" => {
                config.system_prompt = Some(value.as_str()?.to_string());
            }
            _ => {
                return Some(JsonRpcResponse::err(
                    request.id,
                    JsonRpcError::server_error(4002, format!("unknown config key: {}", key)),
                ));
            }
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
    
    // Invalidate agent caches
    state.invalidate_agent_caches().await;
    
    Some(JsonRpcResponse::ok(request.id, json!({"ok": true})))
}

/// config.show - Show the full current config.
pub async fn handle_config_show(
    request: JsonRpcRequest,
    state: &AppState,
) -> Option<JsonRpcResponse> {
    let config = state.config.read().await;
    
    let result = json!({
        "model": config.model,
        "personality": config.personality,
        "max_turns": config.max_turns,
        "system_prompt": config.system_prompt,
        "tools": config.tools,
    });
    
    Some(JsonRpcResponse::ok(request.id, result))
}
