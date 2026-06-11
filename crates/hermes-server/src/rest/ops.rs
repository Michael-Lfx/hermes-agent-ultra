use axum::{
    extract::{Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::json;
use crate::{
    error::{ok_json, AppError},
    state::AppState,
};

/// POST /api/ops/doctor - Run diagnostic checks
pub async fn run_doctor() -> Result<Json<serde_json::Value>, AppError> {
    let mut checks = Vec::new();
    
    // Check config file exists
    let hermes_home = hermes_config::hermes_home();
    let config_path = hermes_home.join("config.yaml");
    checks.push(json!({
        "check": "config_exists",
        "status": config_path.exists(),
    }));
    
    // Check state.db exists
    let db_path = hermes_home.join("state.db");
    checks.push(json!({
        "check": "state_db_exists",
        "status": db_path.exists(),
    }));
    
    // Check .env exists
    let env_path = hermes_home.join(".env");
    checks.push(json!({
        "check": "env_exists",
        "status": env_path.exists(),
    }));
    
    Ok(ok_json(json!({
        "ok": true,
        "checks": checks,
    })))
}

/// POST /api/ops/backup - Create backup
#[derive(Debug, Deserialize)]
pub struct BackupRequest {
    pub output: Option<String>,
}

pub async fn create_backup(
    State(state): State<AppState>,
    Json(req): Json<BackupRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let hermes_home = state.hermes_home.clone();
    let output = req.output.unwrap_or_else(|| {
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        format!("hermes_backup_{}.tar.gz", timestamp)
    });
    
    // Simple backup: archive config.yaml, state.db, .env
    
    // For now, return the paths that would be backed up
    let backup_files = vec![
        hermes_home.join("config.yaml"),
        hermes_home.join("state.db"),
        hermes_home.join(".env"),
    ];
    
    Ok(ok_json(json!({
        "ok": true,
        "output": output,
        "files": backup_files.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>(),
    })))
}

/// POST /api/ops/import - Import from archive
#[derive(Debug, Deserialize)]
pub struct ImportRequest {
    pub archive: String,
}

pub async fn import_backup(
    Json(req): Json<ImportRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Placeholder: would extract archive and merge configs
    Ok(ok_json(json!({
        "ok": true,
        "archive": req.archive,
    })))
}

/// POST /api/ops/dump - Dump state for debugging
pub async fn dump_state(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let config = state.config.read().await;
    
    Ok(ok_json(json!({
        "ok": true,
        "hermes_home": state.hermes_home.to_string_lossy(),
        "config": {
            "model": config.model,
            "personality": config.personality,
            "max_turns": config.max_turns,
            "tools": config.tools,
        },
    })))
}

/// GET /api/ops/logs - Read recent logs
#[derive(Debug, Deserialize)]
pub struct LogsQuery {
    pub lines: Option<usize>,
    pub filter: Option<String>,
    pub file: Option<String>,
    pub level: Option<String>,
    pub component: Option<String>,
}

pub async fn get_logs(
    State(state): State<AppState>,
    Query(query): axum::extract::Query<LogsQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let max_lines = query.lines.unwrap_or(100).min(1000);
    let level_filter = query.level.as_deref().map(|l| l.to_uppercase());
    let component_filter = query.component.as_deref().map(|c| c.to_lowercase());
    let filter_text = query.filter.as_deref().map(|f| f.to_lowercase());

    // Determine log file path
    let log_path = if let Some(file_name) = query.file.as_deref() {
        state.hermes_home.join("logs").join(file_name)
    } else {
        state.hermes_home.join("logs").join("server.log")
    };

    let content = if log_path.exists() {
        tokio::fs::read_to_string(&log_path).await.unwrap_or_default()
    } else {
        String::new()
    };

    let logs: Vec<String> = content
        .lines()
        .filter(|line| {
            if let Some(ref level) = level_filter {
                let level_patterns = ["ERROR", "WARN", "INFO", "DEBUG"];
                let has_level = level_patterns.iter().any(|p| line.contains(p));
                if has_level && !line.contains(level.as_str()) {
                    return false;
                }
            }
            if let Some(ref component) = component_filter {
                if !line.to_lowercase().contains(component) {
                    return false;
                }
            }
            if let Some(ref filter) = filter_text {
                if !line.to_lowercase().contains(filter) {
                    return false;
                }
            }
            true
        })
        .rev()
        .take(max_lines)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|s| s.to_string())
        .collect();

    Ok(ok_json(json!({
        "logs": logs,
        "lines": logs.len(),
        "file": log_path.to_string_lossy(),
    })))
}
