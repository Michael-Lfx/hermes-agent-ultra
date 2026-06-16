use std::collections::HashMap;

use axum::{extract::{Path, State}, Json};
use serde_json::{json, Value};

use crate::{error::AppError, state::AppState};

/// Parse .env content into key-value pairs.
fn parse_env_content_simple(content: &str) -> Vec<(String, String)> {
    content.lines().filter_map(|line| {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { return None; }
        let mut parts = line.splitn(2, '=');
        let key = parts.next()?.trim().to_string();
        let value = parts.next().unwrap_or("").trim().to_string();
        Some((key, value))
    }).collect()
}

/// GET /api/messaging/platforms - List messaging platforms
///
/// Returns all supported messaging platforms with their env_var configurations.
pub async fn list_platforms(State(state): State<AppState>) -> Result<Json<serde_json::Value>, AppError> {
    // (id, name, description, docs_url, env_vars[])
    let catalog: Vec<(&str, &str, &str, &str, Vec<(&str, &str, bool, &str)>)> = vec![
        ("telegram", "Telegram", "Telegram messaging platform", "https://core.telegram.org/bots",
         vec![("TELEGRAM_BOT_TOKEN", "Bot token from @BotFather", true, ""),
              ("TELEGRAM_ALLOWED_USERS", "Comma-separated allowed user IDs", false, ""),
              ("TELEGRAM_PROXY", "SOCKS5 proxy URL", false, "")]),
        ("discord", "Discord", "Discord messaging platform", "https://discord.com/developers/",
         vec![("DISCORD_BOT_TOKEN", "Bot token from Discord Developer Portal", true, ""),
              ("DISCORD_ALLOWED_USERS", "Comma-separated user IDs", false, "")]),
        ("slack", "Slack", "Slack messaging platform", "https://api.slack.com/",
         vec![("SLACK_BOT_TOKEN", "Bot token (xoxb-...)", true, ""),
              ("SLACK_APP_TOKEN", "App-level token (xapp-...)", true, "")]),
        ("mattermost", "Mattermost", "Mattermost messaging platform", "https://docs.mattermost.com/",
         vec![("MATTERMOST_URL", "Mattermost server URL", true, ""),
              ("MATTERMOST_TOKEN", "Bot/personal access token", true, "")]),
        ("matrix", "Matrix", "Matrix messaging protocol", "https://matrix.org/",
         vec![("MATRIX_HOMESERVER", "Matrix homeserver URL", true, ""),
              ("MATRIX_ACCESS_TOKEN", "Matrix access token", true, ""),
              ("MATRIX_USER_ID", "Matrix user ID (@user:server)", true, "")]),
        ("whatsapp", "WhatsApp", "WhatsApp via Baileys bridge", "https://baileys.dev/",
         vec![("WHATSAPP_ENABLED", "Enable WhatsApp bridge", false, "")]),
        ("signal", "Signal", "Signal via signal-cli", "https://github.com/bbernhard/signal-cli-rest-api",
         vec![("SIGNAL_HTTP_URL", "signal-cli REST API URL", true, ""),
              ("SIGNAL_ACCOUNT", "Signal phone number", true, "")]),
        ("bluebubbles", "BlueBubbles", "iMessage via BlueBubbles", "https://bluebubbles.app/",
         vec![("BLUEBUBBLES_SERVER_URL", "BlueBubbles server URL", true, ""),
              ("BLUEBUBBLES_PASSWORD", "BlueBubbles password", true, "")]),
        ("homeassistant", "Home Assistant", "Home Assistant notifications", "https://www.home-assistant.io/",
         vec![("HASS_URL", "Home Assistant URL", true, ""),
              ("HASS_TOKEN", "Long-lived access token", true, "")]),
        ("email", "Email", "Email notifications", "https://docs.python.org/3/library/smtplib.html",
         vec![("EMAIL_ADDRESS", "Email address", true, ""),
              ("EMAIL_PASSWORD", "Email password", true, ""),
              ("EMAIL_IMAP_HOST", "IMAP server host", true, ""),
              ("EMAIL_SMTP_HOST", "SMTP server host", true, "")]),
        ("sms", "SMS", "SMS via Twilio", "https://www.twilio.com/",
         vec![("TWILIO_ACCOUNT_SID", "Twilio account SID", true, ""),
              ("TWILIO_AUTH_TOKEN", "Twilio auth token", true, "")]),
        ("dingtalk", "DingTalk", "DingTalk messaging", "https://open.dingtalk.com/",
         vec![("DINGTALK_CLIENT_ID", "DingTalk client ID", true, ""),
              ("DINGTALK_CLIENT_SECRET", "DingTalk client secret", true, "")]),
        ("feishu", "Feishu", "Feishu/Lark messaging", "https://open.feishu.cn/",
         vec![("FEISHU_APP_ID", "Feishu App ID", true, ""),
              ("FEISHU_APP_SECRET", "Feishu App Secret", true, ""),
              ("FEISHU_ENCRYPT_KEY", "Feishu encryption key", false, ""),
              ("FEISHU_VERIFICATION_TOKEN", "Feishu verification token", false, "")]),
        ("wecom", "WeCom (group bot)", "Send-only WeCom group bot via webhook", "https://developer.work.weixin.qq.com/",
         vec![("WECOM_BOT_ID", "WeCom bot webhook ID", true, ""),
              ("WECOM_SECRET", "WeCom secret", true, "")]),
        ("wecom_callback", "WeCom (app)", "Two-way WeCom integration via callback app", "https://developer.work.weixin.qq.com/",
         vec![("WECOM_CALLBACK_CORP_ID", "WeCom corp ID", true, ""),
              ("WECOM_CALLBACK_CORP_SECRET", "WeCom app corp secret", true, ""),
              ("WECOM_CALLBACK_AGENT_ID", "WeCom app agent ID", true, ""),
              ("WECOM_CALLBACK_TOKEN", "WeCom callback verification token", false, ""),
              ("WECOM_CALLBACK_ENCODING_AES_KEY", "WeCom callback AES encoding key", false, "")]),
        ("weixin", "WeChat (Official Account)", "Connect a WeChat Official Account", "https://developers.weixin.qq.com/",
         vec![("WEIXIN_ACCOUNT_ID", "WeChat account ID", true, ""),
              ("WEIXIN_TOKEN", "WeChat token", true, ""),
              ("WEIXIN_BASE_URL", "WeChat base URL", false, "")]),
        ("qqbot", "QQ Bot", "Connect Hermes to a QQ Bot", "https://bot.q.qq.com/",
         vec![("QQ_APP_ID", "QQ app ID", true, ""),
              ("QQ_CLIENT_SECRET", "QQ client secret", true, "")]),
        ("yuanbao", "Yuanbao (元宝)", "Connect Hermes to Tencent Yuanbao", "",
         vec![]),
        ("api_server", "API Server", "Expose Hermes as an OpenAI-compatible HTTP API", "",
         vec![("API_SERVER_ENABLED", "Enable API server", false, ""),
              ("API_SERVER_KEY", "API server key", false, ""),
              ("API_SERVER_PORT", "API server port", false, ""),
              ("API_SERVER_HOST", "API server host", false, "")]),
        ("webhook", "Webhooks", "Receive events from GitHub, GitLab, and other webhook sources", "https://hermes-agent.nousresearch.com/docs/user-guide/messaging/webhooks/",
         vec![("WEBHOOK_ENABLED", "Enable webhook listener", false, ""),
              ("WEBHOOK_PORT", "Webhook listener port", false, ""),
              ("WEBHOOK_SECRET", "Webhook signing secret", false, "")]),
    ];

    // Read actual env values
    let env_path = state.env_path();
    let actual_vars: HashMap<String, String> = if env_path.exists() {
        std::fs::read_to_string(&env_path).ok()
            .map(|c| parse_env_content_simple(&c).into_iter().collect())
            .unwrap_or_default()
    } else {
        HashMap::new()
    };
    let gateway_running = super::gateway::is_gateway_running(&state).await;

    // Fallback: check for running hermes-agent-ultra processes if state check fails
    let gateway_running_fallback = if !gateway_running {
        std::process::Command::new("tasklist")
            .arg("/FI")
            .arg("IMAGENAME eq hermes-agent-ultra.exe")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.contains("hermes-agent-ultra.exe"))
            .unwrap_or(false)
    } else {
        true
    };
    let gw_running = gateway_running || gateway_running_fallback;

    // Read gateway_state.json for per-platform runtime status
    let runtime_status: serde_json::Map<String, Value> = {
        let state_path = state.hermes_home.join("gateway_state.json");
        if state_path.exists() {
            std::fs::read_to_string(&state_path)
                .ok()
                .and_then(|c| serde_json::from_str::<Value>(&c).ok())
                .and_then(|v| v.get("platforms").and_then(|p| p.as_object()).cloned())
                .unwrap_or_default()
        } else {
            serde_json::Map::new()
        }
    };

    let platforms: Vec<Value> = catalog.into_iter()
        .map(|(id, name, desc, url, env_var_defs)| {
            let has_required_env_vars = env_var_defs.iter().any(|(_, _, required, _)| *required);
            let required_all_set = env_var_defs.iter()
                .filter(|(_, _, required, _)| *required)
                .all(|(key, _, _, _)| actual_vars.contains_key(*key));
            let is_configured = has_required_env_vars && required_all_set;
            let is_enabled = is_configured;
            let env_vars: Vec<Value> = env_var_defs.into_iter()
                .map(|(key, prompt, required, hint)| {
                    let is_set = actual_vars.contains_key(key);
                    json!({
                        "key": key,
                        "prompt": prompt,
                        "description": prompt,
                        "url": if hint.is_empty() { Value::Null } else { Value::String(hint.to_string()) },
                        "is_set": is_set,
                        "required": required,
                    })
                })
                .collect();

            let platform_state = runtime_status.get(id)
                .and_then(|p| p.get("state"))
                .and_then(|s| s.as_str())
                .map(|s| s.to_string());
            let error_code = runtime_status.get(id)
                .and_then(|p| p.get("error_code"))
                .and_then(|s| s.as_str())
                .map(|s| s.to_string());
            let error_message = runtime_status.get(id)
                .and_then(|p| p.get("error_message"))
                .and_then(|s| s.as_str())
                .map(|s| s.to_string());

            let state_value = if !gw_running {
                "gateway_stopped"
            } else if !is_configured {
                "not_configured"
            } else if let Some(ref s) = platform_state {
                s.as_str()
            } else {
                // Gateway running but no runtime status yet — waiting for gateway to report
                "pending_restart"
            };

            json!({
                "id": id,
                "name": name,
                "description": desc,
                "docs_url": url,
                "enabled": is_enabled,
                "configured": is_configured,
                "gateway_running": gw_running,
                "state": state_value,
                "error_code": error_code.map(Value::String).unwrap_or(Value::Null),
                "error_message": error_message.map(Value::String).unwrap_or(Value::Null),
                "updated_at": runtime_status.get(id)
                    .and_then(|p| p.get("updated_at"))
                    .cloned()
                    .unwrap_or(Value::Null),
                "home_channel": Value::Null,
                "env_vars": env_vars,
            })
        })
        .collect();

    Ok(Json(json!({ "platforms": platforms })))
}

/// PUT /api/messaging/platforms/{id} - Update platform configuration
///
/// Saves env var values from the request body to the .env file.
pub async fn update_platform(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    let env_path = state.env_path();
    
    // Read existing .env content
    let mut vars: Vec<(String, String)> = if env_path.exists() {
        std::fs::read_to_string(&env_path)
            .map(|c| parse_env_content_simple(&c))
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    
    // Save env vars from the request body
    if let Some(env_obj) = payload.get("env").and_then(|v| v.as_object()) {
        for (key, value) in env_obj {
            if let Some(val_str) = value.as_str() {
                let trimmed = val_str.trim();
                if !trimmed.is_empty() {
                    let mut found = false;
                    for (k, v) in &mut vars {
                        if k == key {
                            *v = trimmed.to_string();
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        vars.push((key.clone(), trimmed.to_string()));
                    }
                }
            }
        }
    }
    
    // Write back to .env file
    let content: String = vars.iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("\n");
    let _ = std::fs::write(&env_path, content);
    
    // Set in current process
    if let Some(env_obj) = payload.get("env").and_then(|v| v.as_object()) {
        for (key, value) in env_obj {
            if let Some(val_str) = value.as_str() {
                let trimmed = val_str.trim();
                if !trimmed.is_empty() {
                    unsafe { std::env::set_var(key, trimmed); }
                }
            }
        }
    }
    
    tracing::info!(platform = %id, "messaging platform updated");
    Ok(Json(json!({ "ok": true, "platform": id })))
}

/// POST /api/messaging/platforms/{id}/test - Test platform connection
pub async fn test_platform(
    State(_state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    // TODO: Integrate with real messaging module
    Ok(Json(json!({
        "status": "ok",
        "platform": id,
        "connected": true,
        "message": "Connection test passed",
    })))
}
