pub(super) const STEPFUN_BASE_URL: &str = "https://api.stepfun.ai/step_plan/v1";
pub(super) const OPENAI_CODEX_BASE_URL: &str = "https://chatgpt.com/backend-api/codex";
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

pub(super) fn provider_default_base_url(provider: &str) -> Option<&'static str> {
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

pub(super) fn provider_base_url_from_env(provider: &str) -> Option<String> {
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

pub(super) fn provider_is_local_backend(provider: &str) -> bool {
    matches!(
        provider.trim().to_ascii_lowercase().as_str(),
        "ollama-local" | "llama-cpp" | "vllm" | "mlx" | "apple-ane" | "sglang" | "tgi"
    )
}

pub(super) fn url_is_local_or_private(base_url: &str) -> bool {
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
