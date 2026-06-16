use std::collections::HashMap;

use axum::{
    extract::State,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    error::{ok_json, AppError},
    state::AppState,
};

/// Parse .env file content into key-value pairs.
fn parse_env_content(content: &str) -> Vec<( String, String ) > {
    content
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let mut parts = line.splitn(2, '=');
            let key = parts.next()?.trim();
            let value = parts.next().unwrap_or("").trim();
            Some((key.to_string(), value.to_string()))
        })
        .collect()
}

/// Write key-value pairs back to .env file format.
fn write_env_content(vars: &[(String, String)]) -> String {
    vars.iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Mask sensitive values.
fn mask_value(key: &str, value: &str) -> String {
    let sensitive_keys = ["key", "token", "secret", "password", "api_key", "auth"];
    let lower_key = key.to_lowercase();
    if sensitive_keys.iter().any(|&s| lower_key.contains(s)) {
        if value.len() > 8 {
            format!("{}****", &value[..4])
        } else {
            "****".to_string()
        }
    } else {
        value.to_string()
    }
}

/// Check if a key is for sensitive values.
fn is_sensitive_key(key: &str) -> bool {
    let sensitive_keys = ["key", "token", "secret", "password", "api_key", "auth"];
    let lower_key = key.to_lowercase();
    sensitive_keys.iter().any(|&s| lower_key.contains(s))
}

/// Known env vars with metadata. Mirrors Python OPTIONAL_ENV_VARS.
/// (key, description, url, category, is_password, tools)
const KNOWN_ENV_VARS: &[(&str, &str, &str, &str, bool, &[&str])] = &[
    // Provider keys
    ("NOUS_BASE_URL", "Nous Portal base URL override", "", "provider", false, &[]),
    ("OPENROUTER_API_KEY", "OpenRouter API key", "https://openrouter.ai/keys", "provider", true, &["vision_analyze"]),
    ("GOOGLE_API_KEY", "Google AI Studio API key", "https://aistudio.google.com/app/apikey", "provider", true, &[]),
    ("GEMINI_API_KEY", "Gemini API key (alias for GOOGLE_API_KEY)", "https://aistudio.google.com/app/apikey", "provider", true, &[]),
    ("XAI_API_KEY", "xAI API key", "https://console.x.ai/", "provider", true, &[]),
    ("NVIDIA_API_KEY", "NVIDIA NIM API key", "https://build.nvidia.com/", "provider", true, &[]),
    ("MISTRAL_API_KEY", "Mistral API key", "https://console.mistral.ai/", "provider", true, &[]),
    ("GROQ_API_KEY", "Groq API key", "https://console.groq.com/keys", "provider", true, &[]),
    ("CEREBRAS_API_KEY", "Cerebras API key", "https://cloud.cerebras.ai/", "provider", true, &[]),
    ("DEEPSEEK_API_KEY", "DeepSeek API key", "https://platform.deepseek.com/", "provider", true, &[]),
    ("TOGETHER_API_KEY", "Together AI API key", "https://api.together.xyz/settings/api-keys", "provider", true, &[]),
    ("FIREWORKS_API_KEY", "Fireworks AI API key", "https://fireworks.ai/keys", "provider", true, &[]),
    ("SILICONFLOW_API_KEY", "SiliconFlow API key", "https://cloud.siliconflow.cn/", "provider", true, &[]),
    ("MINIMAX_API_KEY", "MiniMax API key", "", "provider", true, &[]),
    ("KIMI_API_KEY", "Kimi/Moonshot API key", "https://platform.moonshot.cn/", "provider", true, &[]),
    ("HUGGINGFACE_API_KEY", "HuggingFace API key", "https://huggingface.co/settings/tokens", "provider", true, &[]),
    ("OPENAI_API_KEY", "OpenAI API key", "https://platform.openai.com/", "provider", true, &[]),
    ("ANTHROPIC_API_KEY", "Anthropic API key", "https://console.anthropic.com/", "provider", true, &[]),
    // Tool keys
    ("EXA_API_KEY", "Exa search API key", "https://exa.ai/", "tool", true, &["web_search"]),
    ("BRAVE_API_KEY", "Brave search API key", "https://brave.com/search/api/", "tool", true, &["web_search"]),
    ("FIRECRAWL_API_KEY", "Firecrawl web scraping API key", "https://firecrawl.dev/", "tool", true, &["web_extract"]),
    ("FIRECRAWL_API_URL", "Firecrawl API base URL", "https://api.firecrawl.dev/v1", "tool", false, &["web_extract"]),
    ("ELEVENLABS_API_KEY", "ElevenLabs API key", "https://elevenlabs.io/app/settings/api-keys", "tool", true, &["text_to_speech"]),
    ("BROWSERBASE_API_KEY", "Browserbase API key", "https://browserbase.com/settings", "tool", true, &["browser"]),
    ("BROWSERBASE_PROJECT_ID", "Browserbase project ID", "https://browserbase.com/settings", "tool", false, &["browser"]),
    ("CAMOFOX_URL", "Camofox URL", "https://camofox.dev/", "tool", false, &["browser"]),
    ("FAL_API_KEY", "Fal.ai API key", "https://fal.ai/dashboard/keys", "tool", true, &["image_generate"]),
    ("TWILIO_ACCOUNT_SID", "Twilio account SID", "https://console.twilio.com/", "messaging", true, &[]),
    ("TWILIO_AUTH_TOKEN", "Twilio auth token", "https://console.twilio.com/", "messaging", true, &[]),
    ("TWILIO_PHONE_NUMBER", "Twilio phone number", "https://console.twilio.com/phone-numbers", "messaging", false, &[]),
    ("GATEWAY_ALLOW_ALL_USERS", "Allow all users to access gateway", "", "setting", false, &[]),
    ("SUDO_PASSWORD", "Sudo password for privileged commands", "", "setting", true, &[]),
    ("HERMES_TOOL_PROGRESS", "Tool progress display mode", "", "setting", false, &[]),
    ("AGENT_BROWSER_ENGINE", "Agent browser engine selection", "", "setting", false, &[]),
];

/// GET /api/env - List environment variables (masked)
///
/// Returns Record<String, EnvVarInfo> format matching Python OPTIONAL_ENV_VARS.
pub async fn list_env(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let env_path = state.env_path();
    
    // Read actual .env values
    let actual_vars: HashMap<String, String> = if env_path.exists() {
        let content = tokio::fs::read_to_string(&env_path)
            .await
            .map_err(|e| AppError::Internal(format!("read .env: {}", e)))?;
        parse_env_content(&content).into_iter().collect()
    } else {
        HashMap::new()
    };
    
    let mut result: serde_json::Map<String, Value> = serde_json::Map::new();
    
    // Add all known env vars from KNOWN_ENV_VARS
    for &(key, description, url, category, is_password, tools) in KNOWN_ENV_VARS {
        let is_set = actual_vars.contains_key(key);
        let redacted_value = if is_set {
            Value::String(mask_value(key, actual_vars[key].as_str()))
        } else {
            Value::Null
        };
        result.insert(key.to_string(), json!({
            "is_set": is_set,
            "redacted_value": redacted_value,
            "description": description,
            "url": if url.is_empty() { Value::Null } else { Value::String(url.to_string()) },
            "category": category,
            "is_password": is_password,
            "tools": tools,
            "advanced": false,
            "channel_managed": false,
        }));
    }
    
    // Merge any extra vars from .env that aren't in KNOWN_ENV_VARS
    let known_keys: std::collections::HashSet<&str> = KNOWN_ENV_VARS.iter().map(|(k, ..)| *k).collect();
    for (key, value) in &actual_vars {
        if !known_keys.contains(key.as_str()) {
            result.insert(key.clone(), json!({
                "is_set": true,
                "redacted_value": Value::String(mask_value(key, value)),
                "description": "",
                "category": "tool",
                "is_password": is_sensitive_key(key),
                "tools": [],
                "advanced": false,
                "channel_managed": false,
            }));
        }
    }
    
    Ok(ok_json(Value::Object(result)))
}

/// PUT /api/env - Set environment variable
#[derive(Debug, Deserialize)]
pub struct SetEnvVar {
    pub key: String,
    pub value: String,
}

pub async fn set_env_var(
    State(state): State<AppState>,
    Json(update): Json<SetEnvVar>,
) -> Result<Json<serde_json::Value>, AppError> {
    let env_path = state.env_path();
    
    // Read existing content
    let mut vars = if env_path.exists() {
        let content = tokio::fs::read_to_string(&env_path)
            .await
            .map_err(|e| AppError::Internal(format!("read .env: {}", e)))?;
        parse_env_content(&content)
    } else {
        Vec::new()
    };
    
    // Update or insert
    let mut found = false;
    for (k, v) in &mut vars {
        if k == &update.key {
            *v = update.value.clone();
            found = true;
            break;
        }
    }
    if !found {
        vars.push((update.key.clone(), update.value.clone()));
    }
    
    // Write back
    let content = write_env_content(&vars);
    tokio::fs::write(&env_path, content)
        .await
        .map_err(|e| AppError::Internal(format!("write .env: {}", e)))?;
    
    // Set in current process (unsafe in Rust 2024)
    unsafe {
        std::env::set_var(&update.key, &update.value);
    }
    
    Ok(ok_json(json!({
        "key": update.key,
        "ok": true,
    })))
}

/// DELETE /api/env - Delete environment variable
#[derive(Debug, Deserialize)]
pub struct DeleteEnvVar {
    pub key: String,
}

pub async fn delete_env_var(
    State(state): State<AppState>,
    Json(update): Json<DeleteEnvVar>,
) -> Result<Json<serde_json::Value>, AppError> {
    let env_path = state.env_path();
    
    if !env_path.exists() {
        return Err(AppError::NotFound(format!("env var {} not found", update.key)));
    }
    
    let content = tokio::fs::read_to_string(&env_path)
        .await
        .map_err(|e| AppError::Internal(format!("read .env: {}", e)))?;
    
    let mut vars = parse_env_content(&content);
    let original_len = vars.len();
    vars.retain(|(k, _)| k != &update.key);
    
    if vars.len() == original_len {
        return Err(AppError::NotFound(format!("env var {} not found", update.key)));
    }
    
    // Write back
    let content = write_env_content(&vars);
    tokio::fs::write(&env_path, content)
        .await
        .map_err(|e| AppError::Internal(format!("write .env: {}", e)))?;
    
    // Remove from current process (unsafe in Rust 2024)
    unsafe {
        std::env::remove_var(&update.key);
    }
    
    Ok(ok_json(json!({
        "key": update.key,
        "deleted": true,
    })))
}

/// POST /api/env/reveal - Reveal real value of an env var
#[derive(Debug, Deserialize)]
pub struct RevealEnvVar {
    pub key: String,
}

pub async fn reveal_env_var(
    State(state): State<AppState>,
    Json(reveal): Json<RevealEnvVar>,
) -> Result<Json<serde_json::Value>, AppError> {
    let env_path = state.env_path();
    
    if !env_path.exists() {
        return Err(AppError::NotFound(format!("env var {} not found", reveal.key)));
    }
    
    let content = tokio::fs::read_to_string(&env_path)
        .await
        .map_err(|e| AppError::Internal(format!("read .env: {}", e)))?;
    
    let vars = parse_env_content(&content);
    
    for (k, v) in vars {
        if k == reveal.key {
            return Ok(ok_json(json!({
                "key": k,
                "value": v,
            })));
        }
    }
    
    Err(AppError::NotFound(format!("env var {} not found", reveal.key)))
}
