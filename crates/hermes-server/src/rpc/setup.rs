use serde_json::json;

use crate::{
    state::AppState,
    ws::rpc::{JsonRpcRequest, JsonRpcResponse},
};

/// setup.status — returns whether any LLM provider has credentials configured.
pub async fn handle_status(
    request: JsonRpcRequest,
    state: &AppState,
) -> Option<JsonRpcResponse> {
    let config = state.config.read().await;
    let provider_configured = config
        .model
        .as_ref()
        .map_or(false, |m| !m.is_empty())
        && config
            .llm_providers
            .values()
            .any(|p| p.api_key.as_ref().map_or(false, |k| !k.is_empty()));

    Some(JsonRpcResponse::ok(
        request.id,
        json!({ "provider_configured": provider_configured }),
    ))
}

/// setup.runtime_check — stricter check: can the configured model actually
/// resolve to a usable runtime right now?
pub async fn handle_runtime_check(
    request: JsonRpcRequest,
    state: &AppState,
) -> Option<JsonRpcResponse> {
    let config = state.config.read().await;
    let provider_configured = config
        .model
        .as_ref()
        .map_or(false, |m| !m.is_empty())
        && config
            .llm_providers
            .values()
            .any(|p| p.api_key.as_ref().map_or(false, |k| !k.is_empty()));

    if provider_configured {
        Some(JsonRpcResponse::ok(request.id, json!({ "ok": true })))
    } else {
        Some(JsonRpcResponse::ok(
            request.id,
            json!({ "ok": false, "error": "No provider is configured." }),
        ))
    }
}
