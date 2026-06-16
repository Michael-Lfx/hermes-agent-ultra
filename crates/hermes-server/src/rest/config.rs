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

const CATEGORY_ORDER: &[&str] = &[
    "general", "agent", "terminal", "display", "delegation",
    "memory", "compression", "security", "browser", "voice",
    "tts", "stt", "logging", "discord", "auxiliary",
];

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_ascii_uppercase().to_string() + chars.as_str(),
    }
}

/// Recursively walk a config Value and build flat `dot-path → {type, description, category}`.
/// Mirrors Python _build_schema_from_config in web_server.py.
fn build_schema(config: &Value, prefix: &str, fields: &mut HashMap<String, Value>) {
    match config {
        Value::Object(map) => {
            for (key, val) in map {
                let full_key = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                let category = if prefix.is_empty() {
                    if val.is_object() { key.clone() } else { "general".to_string() }
                } else {
                    prefix.split('.').next().unwrap_or("general").to_string()
                };
                build_schema(val, &full_key, fields);
                // Add a placeholder entry so the category key still appears in the UI
                if val.is_object() {
                    let display_name = capitalize(key);
                    let entry = json!({
                        "type": "object",
                        "description": display_name,
                        "category": category,
                    });
                    fields.entry(full_key.clone()).or_insert(entry);
                }
            }
        }
        other => {
            let field_type = match other {
                Value::Bool(_) => "boolean",
                Value::Number(_) => "number",
                Value::String(_) => "string",
                Value::Array(_) => "list",
                _ => "string",
            };
            let category = prefix.split('.').next().unwrap_or("general").to_string();
            let name = prefix.rsplit('.').next().unwrap_or(prefix);
            let display_name = capitalize(&name.replace('_', " "));
            fields.insert(prefix.to_string(), json!({
                "type": field_type,
                "description": display_name,
                "category": category,
            }));
        }
    }
}

/// GET /api/config - Get current configuration
pub async fn get_config(State(state): State<AppState>) -> Result<Json<serde_json::Value>, AppError> {
    let config = state.config.read().await;
    let config_json = serde_json::to_value(&*config)
        .map_err(|e| AppError::Internal(format!("serialize config: {}", e)))?;
    Ok(ok_json(config_json))
}

/// PUT /api/config - Save configuration
#[derive(Debug, Deserialize)]
pub struct ConfigUpdate {
    pub config: serde_json::Value,
}

pub async fn put_config(
    State(state): State<AppState>,
    Json(update): Json<ConfigUpdate>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Deserialize the new config
    let new_config: hermes_config::GatewayConfig = serde_json::from_value(update.config)
        .map_err(|e| AppError::BadRequest(format!("invalid config: {}", e)))?;
    
    // Validate
    hermes_config::validate_config(&new_config)
        .map_err(|e| AppError::Config(format!("validation failed: {}", e)))?;
    
    // Save to file
    let config_path = state.config_path();
    hermes_config::save_config_yaml(&config_path, &new_config)?;
    
    // Update in-memory state
    {
        let mut config = state.config.write().await;
        *config = new_config;
    }
    
    // Invalidate cached AgentLoop instances so they are rebuilt with new config
    state.invalidate_agent_caches().await;
    
    Ok(ok_json(json!({ "ok": true })))
}

/// GET /api/config/defaults - Get default configuration
pub async fn get_defaults() -> Result<Json<serde_json::Value>, AppError> {
    let default_config = hermes_config::GatewayConfig::default();
    let config_json = serde_json::to_value(default_config)
        .map_err(|e| AppError::Internal(format!("serialize defaults: {}", e)))?;
    Ok(ok_json(config_json))
}

/// Embedded DEFAULT_CONFIG structure mirroring Python config.py DEFAULT_CONFIG.
/// The schema generator walks this tree to produce the full config schema.
const DEFAULT_CONFIG_JSON: &str = r#"{
  "model": "",
  "toolsets": ["hermes-cli"],
  "file_read_max_chars": 100000,
  "timezone": "",
  "prefill_messages_file": "",
  "paste_collapse_threshold": 5,
  "paste_collapse_threshold_fallback": 5,
  "paste_collapse_char_threshold": 2000,
  "agent": {
    "max_turns": 90,
    "service_tier": "",
    "image_input_mode": "auto",
    "tool_use_enforcement": "auto",
    "task_completion_guidance": true,
    "environment_probe": true,
    "environment_hint": "",
    "coding_context": "auto",
    "gateway_timeout": 1800,
    "clarify_timeout": 600,
    "disabled_toolsets": []
  },
  "terminal": {
    "backend": "local",
    "cwd": ".",
    "timeout": 180,
    "persistent_shell": true,
    "env_passthrough": [],
    "docker_image": "nikolaik/python-nodejs:python3.11-nodejs20",
    "singularity_image": "docker://nikolaik/python-nodejs:python3.11-nodejs20",
    "modal_image": "nikolaik/python-nodejs:python3.11-nodejs20",
    "daytona_image": "nikolaik/python-nodejs:python3.11-nodejs20",
    "auto_source_bashrc": true
  },
  "display": {
    "skin": "default",
    "language": "en",
    "personality": "",
    "compact": false,
    "tool_progress_command": false,
    "tool_progress": "",
    "show_reasoning": false,
    "show_cost": false,
    "streaming": false,
    "timestamps": false,
    "final_response_markdown": "strip",
    "persistent_output": true,
    "persistent_output_max_lines": 200,
    "inline_diffs": true,
    "bell_on_complete": false,
    "busy_input_mode": "interrupt",
    "resume_display": "full",
    "resume_exchanges": 10,
    "interface": "cli",
    "copy_shortcut": "auto"
  },
  "security": {
    "allow_private_urls": false,
    "redact_secrets": true,
    "allow_lazy_installs": true
  },
  "approvals": {
    "mode": "manual",
    "timeout": 60,
    "mcp_reload_confirm": true,
    "destructive_slash_confirm": true
  },
  "compression": {
    "enabled": true,
    "threshold": 0.50,
    "target_ratio": 0.20,
    "protect_last_n": 20,
    "protect_first_n": 3,
    "abort_on_summary_failure": false
  },
  "memory": {
    "memory_enabled": true,
    "user_profile_enabled": true,
    "memory_char_limit": 2200,
    "user_char_limit": 1375,
    "provider": "",
    "write_approval": false
  },
  "context": { "engine": "compressor" },
  "voice": {
    "record_key": "ctrl+b",
    "max_recording_seconds": 120,
    "auto_tts": false,
    "beep_enabled": true,
    "silence_threshold": 200,
    "silence_duration": 3.0
  },
  "tts": {
    "provider": "edge",
    "edge": { "voice": "en-US-AriaNeural" },
    "openai": { "model": "gpt-4o-mini-tts", "voice": "alloy" },
    "elevenlabs": { "voice_id": "pNInz6obpgDQGcFmaJgB", "model_id": "eleven_multilingual_v2" },
    "gemini": { "model": "gemini-2.5-flash-preview-tts", "voice": "Kore" },
    "xai": { "voice_id": "eve", "language": "en", "sample_rate": 24000 },
    "mistral": { "model": "voxtral-mini-tts-2603" },
    "piper": { "voice": "en_US-lessac-medium" }
  },
  "stt": {
    "enabled": true,
    "provider": "local",
    "local": { "model": "base", "language": "" },
    "openai": { "model": "whisper-1" },
    "elevenlabs": { "model_id": "scribe_v2", "language_code": "" }
  },
  "delegation": {
    "max_concurrent_children": 3,
    "max_spawn_depth": 1,
    "orchestrator_enabled": true,
    "child_timeout_seconds": 600,
    "max_iterations": 50,
    "inherit_mcp_toolsets": true,
    "reasoning_effort": ""
  },
  "tool_output": {
    "max_bytes": 50000,
    "max_lines": 2000,
    "max_line_length": 2000
  },
  "checkpoints": {
    "enabled": false,
    "max_snapshots": 20,
    "max_total_size_mb": 500,
    "auto_prune": true,
    "retention_days": 7
  },
  "logging": { "level": "INFO", "max_size_mb": 5, "backup_count": 3 },
  "web": { "backend": "", "search_backend": "", "extract_backend": "" },
  "browser": {
    "engine": "auto",
    "allow_private_urls": false,
    "auto_local_for_private_urls": true,
    "record_sessions": false,
    "inactivity_timeout": 120,
    "command_timeout": 30
  },
  "skills": {
    "external_dirs": [],
    "template_vars": true,
    "inline_shell": false,
    "inline_shell_timeout": 10,
    "write_approval": false
  },
  "curator": {
    "enabled": true,
    "interval_hours": 168,
    "min_idle_hours": 2,
    "stale_after_days": 30,
    "archive_after_days": 90,
    "backup": { "enabled": true, "keep": 5 }
  },
  "cron": { "wrap_response": true },
  "kanban": { "dispatch_in_gateway": true, "dispatch_interval_seconds": 60, "failure_limit": 2 }
}"#;

/// Python _CATEGORY_MERGE: fold small categories into larger ones.
const CATEGORY_MERGE: &[(&str, &str)] = &[
    ("privacy", "security"), ("context", "agent"), ("skills", "agent"),
    ("cron", "agent"), ("network", "agent"), ("checkpoints", "agent"),
    ("approvals", "security"), ("human_delay", "display"), ("dashboard", "display"),
    ("code_execution", "agent"), ("prompt_caching", "agent"), ("goals", "agent"),
    ("tools", "agent"), ("openrouter", "agent"),
];

/// GET /api/config/schema - Get configuration schema
///
/// Walks the embedded DEFAULT_CONFIG tree to produce a flat dot-path → field schema dict,
/// matching Python _build_schema_from_config behavior.
pub async fn get_schema() -> Result<Json<serde_json::Value>, AppError> {
    let config_val: Value = serde_json::from_str(DEFAULT_CONFIG_JSON).unwrap_or_default();
    let mut fields = HashMap::new();
    build_schema(&config_val, "", &mut fields);

    // Apply category merge: fold small categories into larger ones
    for entry in fields.values_mut() {
        if let Some(cat) = entry.get("category").and_then(|v| v.as_str()) {
            for &(from, to) in CATEGORY_MERGE {
                if cat == from {
                    entry["category"] = Value::String(to.to_string());
                    break;
                }
            }
        }
    }

    Ok(ok_json(json!({
        "fields": fields,
        "category_order": CATEGORY_ORDER,
    })))
}

/// GET /api/config/raw - Get raw YAML configuration
pub async fn get_raw_config(State(state): State<AppState>) -> Result<Json<serde_json::Value>, AppError> {
    let config_path = state.config_path();
    let yaml_content = tokio::fs::read_to_string(&config_path)
        .await
        .map_err(|e| AppError::Internal(format!("read config: {}", e)))?;
    
    Ok(ok_json(json!({
        "yaml": yaml_content,
    })))
}

/// PUT /api/config/raw - Save raw YAML configuration
#[derive(Debug, Deserialize)]
pub struct RawConfigUpdate {
    pub yaml: String,
}

pub async fn put_raw_config(
    State(state): State<AppState>,
    Json(update): Json<RawConfigUpdate>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Parse to validate
    let new_config: hermes_config::GatewayConfig = serde_yaml::from_str(&update.yaml)?;
    
    // Validate
    hermes_config::validate_config(&new_config)?;
    
    // Save raw YAML
    let config_path = state.config_path();
    tokio::fs::write(&config_path, update.yaml)
        .await
        .map_err(|e| AppError::Internal(format!("write config: {}", e)))?;
    
    // Update in-memory state
    {
        let mut config = state.config.write().await;
        *config = new_config;
    }
    
    // Invalidate cached AgentLoop instances so they are rebuilt with new config
    state.invalidate_agent_caches().await;
    
    Ok(ok_json(json!({ "ok": true })))
}
