use serde_json::json;

use crate::{
    state::AppState,
    ws::rpc::{JsonRpcError, JsonRpcRequest, JsonRpcResponse},
};

/// tools.list - List available tools.
pub async fn handle_tools_list(
    request: JsonRpcRequest,
    _state: &AppState,
) -> Option<JsonRpcResponse> {
    // For now, return a placeholder list
    // In the future, this should query the ToolRegistry
    let tools = vec![
        json!({"name": "web_search", "description": "Search the web"}),
        json!({"name": "terminal", "description": "Execute terminal commands"}),
        json!({"name": "file_read", "description": "Read files"}),
    ];
    
    Some(JsonRpcResponse::ok(request.id, json!({"tools": tools})))
}

/// tools.show - Show details of a specific tool.
pub async fn handle_tools_show(
    request: JsonRpcRequest,
    _state: &AppState,
) -> Option<JsonRpcResponse> {
    let params = request.params.as_ref()?.as_object()?;
    let tool_name = params.get("tool")?.as_str()?;
    
    let tool = match tool_name {
        "web_search" => json!({
            "name": "web_search",
            "description": "Search the web",
            "schema": {
                "query": {"type": "string"},
            }
        }),
        "terminal" => json!({
            "name": "terminal",
            "description": "Execute terminal commands",
            "schema": {
                "command": {"type": "string"},
                "timeout": {"type": "number"},
            }
        }),
        _ => {
            return Some(JsonRpcResponse::err(
                request.id,
                JsonRpcError::server_error(4004, format!("tool not found: {}", tool_name)),
            ));
        }
    };
    
    Some(JsonRpcResponse::ok(request.id, tool))
}

/// tools.configure - Configure a tool.
pub async fn handle_tools_configure(
    request: JsonRpcRequest,
    _state: &AppState,
) -> Option<JsonRpcResponse> {
    let params = request.params.as_ref()?.as_object()?;
    let tool_name = params.get("tool")?.as_str()?;
    let _config = params.get("config")?;
    
    // For now, just acknowledge
    // In the future, update tool configuration
    Some(JsonRpcResponse::ok(
        request.id,
        json!({"ok": true, "tool": tool_name}),
    ))
}

/// skills.manage - Manage skills (enable/disable).
pub async fn handle_skills_manage(
    request: JsonRpcRequest,
    _state: &AppState,
) -> Option<JsonRpcResponse> {
    let params = request.params.as_ref()?.as_object()?;
    let skill_name = params.get("skill")?.as_str()?;
    let enabled = params.get("enabled")?.as_bool()?;
    
    Some(JsonRpcResponse::ok(
        request.id,
        json!({"ok": true, "skill": skill_name, "enabled": enabled}),
    ))
}

/// skills.reload - Reload skills from disk.
pub async fn handle_skills_reload(
    request: JsonRpcRequest,
    _state: &AppState,
) -> Option<JsonRpcResponse> {
    // For now, just acknowledge
    Some(JsonRpcResponse::ok(request.id, json!({"ok": true})))
}

/// reload.mcp - Reload MCP configuration.
///
/// Params: `{ session_id?: string, confirm?: boolean, always?: boolean }`
/// Returns: `{ status: string, message?: string }`
pub async fn handle_reload_mcp(
    request: JsonRpcRequest,
    _state: &AppState,
) -> Option<JsonRpcResponse> {
    let params = request.params.as_ref()?.as_object()?;
    let _confirm = params.get("confirm").and_then(|v| v.as_bool()).unwrap_or(false);
    let _always = params.get("always").and_then(|v| v.as_bool()).unwrap_or(false);
    
    // TODO: Actually reload MCP servers when MCP support is fully integrated
    Some(JsonRpcResponse::ok(request.id, json!({
        "status": "reloaded",
        "message": "MCP servers reloaded"
    })))
}

/// reload.env - Reload environment variables from ~/.hermes/.env.
///
/// Returns: `{ updated?: number }`
pub async fn handle_reload_env(
    request: JsonRpcRequest,
    state: &AppState,
) -> Option<JsonRpcResponse> {
    let hermes_home = state.hermes_home.clone();
    
    let count = tokio::task::spawn_blocking(move || {
        let env_path = hermes_home.join(".env");
        if !env_path.exists() {
            return 0usize;
        }
        
        match std::fs::read_to_string(&env_path) {
            Ok(content) => {
                let mut updated = 0;
                for line in content.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    if let Some((key, value)) = line.split_once('=') {
                        let key = key.trim();
                        let value = value.trim().trim_matches('"').trim_matches('\'');
                        if !key.is_empty() {
                            unsafe {
                                std::env::set_var(key, value);
                            }
                            updated += 1;
                        }
                    }
                }
                updated
            }
            Err(_) => 0,
        }
    })
    .await
    .unwrap_or(0);
    
    Some(JsonRpcResponse::ok(request.id, json!({ "updated": count })))
}
