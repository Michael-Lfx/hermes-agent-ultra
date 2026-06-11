use axum::{extract::State, Json};
use serde_json::json;
use std::collections::HashMap;

use crate::{error::AppError, state::AppState};

/// Parse YAML frontmatter from a markdown file.
/// Expected format:
/// ```markdown
/// ---
/// name: my-skill
/// description: A test skill
/// enabled: true
/// ---
///
/// # Instructions
/// ...
/// ```
fn parse_frontmatter(content: &str) -> Option<HashMap<String, String>> {
    if !content.starts_with("---") {
        return None;
    }
    
    let end = content.find("\n---\n")?;
    let yaml_str = &content[3..end];
    serde_yaml::from_str(yaml_str).ok()
}

/// Scan a directory for skill files (*.md with YAML frontmatter).
fn scan_skills_dir(skills_dir: &std::path::Path) -> Vec<serde_json::Value> {
    let mut skills = Vec::new();
    
    if let Ok(entries) = std::fs::read_dir(skills_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Some(frontmatter) = parse_frontmatter(&content) {
                        let name = frontmatter.get("name")
                            .cloned()
                            .unwrap_or_else(|| {
                                path.file_stem()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .to_string()
                            });
                        
                        skills.push(json!({
                            "name": name,
                            "description": frontmatter.get("description").cloned().unwrap_or_default(),
                            "enabled": frontmatter.get("enabled")
                                .and_then(|v| v.parse::<bool>().ok())
                                .unwrap_or(true),
                            "category": frontmatter.get("category").cloned().unwrap_or_else(|| "general".to_string()),
                        }));
                    }
                }
            }
        }
    }
    
    skills
}

/// GET /api/skills - List all skills
///
/// Scans the skills directory for markdown files with YAML frontmatter.
pub async fn get_skills(State(state): State<AppState>) -> Result<Json<serde_json::Value>, AppError> {
    // Try to find skills directory
    let skills_dir = state.hermes_home.join("skills");
    
    let skills = scan_skills_dir(&skills_dir);
    
    Ok(Json(json!({
        "skills": skills,
        "directory": skills_dir.to_string_lossy(),
    })))
}

/// Update the `enabled` field in YAML frontmatter of a skill file.
fn update_frontmatter_enabled(content: &str, enabled: bool) -> Option<String> {
    if !content.starts_with("---") {
        return None;
    }
    
    let end = content.find("\n---\n")?;
    let yaml_str = &content[3..end];
    let mut frontmatter: HashMap<String, String> = serde_yaml::from_str(yaml_str).ok()?;
    
    frontmatter.insert("enabled".to_string(), enabled.to_string());
    
    let new_yaml = serde_yaml::to_string(&frontmatter).ok()?;
    let rest = &content[end + 5..];
    Some(format!("---\n{}---\n{}", new_yaml, rest))
}

/// PUT /api/skills/toggle - Enable/disable a skill
///
/// Finds the skill file by name and updates its YAML frontmatter `enabled` field.
pub async fn toggle_skill(
    State(state): State<AppState>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    let skill_name = payload["name"].as_str()
        .ok_or_else(|| AppError::BadRequest("Missing skill name".to_string()))?;
    let enabled = payload["enabled"].as_bool()
        .ok_or_else(|| AppError::BadRequest("Missing enabled flag".to_string()))?;

    let skills_dir = state.hermes_home.join("skills");
    
    // Find the skill file by name
    let mut found_file = None;
    if let Ok(entries) = std::fs::read_dir(&skills_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Some(frontmatter) = parse_frontmatter(&content) {
                        let name = frontmatter.get("name")
                            .cloned()
                            .unwrap_or_else(|| {
                                path.file_stem()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .to_string()
                            });
                        if name == skill_name {
                            found_file = Some((path, content));
                            break;
                        }
                    }
                }
            }
        }
    }
    
    match found_file {
        Some((path, content)) => {
            match update_frontmatter_enabled(&content, enabled) {
                Some(new_content) => {
                    tokio::fs::write(&path, new_content)
                        .await
                        .map_err(|e| AppError::Internal(format!("write skill file: {}", e)))?;
                    
                    tracing::info!(skill = %skill_name, enabled = enabled, "skill toggled successfully");
                    Ok(Json(json!({
                        "status": "ok",
                        "skill": skill_name,
                        "enabled": enabled,
                    })))
                }
                None => {
                    Err(AppError::Internal("Failed to update skill frontmatter".to_string()))
                }
            }
        }
        None => {
            Err(AppError::NotFound(format!("Skill '{}' not found", skill_name)))
        }
    }
}
