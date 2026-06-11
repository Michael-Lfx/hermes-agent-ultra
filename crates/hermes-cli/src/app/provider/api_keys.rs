use super::names::normalize_runtime_provider_name;
use super::urls::{provider_is_local_backend, url_is_local_or_private};

pub(super) fn resolve_api_key_literal_or_env_ref(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(env_ref) = trimmed.strip_prefix("${").and_then(|s| s.strip_suffix('}')) {
        return std::env::var(env_ref).ok().filter(|v| !v.trim().is_empty());
    }
    Some(trimmed.to_string())
}

pub(super) fn parse_runtime_provider_api_mode(
    value: &str,
) -> Option<hermes_agent::agent_loop::ApiMode> {
    match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
        "chat_completions" => Some(hermes_agent::agent_loop::ApiMode::ChatCompletions),
        "anthropic_messages" => Some(hermes_agent::agent_loop::ApiMode::AnthropicMessages),
        "codex_responses" => Some(hermes_agent::agent_loop::ApiMode::CodexResponses),
        "bedrock_converse" => Some(hermes_agent::agent_loop::ApiMode::BedrockConverse),
        _ => None,
    }
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
