use std::sync::Arc;

use hermes_agent::{AgentCallbacks, AgentConfig, AgentLoop, AnthropicProvider, OpenAiProvider, ToolRegistry};
use hermes_config::GatewayConfig;
use hermes_core::{LlmProvider, Message};
use serde_json::json;

use crate::ws::{
    rpc::JsonRpcEvent,
    transport::{ReplaceableTransport, Transport},
};

/// Build an AgentLoop for a session from gateway configuration.
/// Binds tool/thinking callbacks to the session's transport so events flow to the Desktop client.
pub fn build_agent(
    config: &GatewayConfig,
    session_id: &str,
    session_key: &str,
    hermes_home: &std::path::Path,
    transport: ReplaceableTransport,
    pending: &crate::rpc::interaction::PendingInteractions,
    tool_registry: Arc<ToolRegistry>,
    tools_registry: Arc<hermes_tools::ToolRegistry>,
) -> Option<AgentLoop> {
    let mut agent_config = AgentConfig {
        model: config.model.clone().unwrap_or_else(|| "gpt-4o".to_string()),
        session_id: Some(session_id.to_string()),
        gateway_session_key: Some(session_key.to_string()),
        hermes_home: Some(hermes_home.to_string_lossy().to_string()),
        stream: true,
        quiet_mode: false,
        ..Default::default()
    };

    // Resolve provider and API key from config
    let llm_provider = build_llm_provider(config, &mut agent_config)?;

    // Clone before moving into AgentLoop; also needed for interaction dispatch
    let dispatch_registry = tool_registry.clone();
    let mut agent = AgentLoop::new(agent_config, tool_registry, llm_provider);

    // Bind callbacks to push events to the Desktop client via WebSocket
    let sid = session_id.to_string();
    let tool_id_counter = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let current_tool = Arc::new(std::sync::Mutex::new(None::<String>));
    let callbacks = AgentCallbacks {
        on_tool_start: Some(Box::new({
            let transport = transport.clone();
            let sid = sid.clone();
            let current_tool = current_tool.clone();
            let counter = tool_id_counter.clone();
            move |tool_name, args| {
                let tool_id = format!("tool_{}", counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst));
                
                // Track current tool for progress events
                if let Ok(mut guard) = current_tool.lock() {
                    *guard = Some(tool_name.to_string());
                }
                
                let args_preview = serde_json::to_string(&args).unwrap_or_default();
                let context = if args_preview.len() > 200 {
                    format!("{}...", &args_preview[..200])
                } else {
                    args_preview.clone()
                };
                
                // Emit tool.start
                let event = JsonRpcEvent::new(
                    crate::ws::events::types::TOOL_START,
                    Some(sid.clone()),
                    Some(json!({
                        "tool_id": tool_id,
                        "name": tool_name,
                        "context": context,
                        "args_text": args_preview,
                    })),
                );
                if let Ok(val) = serde_json::to_value(&event) {
                    let _ = transport.write(&val);
                }
                
                // Emit tool.generating
                let event = JsonRpcEvent::new(
                    crate::ws::events::types::TOOL_GENERATING,
                    Some(sid.clone()),
                    Some(json!({
                        "name": tool_name,
                    })),
                );
                if let Ok(val) = serde_json::to_value(&event) {
                    let _ = transport.write(&val);
                }
            }
        })),
        on_tool_complete: Some(Box::new({
            let transport = transport.clone();
            let sid = sid.clone();
            let current_tool = current_tool.clone();
            move |tool_name, result| {
                // Clear current tool tracking
                if let Ok(mut guard) = current_tool.lock() {
                    *guard = None;
                }
                
                let event = JsonRpcEvent::new(
                    crate::ws::events::types::TOOL_COMPLETE,
                    Some(sid.clone()),
                    Some(json!({
                        "name": tool_name,
                        "args": serde_json::Value::Null,
                        "result": result,
                    })),
                );
                if let Ok(val) = serde_json::to_value(&event) {
                    let _ = transport.write(&val);
                }
            }
        })),
        on_thinking: Some(Box::new({
            let transport = transport.clone();
            let sid = sid.clone();
            let reasoning_sent = std::sync::atomic::AtomicBool::new(false);
            move |text| {
                let event = JsonRpcEvent::new(
                    crate::ws::events::types::THINKING_DELTA,
                    Some(sid.clone()),
                    Some(json!({
                        "content": text,
                    })),
                );
                if let Ok(val) = serde_json::to_value(&event) {
                    let _ = transport.write(&val);
                }
                
                // Emit reasoning.available once when thinking content is substantial
                if !reasoning_sent.load(std::sync::atomic::Ordering::SeqCst) && text.len() > 50 {
                    let avail_event = JsonRpcEvent::new(
                        crate::ws::events::types::REASONING_AVAILABLE,
                        Some(sid.clone()),
                        Some(json!({})),
                    );
                    if let Ok(val) = serde_json::to_value(&avail_event) {
                        let _ = transport.write(&val);
                    }
                    reasoning_sent.store(true, std::sync::atomic::Ordering::SeqCst);
                }
            }
        })),
        on_stream_delta: Some(Box::new({
            let transport = transport.clone();
            let sid = sid.clone();
            move |text| {
                let event = JsonRpcEvent::new(
                    crate::ws::events::types::MESSAGE_DELTA,
                    Some(sid.clone()),
                    Some(json!({ "text": text })),
                );
                if let Ok(val) = serde_json::to_value(&event) {
                    let _ = transport.write(&val);
                }
            }
        })),
        status_callback: Some(Arc::new({
            let transport = transport.clone();
            let sid = sid.clone();
            let current_tool = current_tool.clone();
            move |kind: &str, msg: &str| {
                if kind == "tool_progress" {
                    let tool_name = current_tool
                        .lock()
                        .ok()
                        .and_then(|g| g.clone())
                        .unwrap_or_else(|| "tool".to_string());
                    let event = JsonRpcEvent::new(
                        crate::ws::events::types::TOOL_PROGRESS,
                        Some(sid.clone()),
                        Some(json!({
                            "name": tool_name,
                            "preview": msg,
                        })),
                    );
                    if let Ok(val) = serde_json::to_value(&event) {
                        let _ = transport.write(&val);
                    }
                }
            }
        })),
        ..Default::default()
    };

    agent = agent.with_callbacks(callbacks);
    
    // Set up async tool dispatch for interactive tools (approval/clarify/sudo/secret)
    // Non-interactive tools fall through to the full hermes_tools registry.
    let dispatch = crate::core::interaction_dispatch::create_interaction_dispatch(
        transport.clone(),
        pending.clone(),
        tools_registry,
        dispatch_registry,
    );
    agent = agent.with_async_tool_dispatch(dispatch);
    
    Some(agent)
}

/// Load conversation history from session persistence DB.
pub fn load_history(
    session_id: &str,
    hermes_home: &std::path::Path,
) -> Result<Vec<Message>, hermes_core::AgentError> {
    let persistence = hermes_agent::SessionPersistence::new(hermes_home);
    persistence.ensure_db()?;
    persistence.load_session(session_id)
}

fn build_llm_provider(
    config: &GatewayConfig,
    agent_config: &mut AgentConfig,
) -> Option<Arc<dyn LlmProvider>> {
    // Parse model prefix (e.g. "custom:stepfun-ai/step-3.5-flash" → "custom")
    let model_prefix = config.model.as_deref()
        .and_then(|m| m.split_once(':'))
        .map(|(p, _)| p)
        .filter(|p| !p.is_empty());

    // Priority 1: if model has a provider prefix, find that provider directly
    if let Some(prefix) = model_prefix {
        if let Some(cfg) = config.llm_providers.get(prefix) {
            let api_key = resolve_api_key(cfg)?;
            let base_url = cfg.base_url.clone()
                .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
            let model_name = config.model.as_deref()
                .and_then(|m| m.split_once(':'))
                .map(|(_, m)| m.trim())
                .filter(|m| !m.is_empty())
                .unwrap_or("gpt-4o");
            agent_config.provider = Some(prefix.to_string());
            return Some(Arc::new(
                OpenAiProvider::new(api_key)
                    .with_base_url(base_url)
                    .with_model(model_name)
                    .with_provider_profile(prefix.to_string()),
            ));
        }
    }

    // Priority 2: try to find a known provider with an API key
    for (name, provider_cfg) in &config.llm_providers {
        let Some(api_key) = resolve_api_key(provider_cfg) else { continue };

        match name.as_str() {
            "openai" => {
                agent_config.provider = Some("openai".to_string());
                return Some(Arc::new(OpenAiProvider::new(api_key)));
            }
            "anthropic" => {
                agent_config.provider = Some("anthropic".to_string());
                return Some(Arc::new(AnthropicProvider::new(api_key)));
            }
            "openrouter" => {
                agent_config.provider = Some("openrouter".to_string());
                return Some(Arc::new(hermes_agent::OpenRouterProvider::new(api_key)));
            }
            _ => continue,
        }
    }

    // Priority 3: try any remaining provider as generic OpenAI-compatible
    for (name, provider_cfg) in &config.llm_providers {
        let api_key = resolve_api_key(provider_cfg)?;
        let base_url = provider_cfg.base_url.clone().unwrap_or_else(|| "https://api.openai.com/v1".to_string());
        let model_name = config.model.as_deref()
            .and_then(|m| m.split_once(':'))
            .map(|(_, m)| m.trim())
            .filter(|m| !m.is_empty())
            .unwrap_or("gpt-4o");
        agent_config.provider = Some(name.clone());
        return Some(Arc::new(
            OpenAiProvider::new(api_key)
                .with_base_url(base_url)
                .with_model(model_name)
                .with_provider_profile(name.clone()),
        ));
    }

    None
}

fn resolve_api_key(provider_cfg: &hermes_config::LlmProviderConfig) -> Option<String> {
    provider_cfg
        .api_key
        .clone()
        .or_else(|| {
            provider_cfg
                .api_key_env
                .as_ref()
                .and_then(|env_var| std::env::var(env_var).ok())
        })
        .filter(|k| !k.trim().is_empty())
}
