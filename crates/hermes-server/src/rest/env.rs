use axum::{
    extract::State,
    Json,
};
use serde::Deserialize;
use serde_json::json;

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

/// GET /api/env - List environment variables (masked)
pub async fn list_env(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let env_path = state.env_path();
    
    if !env_path.exists() {
        return Ok(ok_json(json!({ "vars": [] })));
    }
    
    let content = tokio::fs::read_to_string(&env_path)
        .await
        .map_err(|e| AppError::Internal(format!("read .env: {}", e)))?;
    
    let vars = parse_env_content(&content);
    let masked_vars: Vec<serde_json::Value> = vars
        .into_iter()
        .map(|(key, value)| {
            json!({
                "key": key,
                "value": mask_value(&key, &value),
            })
        })
        .collect();
    
    Ok(ok_json(json!({ "vars": masked_vars })))
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
