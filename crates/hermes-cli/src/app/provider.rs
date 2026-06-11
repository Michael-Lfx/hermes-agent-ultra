use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::time::{Duration, Instant};

use futures::StreamExt;
use serde_json::Value;

use hermes_agent::AgentConfig;
use hermes_agent::agent_loop::ToolRegistry as AgentToolRegistry;
use hermes_agent::provider::{
    AnthropicProvider, GenericProvider, OpenAiProvider, OpenRouterProvider,
};
use hermes_agent::providers_extra::{
    CopilotProvider, KimiProvider, MiniMaxProvider, NousProvider, QwenProvider,
};
use hermes_config::{GatewayConfig, hermes_home as hermes_home_dir};
use hermes_core::{AgentError, LlmProvider};
use hermes_tools::ToolRegistry;

use crate::cli::Cli;

use super::pet::{PetDock, PetSettings};
pub(super) fn apply_cli_runtime_overrides(config: &mut GatewayConfig, cli: &Cli) {
    if let Some(ref model) = cli.model {
        config.model = Some(model.clone());
    }
    if let Some(ref personality) = cli.personality {
        config.personality = Some(personality.clone());
    }
    if let Some(provider) = cli
        .provider
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        let provider = normalize_runtime_provider_name(provider);
        let existing_model = config.model.as_deref().unwrap_or("gpt-4o").trim();
        let model_name = existing_model
            .split_once(':')
            .map(|(_, name)| name.trim())
            .unwrap_or(existing_model);
        config.model = Some(format!("{provider}:{model_name}"));
    }
}

// ---------------------------------------------------------------------------
// Helper: build AgentConfig from GatewayConfig
// ---------------------------------------------------------------------------

pub fn build_agent_config(config: &GatewayConfig, model: &str) -> AgentConfig {
    let (resolved_provider, _) = resolve_provider_and_model(config, model);
    let runtime_provider = normalize_runtime_provider_name(resolved_provider.as_str());
    let provider_extra_body = config
        .llm_providers
        .get(resolved_provider.as_str())
        .or_else(|| config.llm_providers.get(runtime_provider.as_str()))
        .or_else(|| {
            config.llm_providers.iter().find_map(|(name, cfg)| {
                if name.eq_ignore_ascii_case(resolved_provider.as_str())
                    || name.eq_ignore_ascii_case(runtime_provider.as_str())
                {
                    Some(cfg)
                } else {
                    None
                }
            })
        })
        .and_then(|cfg| cfg.extra_body.clone());
    let skip_memory_env = std::env::var("HERMES_SKIP_MEMORY")
        .ok()
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);
    let skip_context_files_env = std::env::var("HERMES_SKIP_CONTEXT_FILES")
        .ok()
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);
    let hermes_home = config
        .home_dir
        .as_ref()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(hermes_config::hermes_home);
    let skip_memory = skip_memory_env || hermes_home.join(".memory_disabled").exists();
    let skip_context_files = config.agent.skip_context_files || skip_context_files_env;

    let mut retry_cfg = hermes_agent::agent_loop::RetryConfig::default();
    if let Ok(raw) = std::env::var("HERMES_FALLBACK_MODELS") {
        let parsed: Vec<String> = raw
            .split(',')
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(ToString::to_string)
            .collect();
        if !parsed.is_empty() {
            retry_cfg.fallback_models = parsed.clone();
            retry_cfg.fallback_model = parsed.first().cloned();
        }
    }
    if retry_cfg.fallback_model.is_none() {
        if let Ok(raw) = std::env::var("HERMES_FALLBACK_MODEL") {
            let value = raw.trim();
            if !value.is_empty() {
                retry_cfg.fallback_model = Some(value.to_string());
                if retry_cfg.fallback_models.is_empty() {
                    retry_cfg.fallback_models.push(value.to_string());
                }
            }
        }
    }

    let cache_ttl = if config.prompt_caching.cache_ttl.as_str() == "1h" {
        "1h".to_string()
    } else {
        "5m".to_string()
    };
    let provider_base_url = config
        .llm_providers
        .get(resolved_provider.as_str())
        .or_else(|| config.llm_providers.get(runtime_provider.as_str()))
        .and_then(|c| c.base_url.clone())
        .unwrap_or_default();
    let api_mode_str = if resolved_provider.eq_ignore_ascii_case("anthropic")
        || model.to_ascii_lowercase().contains("claude")
    {
        "anthropic_messages"
    } else {
        "chat_completions"
    };
    let (use_prompt_caching, use_native_cache_layout) =
        hermes_agent::prompt_caching::anthropic_prompt_cache_policy(
            &resolved_provider,
            &provider_base_url,
            api_mode_str,
            model,
        );
    let max_delegate_depth = config
        .delegation
        .max_spawn_depth
        .map(|depth| depth.max(1))
        .unwrap_or_else(|| AgentConfig::default().max_delegate_depth);

    AgentConfig {
        max_turns: config.max_turns,
        budget: config.budget.clone(),
        model: model.to_string(),
        system_prompt: config.system_prompt.clone(),
        personality: config.personality.clone(),
        extra_body: provider_extra_body,
        hermes_home: config.home_dir.clone(),
        provider: Some(resolved_provider),
        stream: config.streaming.enabled,
        skip_memory,
        skip_context_files,
        platform: Some("cli".to_string()),
        enabled_skills: config.skills.enabled.clone(),
        disabled_skills: config.skills.disabled.clone(),
        pass_session_id: true,
        runtime_providers: config
            .llm_providers
            .iter()
            .map(|(name, cfg)| {
                (
                    name.clone(),
                    hermes_agent::agent_loop::RuntimeProviderConfig {
                        api_key: cfg.api_key.clone(),
                        api_key_env: cfg.api_key_env.clone(),
                        base_url: cfg.base_url.clone(),
                        command: cfg.command.clone(),
                        args: cfg.args.clone(),
                        oauth_token_url: cfg.oauth_token_url.clone(),
                        oauth_client_id: cfg.oauth_client_id.clone(),
                        request_timeout_seconds: cfg.request_timeout_seconds,
                        api_mode: cfg
                            .api_mode
                            .as_deref()
                            .and_then(parse_runtime_provider_api_mode),
                    },
                )
            })
            .collect(),
        retry: retry_cfg,
        smart_model_routing: hermes_agent::agent_loop::SmartModelRoutingConfig {
            enabled: config.smart_model_routing.enabled,
            max_simple_chars: config.smart_model_routing.max_simple_chars,
            max_simple_words: config.smart_model_routing.max_simple_words,
            cheap_model: config.smart_model_routing.cheap_model.as_ref().map(|m| {
                hermes_agent::agent_loop::CheapModelRouteConfig {
                    provider: m.provider.clone(),
                    model: m.model.clone(),
                    base_url: m.base_url.clone(),
                    api_key_env: m.api_key_env.clone(),
                }
            }),
        },
        memory_nudge_interval: config.agent.memory_nudge_interval,
        skill_creation_nudge_interval: config.agent.skill_creation_nudge_interval,
        background_review_enabled: config.agent.background_review_enabled,
        interest: config.interest.clone(),
        code_index_enabled: config.agent.code_index_enabled,
        code_index_max_files: config.agent.code_index_max_files,
        code_index_max_symbols: config.agent.code_index_max_symbols,
        lsp_context_enabled: config.agent.lsp_context_enabled,
        lsp_context_max_chars: config.agent.lsp_context_max_chars,
        cache_ttl,
        use_prompt_caching,
        use_native_cache_layout,
        web_research: config.agent.web_research.clone(),
        max_delegate_depth,
        delegation_model: config
            .delegation
            .model
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        delegation_provider: config
            .delegation
            .provider
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        delegation_base_url: config
            .delegation
            .base_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        delegation_api_key: config
            .delegation
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        prefill_messages: hermes_config::load_prefill_messages(config),
        ..AgentConfig::default()
    }
}

// ---------------------------------------------------------------------------
// Helper: bridge hermes_tools::ToolRegistry → agent_loop::ToolRegistry
// ---------------------------------------------------------------------------

/// Build async tool dispatch for gateway agents (uses `dispatch_async`, no `block_in_place`).
pub fn async_tool_dispatch_for(tools: Arc<ToolRegistry>) -> hermes_agent::AsyncToolDispatch {
    Arc::new(move |name, params| {
        let tools = tools.clone();
        Box::pin(async move {
            let output = tools.dispatch_async(&name, params).await;
            hermes_tools_dispatch_output(output)
        })
    })
}

fn hermes_tools_dispatch_output(output: String) -> Result<String, hermes_core::ToolError> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&output) {
        if let Some(err) = value.get("error").and_then(|v| v.as_str()) {
            return Err(hermes_core::ToolError::ExecutionFailed(err.to_string()));
        }
    }
    Ok(output)
}

pub fn bridge_tool_registry(tools: &ToolRegistry) -> AgentToolRegistry {
    let mut agent_registry = AgentToolRegistry::new();
    for schema in tools.get_definitions() {
        let name = schema.name.clone();
        let tools_clone = tools.clone();
        agent_registry.register(
            name.clone(),
            schema,
            Arc::new(
                move |params: Value| -> Result<String, hermes_core::ToolError> {
                    Ok(tools_clone.dispatch(&name, params))
                },
            ),
        );
    }
    agent_registry
}

// ---------------------------------------------------------------------------
// Helper: build LLM provider from config + model string
// ---------------------------------------------------------------------------

const STEPFUN_BASE_URL: &str = "https://api.stepfun.ai/step_plan/v1";
const OPENAI_CODEX_BASE_URL: &str = "https://chatgpt.com/backend-api/codex";
const QWEN_BASE_URL: &str = "https://dashscope-intl.aliyuncs.com/compatible-mode/v1";
const ALIBABA_CODING_PLAN_BASE_URL: &str = "https://coding-intl.dashscope.aliyuncs.com/v1";
const GOOGLE_GEMINI_CLI_BASE_URL: &str = "cloudcode-pa://google";
const GEMINI_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";
const AI_GATEWAY_BASE_URL: &str = "https://ai-gateway.vercel.sh/v1";
const KIMI_CODING_BASE_URL: &str = "https://api.moonshot.ai/v1";
const KIMI_CODING_CN_BASE_URL: &str = "https://api.moonshot.cn/v1";
const MINIMAX_CN_BASE_URL: &str = "https://api.minimaxi.com/anthropic";
const XAI_BASE_URL: &str = "https://api.x.ai/v1";
const NVIDIA_BASE_URL: &str = "https://integrate.api.nvidia.com/v1";
const OPENCODE_GO_BASE_URL: &str = "https://opencode.ai/zen/go/v1";
const OPENCODE_ZEN_BASE_URL: &str = "https://opencode.ai/zen/v1";
const KILOCODE_BASE_URL: &str = "https://api.kilo.ai/api/gateway";
const HUGGINGFACE_BASE_URL: &str = "https://router.huggingface.co/v1";
const XIAOMI_BASE_URL: &str = "https://api.xiaomimimo.com/v1";
const ZAI_BASE_URL: &str = "https://api.z.ai/api/paas/v4";
const ARCEE_BASE_URL: &str = "https://api.arcee.ai/api/v1";
const OLLAMA_CLOUD_BASE_URL: &str = "https://ollama.com/v1";
const DEEPSEEK_BASE_URL: &str = "https://api.deepseek.com/v1";
const OLLAMA_LOCAL_BASE_URL: &str = "http://127.0.0.1:11434/v1";
const LLAMA_CPP_BASE_URL: &str = "http://127.0.0.1:8080/v1";
const VLLM_BASE_URL: &str = "http://127.0.0.1:8000/v1";
const MLX_BASE_URL: &str = "http://127.0.0.1:8080/v1";
const APPLE_ANE_BASE_URL: &str = "http://127.0.0.1:8081/v1";
const SGLANG_BASE_URL: &str = "http://127.0.0.1:30000/v1";
const TGI_BASE_URL: &str = "http://127.0.0.1:8082/v1";

pub(super) fn normalize_runtime_provider_name(provider: &str) -> String {
    let normalized = provider.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "codex" => "openai-codex".to_string(),
        "claude" | "claude-code" => "anthropic".to_string(),
        "qwen-cli" | "qwen-portal" => "qwen-oauth".to_string(),
        "gemini-cli" | "gemini-oauth" => "google-gemini-cli".to_string(),
        "step" | "step-plan" => "stepfun".to_string(),
        "moonshot" | "kimi-coding" | "kimi-coding-cn" => "kimi".to_string(),
        "alibaba" | "alibaba-coding-plan" => "qwen".to_string(),
        "minimax-cn" => "minimax".to_string(),
        "kilo" | "kilo-code" | "kilo-gateway" => "kilocode".to_string(),
        "opencode" | "opencode-zen" | "zen" => "opencode-zen".to_string(),
        "go" => "opencode-go".to_string(),
        "ollama" => "ollama-local".to_string(),
        "llama.cpp" | "llamacpp" => "llama-cpp".to_string(),
        "ollvm" | "llvm" => "vllm".to_string(),
        "mlx-lm" | "apple-mlx" => "mlx".to_string(),
        "ane" | "apple-neural-engine" | "neural-engine" => "apple-ane".to_string(),
        "text-generation-inference" => "tgi".to_string(),
        _ => normalized,
    }
}

fn provider_default_base_url(provider: &str) -> Option<&'static str> {
    match provider.trim().to_ascii_lowercase().as_str() {
        "openai-codex" | "codex" => Some(OPENAI_CODEX_BASE_URL),
        "google-gemini-cli" | "gemini-cli" | "gemini-oauth" => Some(GOOGLE_GEMINI_CLI_BASE_URL),
        "gemini" | "google" => Some(GEMINI_BASE_URL),
        "qwen" | "alibaba" => Some(QWEN_BASE_URL),
        "alibaba-coding-plan" => Some(ALIBABA_CODING_PLAN_BASE_URL),
        "stepfun" | "step" | "step-plan" => Some(STEPFUN_BASE_URL),
        "ai-gateway" => Some(AI_GATEWAY_BASE_URL),
        "kimi-coding" => Some(KIMI_CODING_BASE_URL),
        "kimi-coding-cn" | "moonshot" | "kimi" => Some(KIMI_CODING_CN_BASE_URL),
        "minimax-cn" => Some(MINIMAX_CN_BASE_URL),
        "xai" => Some(XAI_BASE_URL),
        "nvidia" => Some(NVIDIA_BASE_URL),
        "opencode-go" => Some(OPENCODE_GO_BASE_URL),
        "opencode-zen" | "opencode" => Some(OPENCODE_ZEN_BASE_URL),
        "kilocode" | "kilo" => Some(KILOCODE_BASE_URL),
        "huggingface" => Some(HUGGINGFACE_BASE_URL),
        "xiaomi" => Some(XIAOMI_BASE_URL),
        "zai" => Some(ZAI_BASE_URL),
        "arcee" => Some(ARCEE_BASE_URL),
        "ollama-cloud" => Some(OLLAMA_CLOUD_BASE_URL),
        "ollama-local" | "ollama" => Some(OLLAMA_LOCAL_BASE_URL),
        "llama-cpp" | "llama.cpp" | "llamacpp" => Some(LLAMA_CPP_BASE_URL),
        "vllm" | "ollvm" | "llvm" => Some(VLLM_BASE_URL),
        "mlx" | "mlx-lm" | "apple-mlx" => Some(MLX_BASE_URL),
        "apple-ane" | "ane" | "apple-neural-engine" => Some(APPLE_ANE_BASE_URL),
        "sglang" => Some(SGLANG_BASE_URL),
        "tgi" | "text-generation-inference" => Some(TGI_BASE_URL),
        "deepseek" => Some(DEEPSEEK_BASE_URL),
        _ => None,
    }
}

pub(super) fn resolve_provider_and_model(config: &GatewayConfig, model: &str) -> (String, String) {
    let trimmed = model.trim();
    if let Some((provider, model_name)) = trimmed.split_once(':') {
        return (provider.trim().to_string(), model_name.trim().to_string());
    }

    if let Some((provider, _)) = config.llm_providers.iter().find(|(_, cfg)| {
        cfg.model
            .as_deref()
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .is_some_and(|m| m == trimmed)
    }) {
        return (provider.to_string(), trimmed.to_string());
    }

    if config.llm_providers.len() == 1 {
        if let Some((provider, _)) = config.llm_providers.iter().next() {
            return (provider.to_string(), trimmed.to_string());
        }
    }

    ("openai".to_string(), trimmed.to_string())
}

pub(super) fn resolve_startup_model(config: &GatewayConfig, configured_model: &str) -> String {
    let raw = configured_model.trim();
    if raw.is_empty() {
        return "gpt-4o".to_string();
    }
    if raw.contains(':') {
        return raw.to_string();
    }

    // If config.model is a provider slug (e.g. "nous"), prefer that provider's
    // configured runtime model instead of sending the bare slug as a model id.
    if let Some((provider, provider_cfg)) = config
        .llm_providers
        .iter()
        .find(|(provider, _)| provider.eq_ignore_ascii_case(raw))
    {
        if let Some(runtime_model) = provider_cfg
            .model
            .as_deref()
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .filter(|m| !m.eq_ignore_ascii_case(provider))
        {
            if runtime_model.contains(':') {
                return runtime_model.to_string();
            }
            return format!("{provider}:{runtime_model}");
        }
    }

    raw.to_string()
}

pub(super) fn sync_runtime_model_env(config: &GatewayConfig, provider_model: &str) {
    let model = provider_model.trim();
    if model.is_empty() {
        return;
    }
    let (provider, _) = resolve_provider_and_model(config, model);
    crate::env_vars::set_var("HERMES_MODEL", model);
    crate::env_vars::set_var("HERMES_INFERENCE_MODEL", model);
    crate::env_vars::set_var("HERMES_INFERENCE_PROVIDER", provider.as_str());
    if std::env::var_os("HERMES_TUI_PROVIDER").is_some() {
        crate::env_vars::set_var("HERMES_TUI_PROVIDER", provider.as_str());
    }
}

fn resolve_api_key_literal_or_env_ref(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(env_ref) = trimmed.strip_prefix("${").and_then(|s| s.strip_suffix('}')) {
        return std::env::var(env_ref).ok().filter(|v| !v.trim().is_empty());
    }
    Some(trimmed.to_string())
}

pub(super) fn default_mouse_enabled() -> bool {
    match std::env::var("HERMES_TUI_MOUSE") {
        Ok(value) => !matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "0" | "false" | "off" | "no"
        ),
        Err(_) => false,
    }
}

fn pet_settings_path() -> PathBuf {
    hermes_home_dir().join("pet.json")
}

fn parse_runtime_provider_api_mode(value: &str) -> Option<hermes_agent::agent_loop::ApiMode> {
    match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
        "chat_completions" => Some(hermes_agent::agent_loop::ApiMode::ChatCompletions),
        "anthropic_messages" => Some(hermes_agent::agent_loop::ApiMode::AnthropicMessages),
        "codex_responses" => Some(hermes_agent::agent_loop::ApiMode::CodexResponses),
        "bedrock_converse" => Some(hermes_agent::agent_loop::ApiMode::BedrockConverse),
        _ => None,
    }
}

fn parse_bool_env(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn default_pet_settings() -> PetSettings {
    let mut settings = PetSettings::default();
    if let Ok(raw) = std::env::var("HERMES_PET") {
        if let Some(enabled) = parse_bool_env(&raw) {
            settings.enabled = enabled;
        }
    }
    if let Ok(raw) = std::env::var("HERMES_PET_SPECIES") {
        settings.species = raw;
    }
    if let Ok(raw) = std::env::var("HERMES_PET_MOOD") {
        settings.mood = raw;
    }
    if let Ok(raw) = std::env::var("HERMES_PET_DOCK") {
        settings.dock = if raw.trim().eq_ignore_ascii_case("left") {
            PetDock::Left
        } else {
            PetDock::Right
        };
    }
    if let Ok(raw) = std::env::var("HERMES_PET_TICK_MS") {
        if let Ok(value) = raw.trim().parse::<u64>() {
            settings.tick_ms = value;
        }
    }
    settings.normalized()
}

fn load_pet_settings() -> PetSettings {
    let path = pet_settings_path();
    let from_file = std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str::<PetSettings>(&raw).ok())
        .map(PetSettings::normalized);
    from_file.unwrap_or_else(default_pet_settings)
}

fn persist_pet_settings(settings: &PetSettings) -> Result<(), AgentError> {
    let path = pet_settings_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            AgentError::Io(format!(
                "Failed to create pet settings directory '{}': {}",
                parent.display(),
                e
            ))
        })?;
    }
    let body = serde_json::to_string_pretty(settings)
        .map_err(|e| AgentError::Config(format!("pet settings serialization failed: {e}")))?;
    std::fs::write(&path, format!("{body}\n")).map_err(|e| {
        AgentError::Io(format!(
            "Failed to persist pet settings '{}': {}",
            path.display(),
            e
        ))
    })
}

pub(super) fn default_rtk_raw_mode() -> bool {
    match std::env::var("HERMES_RTK_RAW") {
        Ok(value) => matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "on" | "yes"
        ),
        Err(_) => false,
    }
}

fn provider_base_url_from_env(provider: &str) -> Option<String> {
    let env_var = match provider.trim().to_ascii_lowercase().as_str() {
        "ollama-local" | "ollama" => "OLLAMA_BASE_URL",
        "llama-cpp" | "llama.cpp" | "llamacpp" => "LLAMA_CPP_BASE_URL",
        "vllm" | "ollvm" | "llvm" => "VLLM_BASE_URL",
        "mlx" | "mlx-lm" | "apple-mlx" => "MLX_BASE_URL",
        "apple-ane" | "ane" | "apple-neural-engine" => "APPLE_ANE_BASE_URL",
        "sglang" => "SGLANG_BASE_URL",
        "tgi" | "text-generation-inference" => "TGI_BASE_URL",
        _ => return None,
    };
    std::env::var(env_var)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn provider_is_local_backend(provider: &str) -> bool {
    matches!(
        provider.trim().to_ascii_lowercase().as_str(),
        "ollama-local" | "llama-cpp" | "vllm" | "mlx" | "apple-ane" | "sglang" | "tgi"
    )
}

pub(crate) fn allow_no_api_key(
    provider_name: &str,
    runtime_provider: &str,
    base_url: Option<&str>,
) -> bool {
    provider_is_local_backend(runtime_provider)
        || provider_is_local_backend(provider_name)
        || base_url.is_some_and(url_is_local_or_private)
}

fn url_is_local_or_private(base_url: &str) -> bool {
    let trimmed = base_url.trim();
    let no_scheme = trimmed
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(trimmed);
    let authority = no_scheme.split('/').next().unwrap_or(no_scheme).trim();
    let host = if authority.starts_with('[') {
        authority
            .find(']')
            .map(|idx| authority[1..idx].to_string())
            .unwrap_or_else(|| authority.trim_matches(&['[', ']'][..]).to_string())
    } else {
        authority
            .split(':')
            .next()
            .unwrap_or(authority)
            .trim()
            .to_string()
    }
    .to_ascii_lowercase();

    if host == "localhost" {
        return true;
    }

    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return match ip {
            std::net::IpAddr::V4(v4) => v4.is_loopback() || v4.is_private() || v4.is_link_local(),
            std::net::IpAddr::V6(v6) => v6.is_loopback() || v6.is_unique_local(),
        };
    }
    false
}

/// Resolve API key / token for a named LLM provider from well-known environment variables.
pub fn provider_api_key_from_env(provider: &str) -> Option<String> {
    let provider = normalize_runtime_provider_name(provider);
    match provider.as_str() {
        "openai" => std::env::var("HERMES_OPENAI_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| std::env::var("OPENAI_API_KEY").ok())
            .filter(|s| !s.trim().is_empty()),
        "openai-codex" | "codex" => std::env::var("HERMES_OPENAI_CODEX_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty()),
        "anthropic" | "claude" | "claude-code" => std::env::var("ANTHROPIC_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| std::env::var("ANTHROPIC_TOKEN").ok())
            .filter(|s| !s.trim().is_empty())
            .or_else(|| std::env::var("CLAUDE_CODE_OAUTH_TOKEN").ok())
            .filter(|s| !s.trim().is_empty()),
        "google-gemini-cli" | "gemini-cli" | "gemini-oauth" => {
            std::env::var("HERMES_GEMINI_OAUTH_API_KEY")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .or_else(|| std::env::var("GOOGLE_API_KEY").ok())
                .filter(|s| !s.trim().is_empty())
                .or_else(|| std::env::var("GEMINI_API_KEY").ok())
                .filter(|s| !s.trim().is_empty())
        }
        "openrouter" => std::env::var("OPENROUTER_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty()),
        "qwen" => std::env::var("DASHSCOPE_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty()),
        "qwen-oauth" => std::env::var("HERMES_QWEN_OAUTH_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| std::env::var("DASHSCOPE_API_KEY").ok())
            .filter(|s| !s.trim().is_empty()),
        "kimi" | "moonshot" => std::env::var("KIMI_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| std::env::var("KIMI_CODING_API_KEY").ok())
            .filter(|s| !s.trim().is_empty())
            .or_else(|| std::env::var("MOONSHOT_API_KEY").ok())
            .filter(|s| !s.trim().is_empty())
            .or_else(|| std::env::var("KIMI_CN_API_KEY").ok())
            .filter(|s| !s.trim().is_empty()),
        "minimax" => std::env::var("MINIMAX_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| std::env::var("MINIMAX_CN_API_KEY").ok())
            .filter(|s| !s.trim().is_empty()),
        "stepfun" => std::env::var("HERMES_STEPFUN_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| std::env::var("STEPFUN_API_KEY").ok())
            .filter(|s| !s.trim().is_empty()),
        "nous" => std::env::var("NOUS_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty()),
        "copilot" => std::env::var("GITHUB_COPILOT_TOKEN")
            .ok()
            .filter(|s| !s.trim().is_empty()),
        "ai-gateway" => std::env::var("AI_GATEWAY_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty()),
        "arcee" => std::env::var("ARCEEAI_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| std::env::var("ARCEE_API_KEY").ok())
            .filter(|s| !s.trim().is_empty()),
        "deepseek" => std::env::var("DEEPSEEK_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty()),
        "huggingface" => std::env::var("HF_TOKEN")
            .ok()
            .filter(|s| !s.trim().is_empty()),
        "kilocode" => std::env::var("KILOCODE_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty()),
        "nvidia" => std::env::var("NVIDIA_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty()),
        "ollama-cloud" => std::env::var("OLLAMA_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty()),
        "ollama-local" => std::env::var("OLLAMA_LOCAL_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| std::env::var("OLLAMA_API_KEY").ok())
            .filter(|s| !s.trim().is_empty()),
        "llama-cpp" => std::env::var("LLAMA_CPP_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty()),
        "vllm" => std::env::var("VLLM_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty()),
        "mlx" => std::env::var("MLX_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty()),
        "apple-ane" => std::env::var("APPLE_ANE_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty()),
        "sglang" => std::env::var("SGLANG_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty()),
        "tgi" => std::env::var("TGI_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| std::env::var("HUGGINGFACE_API_KEY").ok())
            .filter(|s| !s.trim().is_empty()),
        "opencode-go" => std::env::var("OPENCODE_GO_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty()),
        "opencode-zen" => std::env::var("OPENCODE_ZEN_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty()),
        "xai" => std::env::var("XAI_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty()),
        "xiaomi" => std::env::var("XIAOMI_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty()),
        "zai" => std::env::var("GLM_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| std::env::var("ZAI_API_KEY").ok())
            .filter(|s| !s.trim().is_empty())
            .or_else(|| std::env::var("Z_AI_API_KEY").ok())
            .filter(|s| !s.trim().is_empty()),
        _ => None,
    }
}

pub fn build_provider(config: &GatewayConfig, model: &str) -> Arc<dyn LlmProvider> {
    let (provider_name, model_name) = resolve_provider_and_model(config, model);
    let runtime_provider = normalize_runtime_provider_name(provider_name.as_str());

    let provider_config = config
        .llm_providers
        .get(provider_name.as_str())
        .or_else(|| config.llm_providers.get(runtime_provider.as_str()));
    let provider_config = provider_config.or_else(|| {
        config.llm_providers.iter().find_map(|(name, cfg)| {
            if name.eq_ignore_ascii_case(provider_name.as_str())
                || name.eq_ignore_ascii_case(runtime_provider.as_str())
            {
                Some(cfg)
            } else {
                None
            }
        })
    });

    let default_base_url = provider_default_base_url(provider_name.as_str())
        .or_else(|| provider_default_base_url(runtime_provider.as_str()));
    let base_url = provider_config
        .and_then(|c| c.base_url.clone())
        .or_else(|| provider_base_url_from_env(provider_name.as_str()))
        .or_else(|| provider_base_url_from_env(runtime_provider.as_str()))
        .or_else(|| default_base_url.map(ToString::to_string));

    let api_key = provider_config
        .and_then(|c| c.api_key.as_deref())
        .and_then(resolve_api_key_literal_or_env_ref)
        .or_else(|| {
            provider_config
                .and_then(|c| c.api_key_env.as_deref())
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .and_then(|name| std::env::var(name).ok())
                .filter(|v| !v.trim().is_empty())
        })
        .or_else(|| provider_api_key_from_env(provider_name.as_str()))
        .or_else(|| provider_api_key_from_env(runtime_provider.as_str()));

    let local_no_key_ok = allow_no_api_key(
        provider_name.as_str(),
        runtime_provider.as_str(),
        base_url.as_deref(),
    );

    let api_key = match api_key {
        Some(k) => k,
        None if local_no_key_ok => "local-no-key".to_string(),
        None => {
            tracing::warn!(
                provider = %provider_name,
                runtime_provider = %runtime_provider,
                model = %model,
                impact = "llm requests will fail until a valid API key is configured",
                "No API key for provider; using NoBackendProvider"
            );
            return Arc::new(NoBackendProvider {
                model: model.to_string(),
            });
        }
    };

    let cache_key = provider_cache_key(
        runtime_provider.as_str(),
        model_name.as_str(),
        base_url.as_deref(),
        &api_key,
    );
    {
        let mut guard = provider_cache().lock().unwrap();
        if let Some(entry) = guard.get_mut(&cache_key) {
            entry.last_used = Instant::now();
            return entry.provider.clone();
        }
    }

    let built: Arc<dyn LlmProvider> = match runtime_provider.as_str() {
        "openai" => {
            let mut p = OpenAiProvider::new(&api_key).with_model(model_name.as_str());
            if let Some(url) = base_url.clone() {
                p = p.with_base_url(url);
            }
            Arc::new(p) as Arc<dyn LlmProvider>
        }
        "openai-codex" | "codex" => {
            let mut p = OpenAiProvider::new(&api_key).with_model(model_name.as_str());
            p = p.with_base_url(
                base_url
                    .clone()
                    .unwrap_or_else(|| OPENAI_CODEX_BASE_URL.to_string()),
            );
            Arc::new(p) as Arc<dyn LlmProvider>
        }
        "anthropic" => {
            let mut p = AnthropicProvider::new(&api_key).with_model(model_name.as_str());
            if let Some(url) = base_url.clone() {
                p = p.with_base_url(url);
            }
            Arc::new(p) as Arc<dyn LlmProvider>
        }
        "openrouter" => {
            let p = OpenRouterProvider::new(&api_key).with_model(model_name.as_str());
            Arc::new(p) as Arc<dyn LlmProvider>
        }
        "qwen" | "qwen-oauth" => {
            let mut p = QwenProvider::new(&api_key).with_model(model_name.as_str());
            if let Some(url) = base_url.clone() {
                p = p.with_base_url(url);
            }
            Arc::new(p) as Arc<dyn LlmProvider>
        }
        "kimi" | "moonshot" => {
            let mut p = KimiProvider::new(&api_key).with_model(model_name.as_str());
            if let Some(url) = base_url.clone() {
                p = p.with_base_url(url);
            }
            Arc::new(p) as Arc<dyn LlmProvider>
        }
        "minimax" => {
            let mut p = MiniMaxProvider::new(&api_key).with_model(model_name.as_str());
            if let Some(url) = base_url.clone() {
                p = p.with_base_url(url);
            }
            Arc::new(p) as Arc<dyn LlmProvider>
        }
        "stepfun" => {
            let url = base_url
                .clone()
                .unwrap_or_else(|| STEPFUN_BASE_URL.to_string());
            Arc::new(GenericProvider::new(url, &api_key, model_name.as_str()))
                as Arc<dyn LlmProvider>
        }
        "nous" => {
            let mut p = NousProvider::new(&api_key).with_model(model_name.as_str());
            if let Some(url) = base_url.clone() {
                p = p.with_base_url(url);
            }
            Arc::new(p) as Arc<dyn LlmProvider>
        }
        "copilot" => {
            let p = CopilotProvider::new(
                base_url
                    .clone()
                    .unwrap_or_else(|| "https://api.github.com/copilot".to_string()),
                &api_key,
            )
            .with_model(model_name.as_str());
            Arc::new(p) as Arc<dyn LlmProvider>
        }
        "ollama-local" | "llama-cpp" | "vllm" | "mlx" | "apple-ane" | "sglang" | "tgi" => {
            let url = base_url
                .clone()
                .unwrap_or_else(|| "http://127.0.0.1:11434/v1".to_string());
            Arc::new(GenericProvider::new(url, &api_key, model_name.as_str()))
                as Arc<dyn LlmProvider>
        }
        _ => {
            let url = base_url
                .clone()
                .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
            Arc::new(GenericProvider::new(url, &api_key, model_name.as_str()))
                as Arc<dyn LlmProvider>
        }
    };
    {
        let mut guard = provider_cache().lock().unwrap();
        guard.insert(
            cache_key,
            ProviderCacheEntry {
                provider: built.clone(),
                last_used: Instant::now(),
            },
        );
        prune_provider_cache(&mut guard);
    }
    built
}

const PROVIDER_CACHE_MAX_SIZE: usize = 128;
const PROVIDER_CACHE_IDLE_TTL: Duration = Duration::from_secs(3600);

struct ProviderCacheEntry {
    provider: Arc<dyn LlmProvider>,
    last_used: Instant,
}

pub(crate) fn provider_cache()
-> &'static StdMutex<std::collections::HashMap<String, ProviderCacheEntry>> {
    static CACHE: OnceLock<StdMutex<std::collections::HashMap<String, ProviderCacheEntry>>> =
        OnceLock::new();
    CACHE.get_or_init(|| StdMutex::new(std::collections::HashMap::new()))
}

pub(crate) fn clear_provider_cache() {
    provider_cache().lock().unwrap().clear();
}

pub(crate) fn provider_cache_key(
    runtime_provider: &str,
    model_name: &str,
    base_url: Option<&str>,
    api_key: &str,
) -> String {
    format!(
        "{}|{}|{}|{}",
        runtime_provider,
        model_name,
        base_url.unwrap_or(""),
        api_key
    )
}

fn prune_provider_cache(cache: &mut std::collections::HashMap<String, ProviderCacheEntry>) {
    let now = Instant::now();
    cache.retain(|_, entry| now.duration_since(entry.last_used) <= PROVIDER_CACHE_IDLE_TTL);
    if cache.len() <= PROVIDER_CACHE_MAX_SIZE {
        return;
    }
    let mut entries: Vec<(String, Instant)> = cache
        .iter()
        .map(|(k, v)| (k.clone(), v.last_used))
        .collect();
    entries.sort_by_key(|(_, used)| *used);
    let overflow = cache.len().saturating_sub(PROVIDER_CACHE_MAX_SIZE);
    for (key, _) in entries.into_iter().take(overflow) {
        cache.remove(&key);
    }
}

// ---------------------------------------------------------------------------
// NoBackendProvider — explicit fallback when no API key is configured
// ---------------------------------------------------------------------------

pub(crate) struct NoBackendProvider {
    pub(crate) model: String,
}

#[async_trait::async_trait]
impl LlmProvider for NoBackendProvider {
    async fn chat_completion(
        &self,
        _messages: &[hermes_core::Message],
        _tools: &[hermes_core::ToolSchema],
        _max_tokens: Option<u32>,
        _temperature: Option<f64>,
        _model: Option<&str>,
        _extra_body: Option<&Value>,
    ) -> Result<hermes_core::LlmResponse, AgentError> {
        Err(AgentError::LlmApi(format!(
            "NoBackendProvider: no LLM backend configured for model '{}'. \
             Configure an API key and provider in the config file.",
            self.model
        )))
    }

    fn chat_completion_stream(
        &self,
        _messages: &[hermes_core::Message],
        _tools: &[hermes_core::ToolSchema],
        _max_tokens: Option<u32>,
        _temperature: Option<f64>,
        _model: Option<&str>,
        _extra_body: Option<&Value>,
    ) -> futures::stream::BoxStream<'static, Result<hermes_core::StreamChunk, AgentError>> {
        futures::stream::once(async move {
            Err(AgentError::LlmApi(
                "NoBackendProvider: no LLM backend configured for streaming.".to_string(),
            ))
        })
        .boxed()
    }
}
