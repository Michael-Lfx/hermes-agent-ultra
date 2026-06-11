//! CLI subcommand handlers (dispatched from main.rs / dispatch.rs).

use std::process::Stdio;
use std::sync::Arc;
use std::{
    collections::{HashMap, HashSet},
    fmt::Write as _,
    io::Write as _,
    path::{Path, PathBuf},
    time::SystemTime,
};

use hermes_agent::{
    RunConversationParams, plugins::PluginManifest, split_messages_for_run_conversation,
};
use hermes_core::AgentError;
use hermes_tools::tools::messaging::MessagingSessionContext;
use regex::Regex;
use serde::Deserialize;

use super::model::{
    rank_catalog_model_candidates, resolve_catalog_model_candidate, split_provider_model,
};
use super::session;
use super::skills_infra::{SENTRUX_MCP_ARG, SENTRUX_MCP_COMMAND, SENTRUX_MCP_SERVER_NAME};
use super::{mask_secret_value, secret_stdout_allowed, yes_no};
use crate::model_switch::{normalize_provider_model, provider_model_ids};
// ---------------------------------------------------------------------------
// CLI subcommand handlers (dispatched from main.rs)
// ---------------------------------------------------------------------------

pub(crate) fn resolve_cli_chat_provider_model(
    config_model: Option<&str>,
    model_override: Option<&str>,
    provider_override: Option<&str>,
) -> Result<String, AgentError> {
    let provider_override = provider_override
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| v.to_ascii_lowercase());
    let model_override = model_override.map(str::trim).filter(|v| !v.is_empty());

    let mut current_model = config_model
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or("gpt-4o")
        .to_string();

    if let Some(model) = model_override {
        current_model = model.to_string();
    } else if provider_override.is_none() {
        if let Ok(model_env) = std::env::var("HERMES_INFERENCE_MODEL") {
            let model_env = model_env.trim();
            if !model_env.is_empty() {
                current_model = model_env.to_string();
            }
        }
    }
    if let Some(provider) = provider_override.as_deref() {
        if let Some((_, model_name)) = current_model.split_once(':') {
            current_model = format!("{provider}:{}", model_name.trim());
        } else {
            current_model = format!("{provider}:{}", current_model.trim());
        }
    }
    if !current_model.contains(':') {
        current_model = normalize_provider_model(&current_model)?;
    }
    Ok(current_model)
}

pub(crate) fn apply_cli_chat_runtime_env(provider_model: &str) {
    let provider_model = provider_model.trim();
    if provider_model.is_empty() {
        return;
    }
    crate::env_vars::set_var("HERMES_MODEL", provider_model);
    crate::env_vars::set_var("HERMES_INFERENCE_MODEL", provider_model);
    if let Some((provider, _)) = provider_model.split_once(':') {
        let provider = provider.trim();
        if !provider.is_empty() {
            crate::env_vars::set_var("HERMES_INFERENCE_PROVIDER", provider);
            if std::env::var_os("HERMES_TUI_PROVIDER").is_some() {
                crate::env_vars::set_var("HERMES_TUI_PROVIDER", provider);
            }
        }
    }
}

const QUERY_ALLOW_TOOLS_ENV_KEY: &str = "HERMES_QUERY_ALLOW_TOOLS";
const QUERY_DISABLE_TOOLS_ENV_KEY: &str = "HERMES_QUERY_DISABLE_TOOLS";

pub(crate) fn query_mode_tools_enabled(query_mode: bool, allow_tools_flag: bool) -> bool {
    if !query_mode {
        return true;
    }
    if allow_tools_flag {
        return true;
    }
    if hermes_config::env_var_enabled(QUERY_DISABLE_TOOLS_ENV_KEY) {
        return false;
    }
    // Backward compatible explicit-enable override (now redundant with default-on).
    if hermes_config::env_var_enabled(QUERY_ALLOW_TOOLS_ENV_KEY) {
        return true;
    }
    true
}

fn query_mode_model_not_found(err: &hermes_core::AgentError) -> bool {
    let msg = err.to_string().to_ascii_lowercase();
    (msg.contains("model") && msg.contains("not found"))
        || msg.contains("requested model does not exist")
        || msg.contains("openrouter catalog")
}

async fn query_mode_remediation_target(provider_model: &str) -> Option<(String, Vec<String>)> {
    let (provider, model_id) = split_provider_model(provider_model);
    let provider = provider.trim().to_ascii_lowercase();
    if provider.is_empty() || model_id.trim().is_empty() {
        return None;
    }
    let catalog = provider_model_ids(&provider).await;
    if catalog.is_empty() {
        return None;
    }
    let close = rank_catalog_model_candidates(model_id.trim(), &catalog, 5);
    let selected = resolve_catalog_model_candidate(model_id.trim(), &catalog)
        .or_else(|| close.first().cloned())
        .or_else(|| catalog.first().cloned())?;
    let next = format!("{}:{}", provider, selected.trim());
    if next.eq_ignore_ascii_case(provider_model) {
        return None;
    }
    Some((next, close))
}

/// Handle `hermes chat [--query ...] [--preload-skill ...] [--yolo]`.
pub async fn handle_cli_chat(
    query: Option<String>,
    preload_skill: Option<String>,
    yolo: bool,
    model_override: Option<String>,
    provider_override: Option<String>,
    allow_tools_flag: bool,
) -> Result<(), hermes_core::AgentError> {
    use crate::runtime_tool_wiring::{wire_cron_scheduler_backend, wire_stdio_clarify_backend};
    use crate::terminal_backend::build_terminal_backend;
    use crate::tool_preview::{build_tool_preview_from_value, tool_emoji};
    use hermes_config::load_config;
    use hermes_core::MessageRole;
    use hermes_cron::cron_scheduler_for_data_dir;
    use hermes_skills::{FileSkillStore, SkillManager};
    use hermes_tools::ToolRegistry;

    if let Some(skill) = &preload_skill {
        println!("[Preloading skill: {}]", skill);
    }
    if yolo {
        println!("[YOLO mode: tool confirmations disabled]");
    }

    let mut config =
        load_config(None).map_err(|e| hermes_core::AgentError::Config(e.to_string()))?;

    if yolo {
        config.approval.require_approval = false;
    }

    let query_mode = query.is_some();
    let tools_enabled = query_mode_tools_enabled(query_mode, allow_tools_flag);
    if query_mode && !tools_enabled {
        println!(
            "[Query mode tools are disabled by {}=1. Unset it or pass --allow-tools to re-enable.]",
            QUERY_DISABLE_TOOLS_ENV_KEY
        );
    }

    let current_model = resolve_cli_chat_provider_model(
        config.model.as_deref(),
        model_override.as_deref(),
        provider_override.as_deref(),
    )?;
    apply_cli_chat_runtime_env(&current_model);

    let tool_registry = Arc::new(ToolRegistry::new());
    let tool_schemas = if tools_enabled {
        let terminal_backend = build_terminal_backend(&config);
        let skill_store = Arc::new(FileSkillStore::new(FileSkillStore::default_dir()));
        let skill_provider: Arc<dyn hermes_core::SkillProvider> =
            Arc::new(SkillManager::new(skill_store));
        hermes_tools::register_builtin_tools(&tool_registry, terminal_backend, skill_provider);
        let live_count =
            crate::live_messaging::enable_live_messaging_tool(&config, &tool_registry).await;
        if live_count > 0 {
            println!(
                "[send_message live delivery enabled via {} configured adapter(s)]",
                live_count
            );
        }
        wire_stdio_clarify_backend(&tool_registry);
        let cron_data_dir = hermes_config::cron_dir();
        std::fs::create_dir_all(&cron_data_dir)
            .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;
        let cron_scheduler = Arc::new(cron_scheduler_for_data_dir(cron_data_dir));
        cron_scheduler
            .load_persisted_jobs()
            .await
            .map_err(|e| hermes_core::AgentError::Config(format!("cron load: {e}")))?;
        cron_scheduler.start().await;
        wire_cron_scheduler_backend(
            &tool_registry,
            cron_scheduler,
            MessagingSessionContext::new(),
        );
        crate::platform_toolsets::resolve_platform_tool_schemas(&config, "cli", &tool_registry)
    } else {
        Vec::new()
    };
    let agent_tool_registry = Arc::new(crate::app::bridge_tool_registry(&tool_registry));

    let build_query_agent = |provider_model: &str| {
        let on_tool_start: Box<dyn Fn(&str, &serde_json::Value) + Send + Sync> =
            Box::new(move |name: &str, args: &serde_json::Value| {
                let emoji = tool_emoji(name);
                let preview = build_tool_preview_from_value(name, args, 56).unwrap_or_default();
                if preview.is_empty() {
                    println!("┊ {emoji} {name}");
                } else {
                    println!("┊ {emoji} {name:<16} {preview}");
                }
            });
        let on_tool_complete: Box<dyn Fn(&str, &str) + Send + Sync> =
            Box::new(move |name: &str, result: &str| {
                let mut snippet: String = result.trim().chars().take(96).collect();
                if result.trim().chars().count() > 96 {
                    snippet.push_str("...");
                }
                let emoji = tool_emoji(name);
                if snippet.is_empty() {
                    println!("┊ {emoji} {name:<16} done");
                } else {
                    println!("┊ {emoji} {name:<16} done: {snippet}");
                }
            });
        let callbacks = hermes_agent::AgentCallbacks {
            on_tool_start: Some(on_tool_start),
            on_tool_complete: Some(on_tool_complete),
            ..Default::default()
        };
        let agent_config = crate::app::build_agent_config(&config, provider_model);
        let provider = crate::app::build_provider(&config, provider_model);
        let base =
            hermes_agent::AgentLoop::new(agent_config, Arc::clone(&agent_tool_registry), provider)
                .with_async_tool_dispatch(crate::app::async_tool_dispatch_for(
                    tool_registry.clone(),
                ))
                .with_callbacks(callbacks);
        if query_mode {
            hermes_agent::attach_discovered_plugins(base)
        } else {
            hermes_agent::attach_agent_runtime(base)
        }
    };

    match query {
        Some(q) => {
            let mut active_model = current_model.clone();
            if let Some((next_model, close)) = query_mode_remediation_target(&active_model).await {
                println!(
                    "[Model remediation: {} -> {}. Close matches: {}]",
                    active_model,
                    next_model,
                    if close.is_empty() {
                        "(none)".to_string()
                    } else {
                        close.join(", ")
                    }
                );
                active_model = next_model;
            }
            apply_cli_chat_runtime_env(&active_model);
            let agent = build_query_agent(&active_model);
            let result = match agent
                .run_conversation(RunConversationParams {
                    user_message: q.clone(),
                    conversation_history: vec![],
                    task_id: None,
                    stream_callback: None,
                    persist_user_message: None,
                    tools: Some(tool_schemas.clone()),
                    persist_session: false,
                })
                .await
            {
                Ok(conv) => conv.into_loop_result(),
                Err(err) => {
                    if query_mode_model_not_found(&err) {
                        if let Some((next_model, close)) =
                            query_mode_remediation_target(&active_model).await
                        {
                            return Err(hermes_core::AgentError::Config(format!(
                                "{}\nModel remediation suggestion: {} -> {} (close matches: {})",
                                err,
                                active_model,
                                next_model,
                                if close.is_empty() {
                                    "(none)".to_string()
                                } else {
                                    close.join(", ")
                                }
                            )));
                        }
                    }
                    return Err(err);
                }
            };

            let reply = result
                .messages
                .iter()
                .rev()
                .find_map(|m| {
                    if m.role == MessageRole::Assistant {
                        m.content.clone()
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| "(no assistant reply)".to_string());
            println!("{}", reply);
        }
        None => {
            println!("Starting interactive chat session...");
            println!("(Use `hermes` for the default interactive TUI)");
        }
    }
    Ok(())
}

/// Handle `hermes skills [action] [name] [--extra ...]`.
// handle_cli_skills moved to skills.rs

// ---------------------------------------------------------------------------
// Plugin discovery / surface rendering
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PluginSurfaceSource {
    User,
    Project,
    Entrypoint,
}

impl PluginSurfaceSource {
    fn label(&self) -> &'static str {
        match self {
            PluginSurfaceSource::User => "user",
            PluginSurfaceSource::Project => "project",
            PluginSurfaceSource::Entrypoint => "entrypoint",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PluginSurfaceEntry {
    name: String,
    version: String,
    description: String,
    kind: Option<String>,
    source: PluginSurfaceSource,
    path: Option<PathBuf>,
    enabled: bool,
    entrypoint_value: Option<String>,
    entrypoint_dist: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PythonEntrypointPayload {
    #[serde(default)]
    entries: Vec<PythonEntrypointItem>,
}

#[derive(Debug, Deserialize)]
struct PythonEntrypointItem {
    name: String,
    value: String,
    #[serde(default)]
    dist: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PythonPluginCommandPayload {
    #[serde(default)]
    commands: Vec<PythonPluginCommandItem>,
}

#[derive(Debug, Deserialize, Clone)]
struct PythonPluginCommandItem {
    name: String,
    #[serde(default)]
    help: String,
}

fn coerce_memory_provider_kind(path: &Path, kind: Option<String>) -> Option<String> {
    let explicit_kind = kind
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string);
    if explicit_kind.is_some() {
        return explicit_kind;
    }
    let init_file = path.join("__init__.py");
    let Ok(source) = std::fs::read_to_string(&init_file) else {
        return None;
    };
    let probe = if source.len() > 8192 {
        &source[..8192]
    } else {
        source.as_str()
    };
    if probe.contains("register_memory_provider") || probe.contains("MemoryProvider") {
        Some("exclusive".to_string())
    } else {
        None
    }
}

fn scan_plugin_manifest_root(root: &Path, source: PluginSurfaceSource) -> Vec<PluginSurfaceEntry> {
    let mut out = Vec::new();
    if !root.exists() {
        return out;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let manifest_path = path.join("plugin.yaml");
        if !manifest_path.exists() {
            continue;
        }
        let content = match std::fs::read_to_string(&manifest_path) {
            Ok(content) => content,
            Err(_) => continue,
        };
        let manifest: PluginManifest = match serde_yaml::from_str(&content) {
            Ok(manifest) => manifest,
            Err(_) => continue,
        };
        let disabled_marker = path.join(".disabled");
        out.push(PluginSurfaceEntry {
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            description: manifest.description.clone(),
            kind: coerce_memory_provider_kind(&path, manifest.kind.clone()),
            source,
            path: Some(path),
            enabled: !disabled_marker.exists(),
            entrypoint_value: None,
            entrypoint_dist: None,
        });
    }
    out
}

fn discover_python_entrypoint_plugins() -> Vec<PluginSurfaceEntry> {
    let script = r#"
import json
from importlib import metadata

def _entry_points():
    eps = metadata.entry_points()
    if hasattr(eps, "select"):
        return list(eps.select(group="hermes_agent.plugins"))
    if isinstance(eps, dict):
        return list(eps.get("hermes_agent.plugins", []))
    return [ep for ep in eps if getattr(ep, "group", "") == "hermes_agent.plugins"]

rows = []
try:
    for ep in _entry_points():
        dist = None
        try:
            if getattr(ep, "dist", None):
                dist = ep.dist.name
        except Exception:
            dist = None
        rows.append({
            "name": str(getattr(ep, "name", "") or ""),
            "value": str(getattr(ep, "value", "") or ""),
            "dist": dist,
        })
except Exception:
    rows = []
print(json.dumps({"entries": rows}))
"#;

    let output = std::process::Command::new("python3")
        .args(["-c", script])
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let payload: PythonEntrypointPayload = match serde_json::from_slice(&output.stdout) {
        Ok(payload) => payload,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    for item in payload.entries {
        let name = item.name.trim().to_string();
        if name.is_empty() {
            continue;
        }
        out.push(PluginSurfaceEntry {
            name,
            version: "entrypoint".to_string(),
            description: String::new(),
            kind: None,
            source: PluginSurfaceSource::Entrypoint,
            path: None,
            enabled: true,
            entrypoint_value: Some(item.value),
            entrypoint_dist: item.dist,
        });
    }
    out
}

pub(crate) fn discover_plugin_surface(include_entrypoints: bool) -> Vec<PluginSurfaceEntry> {
    let mut rows = Vec::new();
    let user_root = hermes_config::hermes_home().join("plugins");
    rows.extend(scan_plugin_manifest_root(
        &user_root,
        PluginSurfaceSource::User,
    ));

    if hermes_config::env_var_enabled("HERMES_ENABLE_PROJECT_PLUGINS") {
        if let Ok(cwd) = std::env::current_dir() {
            let project_root = hermes_config::project_hermes_dir(&cwd).join("plugins");
            rows.extend(scan_plugin_manifest_root(
                &project_root,
                PluginSurfaceSource::Project,
            ));
        }
    }

    if include_entrypoints {
        rows.extend(discover_python_entrypoint_plugins());
    }

    rows.sort_by(|a, b| {
        a.source.cmp(&b.source).then_with(|| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
        })
    });
    rows
}

fn resolve_local_plugin_path_by_name(name: &str) -> Option<PathBuf> {
    discover_plugin_surface(false)
        .into_iter()
        .filter_map(|row| {
            if row.name.eq_ignore_ascii_case(name) {
                row.path
            } else {
                None
            }
        })
        .next()
}

pub(crate) fn render_plugin_surface_table(rows: &[PluginSurfaceEntry]) -> String {
    if rows.is_empty() {
        return "  (no plugins discovered)".to_string();
    }
    let mut out = String::new();
    for row in rows {
        let status = if row.enabled { "enabled" } else { "disabled" };
        let mut meta_parts = vec![format!("source={}", row.source.label())];
        if let Some(kind) = row.kind.as_deref().filter(|k| !k.trim().is_empty()) {
            meta_parts.push(format!("kind={}", kind));
        }
        if let Some(dist) = row
            .entrypoint_dist
            .as_deref()
            .filter(|d| !d.trim().is_empty())
        {
            meta_parts.push(format!("dist={}", dist));
        }
        if let Some(value) = row
            .entrypoint_value
            .as_deref()
            .filter(|v| !v.trim().is_empty())
        {
            meta_parts.push(format!("entry={}", value));
        }
        let path = row
            .path
            .as_deref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "-".to_string());
        let version = if row.version.trim().is_empty() {
            "unknown".to_string()
        } else {
            row.version.clone()
        };
        let description = row.description.trim();
        let _ = writeln!(
            out,
            "  • {} v{} [{}; {}; path={}]",
            row.name,
            version,
            status,
            meta_parts.join(", "),
            path
        );
        if !description.is_empty() {
            let _ = writeln!(out, "    {}", description);
        }
    }
    out.trim_end().to_string()
}

fn set_plugin_enabled(path: &Path, enable: bool) -> Result<(), AgentError> {
    let marker = path.join(".disabled");
    if enable {
        if marker.exists() {
            std::fs::remove_file(&marker)
                .map_err(|e| AgentError::Io(format!("Failed to enable plugin: {}", e)))?;
        }
    } else {
        std::fs::write(&marker, "")
            .map_err(|e| AgentError::Io(format!("Failed to disable plugin: {}", e)))?;
    }
    Ok(())
}

fn parse_selection_indices(raw: &str, max: usize) -> Vec<usize> {
    let mut out = Vec::new();
    for token in raw.split(|c: char| c == ',' || c.is_ascii_whitespace()) {
        let trimmed = token.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(idx) = trimmed.parse::<usize>() else {
            continue;
        };
        if idx == 0 || idx > max {
            continue;
        }
        out.push(idx - 1);
    }
    out.sort_unstable();
    out.dedup();
    out
}

fn run_plugins_interactive_toggle() -> Result<(), AgentError> {
    let mut rows: Vec<PluginSurfaceEntry> = discover_plugin_surface(false)
        .into_iter()
        .filter(|row| row.path.is_some())
        .collect();
    if rows.is_empty() {
        println!("No plugin bundles discovered.");
        println!("Install one with: hermes plugins install <owner/repo>  (or a trusted git URL)");
        return Ok(());
    }

    rows.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
    });

    println!("Plugin toggle UI (interactive)");
    println!("------------------------------");
    println!("Source roots:");
    println!(
        "  - user:    {}",
        hermes_config::hermes_home().join("plugins").display()
    );
    if hermes_config::env_var_enabled("HERMES_ENABLE_PROJECT_PLUGINS") {
        if let Ok(cwd) = std::env::current_dir() {
            println!(
                "  - project: {}",
                hermes_config::project_hermes_dir(&cwd)
                    .join("plugins")
                    .display()
            );
        }
    } else {
        println!("  - project: disabled (set HERMES_ENABLE_PROJECT_PLUGINS=true)");
    }
    println!();

    let mut provider_indices = Vec::new();
    println!("General Plugins");
    for (idx, row) in rows.iter().enumerate() {
        let is_provider = row.kind.as_deref() == Some("exclusive");
        if is_provider {
            provider_indices.push(idx);
            continue;
        }
        let mark = if row.enabled { "✓" } else { " " };
        println!(
            "  {:>2}. [{}] {} (source={})",
            idx + 1,
            mark,
            row.name,
            row.source.label()
        );
    }

    if !provider_indices.is_empty() {
        println!();
        println!("Provider Plugins (single-select recommended)");
        for idx in &provider_indices {
            let row = &rows[*idx];
            let mark = if row.enabled { "✓" } else { " " };
            println!(
                "  {:>2}. [{}] {} (source={}, kind={})",
                idx + 1,
                mark,
                row.name,
                row.source.label(),
                row.kind.clone().unwrap_or_else(|| "provider".to_string())
            );
        }
    }

    use std::io::Write as _;
    print!("\nToggle plugin numbers (comma/space separated, Enter to skip): ");
    let _ = std::io::stdout().flush();
    let mut toggle_buf = String::new();
    std::io::stdin()
        .read_line(&mut toggle_buf)
        .map_err(|e| AgentError::Io(format!("Failed to read selection: {}", e)))?;
    let toggle_indices = parse_selection_indices(&toggle_buf, rows.len());
    for idx in toggle_indices {
        if let Some(path) = rows[idx].path.as_deref() {
            let target = !rows[idx].enabled;
            set_plugin_enabled(path, target)?;
            rows[idx].enabled = target;
        }
    }

    if !provider_indices.is_empty() {
        print!("Activate exactly one provider plugin number (Enter to keep current): ");
        let _ = std::io::stdout().flush();
        let mut provider_buf = String::new();
        std::io::stdin()
            .read_line(&mut provider_buf)
            .map_err(|e| AgentError::Io(format!("Failed to read provider selection: {}", e)))?;
        let selected = parse_selection_indices(&provider_buf, rows.len());
        if let Some(selected_idx) = selected.first().copied() {
            if provider_indices.contains(&selected_idx) {
                for idx in provider_indices {
                    if let Some(path) = rows[idx].path.as_deref() {
                        let should_enable = idx == selected_idx;
                        set_plugin_enabled(path, should_enable)?;
                        rows[idx].enabled = should_enable;
                    }
                }
            } else {
                println!(
                    "Selection {} is not a provider plugin row; keeping provider state unchanged.",
                    selected_idx + 1
                );
            }
        }
    }

    println!("\nUpdated plugin state:");
    println!("{}", render_plugin_surface_table(&rows));
    Ok(())
}

fn discover_python_plugin_cli_commands() -> Vec<PythonPluginCommandItem> {
    let script = r#"
import json
rows = []
try:
    from plugins.memory import discover_plugin_cli_commands
    for cmd in (discover_plugin_cli_commands() or []):
        name = str(cmd.get("name", "") or "").strip()
        if not name:
            continue
        help_text = str(cmd.get("help") or cmd.get("description") or "")
        rows.append({"name": name, "help": help_text})
except Exception:
    rows = []
print(json.dumps({"commands": rows}))
"#;
    let output = std::process::Command::new("python3")
        .args(["-c", script])
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let payload: PythonPluginCommandPayload = match serde_json::from_slice(&output.stdout) {
        Ok(payload) => payload,
        Err(_) => return Vec::new(),
    };
    let mut rows = payload.commands;
    rows.sort_by(|a, b| a.name.cmp(&b.name));
    rows.dedup_by(|a, b| a.name == b.name);
    rows
}

pub async fn handle_cli_external_plugin_subcommand(raw: Vec<String>) -> Result<(), AgentError> {
    if raw.is_empty() {
        return Err(AgentError::Config(
            "Unknown command. Run `hermes --help` for available commands.".to_string(),
        ));
    }
    let command_name = raw[0].trim().to_string();
    let command_args: Vec<String> = raw[1..].to_vec();
    let available = discover_python_plugin_cli_commands();
    if !available.iter().any(|row| row.name == command_name) {
        let catalog = if available.is_empty() {
            "none discovered".to_string()
        } else {
            available
                .iter()
                .map(|row| {
                    if row.help.trim().is_empty() {
                        format!("  - {}", row.name)
                    } else {
                        format!("  - {}: {}", row.name, row.help.trim())
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        return Err(AgentError::Config(format!(
            "Unknown command '{}'. Run `hermes --help` for core commands.\nDiscovered plugin commands:\n{}",
            command_name, catalog
        )));
    }

    let args_json = serde_json::to_string(&command_args)
        .map_err(|e| AgentError::Config(format!("Failed to serialize plugin CLI args: {}", e)))?;
    let script = r#"
import argparse
import json
import sys

try:
    from plugins.memory import discover_plugin_cli_commands
except Exception as exc:
    print(f"Plugin CLI bridge unavailable: {exc}", file=sys.stderr)
    sys.exit(2)

name = sys.argv[1]
argv = json.loads(sys.argv[2])

for item in (discover_plugin_cli_commands() or []):
    if str(item.get("name", "")).strip() != name:
        continue
    setup = item.get("setup_fn")
    if not callable(setup):
        print(f"Plugin command '{name}' is missing setup_fn", file=sys.stderr)
        sys.exit(2)
    parser = argparse.ArgumentParser(prog=name)
    setup(parser)
    ns = parser.parse_args(argv)
    handler = item.get("handler_fn")
    if callable(handler):
        handler(ns)
        sys.exit(0)
    if hasattr(ns, "func") and callable(getattr(ns, "func")):
        ns.func(ns)
        sys.exit(0)
    parser.print_help()
    sys.exit(0)

print(f"Unknown plugin command: {name}", file=sys.stderr)
sys.exit(3)
"#;

    let output = tokio::process::Command::new("python3")
        .args(["-c", script, &command_name, &args_json])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await
        .map_err(|e| AgentError::Io(format!("Failed to execute plugin command: {}", e)))?;
    if !output.success() {
        return Err(AgentError::Config(format!(
            "Plugin command '{}' failed with exit code {:?}.",
            command_name,
            output.code()
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Plugin security (remote Git installs)
// ---------------------------------------------------------------------------

fn default_git_host_allowlist() -> Vec<&'static str> {
    vec![
        "github.com",
        "www.github.com",
        "raw.githubusercontent.com",
        "gitlab.com",
        "www.gitlab.com",
        "codeberg.org",
        "www.codeberg.org",
        "gitea.com",
        "bitbucket.org",
    ]
}

fn plugin_git_host_allowed(url: &str, allow_untrusted: bool) -> bool {
    if allow_untrusted {
        return true;
    }
    let extra = std::env::var("HERMES_PLUGIN_GIT_EXTRA_HOSTS").unwrap_or_default();
    let mut hosts: Vec<String> = default_git_host_allowlist()
        .into_iter()
        .map(|s| s.to_string())
        .collect();
    for part in extra.split(',') {
        let p = part.trim();
        if !p.is_empty() {
            hosts.push(p.to_lowercase());
        }
    }
    let lower = url.to_lowercase();
    let host_part = if lower.contains("://") {
        lower.split("://").nth(1).unwrap_or("")
    } else if lower.starts_with("git@") {
        lower
            .trim_start_matches("git@")
            .split(':')
            .next()
            .unwrap_or("")
    } else {
        return false;
    };
    let host = host_part
        .split('/')
        .next()
        .unwrap_or(host_part)
        .split('@')
        .last()
        .unwrap_or(host_part);
    let host = host.split(':').next().unwrap_or(host).to_lowercase();
    hosts
        .iter()
        .any(|h| host == *h || host.ends_with(&format!(".{}", h)))
}

fn short_sha(sha: &str) -> String {
    sha.chars().take(8).collect()
}

/// Static scan of a cloned plugin tree: risky patterns in scripts/config.
fn scan_plugin_security(root: &std::path::Path) -> Vec<String> {
    let mut out = Vec::new();
    let manifest = root.join("plugin.yaml");
    if manifest.exists() {
        if let Ok(text) = std::fs::read_to_string(&manifest) {
            if text.contains("post_install") || text.contains("postInstall") {
                out.push(
                    "plugin.yaml declares post_install / postInstall — review before running the plugin"
                        .into(),
                );
            }
            if Regex::new(r"(?i)curl\s+[^|\n]*\|\s*(ba)?sh")
                .ok()
                .and_then(|re| re.find(&text))
                .is_some()
            {
                out.push("plugin.yaml references curl|sh style install — high risk".into());
            }
        }
    }

    let risky_file_patterns: &[(&str, &[(&str, &str)])] = &[(
        r"\.(sh|bash|zsh|py|rb|ps1|fish)$",
        &[
            (r"(?i)\bcurl\s+[^|\n]*\|\s*(ba)?sh", "curl piped to shell"),
            (r"(?i)\bwget\s+[^|\n]*\|\s*(ba)?sh", "wget piped to shell"),
            (r"(?i)\beval\s*\(", "eval("),
            (r"(?i)\bexec\s*\(", "exec("),
            (r"(?i)(base64[._-]?decode|atob)\s*\(", "base64 decode"),
            (r"(?i)\brm\s+-rf\s+/", "rm -rf on absolute path"),
        ],
    )];

    fn walk(dir: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
        let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if dir.is_dir() && (name == ".git" || name == "target" || name == "node_modules") {
            return;
        }
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, files);
                } else if p.is_file() {
                    files.push(p);
                }
            }
        }
    }

    let mut files = Vec::new();
    walk(root, &mut files);

    for fp in files {
        let fname = fp.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if fname == ".DS_Store" {
            continue;
        }
        let rel = fp.strip_prefix(root).unwrap_or(&fp).display().to_string();
        let Ok(content) = std::fs::read_to_string(&fp) else {
            continue;
        };
        for (ext_re, rules) in risky_file_patterns {
            if let Ok(re_ext) = Regex::new(ext_re) {
                if !re_ext.is_match(fname) {
                    continue;
                }
                for (pat, label) in *rules {
                    if let Ok(re) = Regex::new(pat) {
                        if re.is_match(&content) {
                            out.push(format!("{}: {}", rel, label));
                        }
                    }
                }
            }
        }
    }

    out.sort();
    out.dedup();
    out
}

async fn git_checkout_ref(repo_dir: &std::path::Path, git_ref: &str) -> Result<(), String> {
    let dir = repo_dir.to_string_lossy().to_string();
    let fetch = tokio::process::Command::new("git")
        .args(["-C", &dir, "fetch", "--depth", "1", "origin", git_ref])
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if !fetch.status.success() {
        let err = String::from_utf8_lossy(&fetch.stderr);
        return Err(format!("git fetch origin {}: {}", git_ref, err.trim()));
    }
    let co = tokio::process::Command::new("git")
        .args(["-C", &dir, "checkout", git_ref])
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if !co.status.success() {
        let err = String::from_utf8_lossy(&co.stderr);
        return Err(format!("git checkout {}: {}", git_ref, err.trim()));
    }
    Ok(())
}

/// Handle `hermes plugins [action] [name]`.
pub async fn handle_cli_plugins(
    action: Option<String>,
    name: Option<String>,
    git_ref: Option<String>,
    allow_untrusted_git_host: bool,
) -> Result<(), hermes_core::AgentError> {
    let plugins_dir = hermes_config::hermes_home().join("plugins");

    match action.as_deref() {
        None => {
            run_plugins_interactive_toggle()?;
        }
        Some("list") => {
            let rows = discover_plugin_surface(true);
            println!("Plugin surface ({} entries):", rows.len());
            println!("{}", render_plugin_surface_table(&rows));
        }
        Some("enable") => {
            let plugin_name = name.ok_or_else(|| {
                hermes_core::AgentError::Config(
                    "Missing plugin name. Usage: hermes plugins enable <name>".into(),
                )
            })?;
            let target = resolve_local_plugin_path_by_name(&plugin_name)
                .unwrap_or_else(|| plugins_dir.join(&plugin_name));
            let disabled_marker = target.join(".disabled");
            if disabled_marker.exists() {
                std::fs::remove_file(&disabled_marker).map_err(|e| {
                    hermes_core::AgentError::Io(format!("Failed to enable plugin: {}", e))
                })?;
                println!("Plugin '{}' enabled.", plugin_name);
            } else {
                println!(
                    "Plugin '{}' is already enabled (or not installed).",
                    plugin_name
                );
            }
        }
        Some("disable") => {
            let plugin_name = name.ok_or_else(|| {
                hermes_core::AgentError::Config(
                    "Missing plugin name. Usage: hermes plugins disable <name>".into(),
                )
            })?;
            let plugin_dir = resolve_local_plugin_path_by_name(&plugin_name)
                .unwrap_or_else(|| plugins_dir.join(&plugin_name));
            if !plugin_dir.exists() {
                println!("Plugin '{}' not found.", plugin_name);
                return Ok(());
            }
            let disabled_marker = plugin_dir.join(".disabled");
            std::fs::write(&disabled_marker, "").map_err(|e| {
                hermes_core::AgentError::Io(format!("Failed to disable plugin: {}", e))
            })?;
            println!("Plugin '{}' disabled.", plugin_name);
        }
        Some("install") => {
            let plugin_name = name.ok_or_else(|| {
                hermes_core::AgentError::Config(
                    "Missing plugin name. Usage: hermes plugins install <name|url>".into(),
                )
            })?;
            println!("Installing plugin: {}...", plugin_name);

            let is_git_url = plugin_name.starts_with("http://")
                || plugin_name.starts_with("https://")
                || plugin_name.starts_with("git@");

            if is_git_url {
                if !plugin_git_host_allowed(&plugin_name, allow_untrusted_git_host) {
                    println!(
                        "  ✗ Git host is not on the default allow-list (github.com, gitlab.com, codeberg.org, …)."
                    );
                    println!(
                        "    Set comma-separated HERMES_PLUGIN_GIT_EXTRA_HOSTS or pass --allow-untrusted-git-host after you trust the source."
                    );
                    return Ok(());
                }
                // Extract repo name from URL for target directory
                let repo_name = plugin_name
                    .trim_end_matches('/')
                    .trim_end_matches(".git")
                    .rsplit('/')
                    .next()
                    .unwrap_or("unknown-plugin")
                    .to_string();

                // Also handle git@ SSH URLs like git@github.com:user/repo.git
                let repo_name = if repo_name.contains(':') {
                    repo_name
                        .rsplit(':')
                        .next()
                        .unwrap_or(&repo_name)
                        .trim_end_matches(".git")
                        .rsplit('/')
                        .next()
                        .unwrap_or(&repo_name)
                        .to_string()
                } else {
                    repo_name
                };

                let target = plugins_dir.join(&repo_name);
                if target.exists() {
                    println!(
                        "Plugin '{}' is already installed at {}",
                        repo_name,
                        target.display()
                    );
                    return Ok(());
                }

                std::fs::create_dir_all(&plugins_dir).map_err(|e| {
                    hermes_core::AgentError::Io(format!("Failed to create plugins dir: {}", e))
                })?;

                println!("  Cloning {} ...", plugin_name);
                let output = tokio::process::Command::new("git")
                    .args([
                        "clone",
                        "--depth",
                        "1",
                        &plugin_name,
                        &target.to_string_lossy(),
                    ])
                    .output()
                    .await
                    .map_err(|e| hermes_core::AgentError::Io(format!("git clone failed: {}", e)))?;

                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    println!("  ✗ git clone failed: {}", stderr.trim());
                    return Ok(());
                }

                if let Some(gr) = git_ref.as_deref() {
                    println!("  Checking out ref: {} ...", gr);
                    if let Err(e) = git_checkout_ref(&target, gr).await {
                        println!("  ✗ {}", e);
                        let _ = std::fs::remove_dir_all(&target);
                        return Ok(());
                    }
                }

                // Verify plugin.yaml exists
                let manifest_path = target.join("plugin.yaml");
                if !manifest_path.exists() {
                    println!("  ✗ No plugin.yaml found in cloned repository.");
                    println!("    Removing {}...", target.display());
                    let _ = std::fs::remove_dir_all(&target);
                    return Ok(());
                }

                // Parse and display plugin info
                let manifest_content = std::fs::read_to_string(&manifest_path)
                    .map_err(|e| hermes_core::AgentError::Io(format!("Read error: {}", e)))?;
                let manifest: serde_json::Value =
                    serde_yaml::from_str(&manifest_content).unwrap_or(serde_json::json!({}));

                let p_name = manifest
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&repo_name);
                let p_version = manifest
                    .get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let p_desc = manifest
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                // Security scan of cloned files
                let suspicious = scan_plugin_security(&target);
                let hard_block = suspicious.iter().any(|s| {
                    s.contains("curl piped to shell")
                        || s.contains("wget piped to shell")
                        || s.contains("curl|sh style install")
                });
                if hard_block && !allow_untrusted_git_host {
                    println!("\n  ✗ High-risk install patterns detected — clone removed.");
                    for warning in &suspicious {
                        println!("    - {}", warning);
                    }
                    println!(
                        "\n  If you reviewed the code manually, re-run with --allow-untrusted-git-host."
                    );
                    let _ = std::fs::remove_dir_all(&target);
                    return Ok(());
                }
                if !suspicious.is_empty() {
                    println!("\n  ⚠ Security warnings found ({}):", suspicious.len());
                    for warning in &suspicious {
                        println!("    - {}", warning);
                    }
                    println!("\n  Review the warnings above before enabling this plugin.");
                }

                println!("  ✓ Plugin installed successfully!");
                println!("    Name:        {}", p_name);
                println!("    Version:     {}", p_version);
                println!("    Description: {}", p_desc);
                println!("    Path:        {}", target.display());
            } else if plugin_name.starts_with("gh:") || plugin_name.contains('/') {
                // Convert gh:user/repo or user/repo to a GitHub HTTPS URL
                let repo_path = plugin_name.trim_start_matches("gh:");
                let git_url = format!("https://github.com/{}.git", repo_path);
                let repo_name = repo_path.rsplit('/').next().unwrap_or("unknown-plugin");
                let target = plugins_dir.join(repo_name);
                if target.exists() {
                    println!("Plugin '{}' is already installed.", repo_name);
                    return Ok(());
                }

                std::fs::create_dir_all(&plugins_dir).map_err(|e| {
                    hermes_core::AgentError::Io(format!("Failed to create plugins dir: {}", e))
                })?;

                println!("  Cloning from GitHub: {}", git_url);
                let output = tokio::process::Command::new("git")
                    .args(["clone", "--depth", "1", &git_url, &target.to_string_lossy()])
                    .output()
                    .await
                    .map_err(|e| hermes_core::AgentError::Io(format!("git clone failed: {}", e)))?;

                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    println!("  ✗ git clone failed: {}", stderr.trim());
                    return Ok(());
                }

                if let Some(gr) = git_ref.as_deref() {
                    println!("  Checking out ref: {} ...", gr);
                    if let Err(e) = git_checkout_ref(&target, gr).await {
                        println!("  ✗ {}", e);
                        let _ = std::fs::remove_dir_all(&target);
                        return Ok(());
                    }
                }

                let manifest_path = target.join("plugin.yaml");
                if !manifest_path.exists() {
                    println!("  ✗ No plugin.yaml found in cloned repository.");
                    let _ = std::fs::remove_dir_all(&target);
                    return Ok(());
                }

                let manifest_content = std::fs::read_to_string(&manifest_path).unwrap_or_default();
                let manifest: serde_json::Value =
                    serde_yaml::from_str(&manifest_content).unwrap_or(serde_json::json!({}));

                let p_name = manifest
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or(repo_name);
                let p_version = manifest
                    .get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let p_desc = manifest
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let suspicious = scan_plugin_security(&target);
                let hard_block = suspicious.iter().any(|s| {
                    s.contains("curl piped to shell")
                        || s.contains("wget piped to shell")
                        || s.contains("curl|sh style install")
                });
                if hard_block && !allow_untrusted_git_host {
                    println!("\n  ✗ High-risk install patterns detected — clone removed.");
                    for warning in &suspicious {
                        println!("    - {}", warning);
                    }
                    println!(
                        "\n  If you reviewed the code manually, re-run with --allow-untrusted-git-host."
                    );
                    let _ = std::fs::remove_dir_all(&target);
                    return Ok(());
                }
                if !suspicious.is_empty() {
                    println!("\n  ⚠ Security warnings found ({}):", suspicious.len());
                    for warning in &suspicious {
                        println!("    - {}", warning);
                    }
                }

                println!("  ✓ Plugin installed successfully!");
                println!("    Name:        {}", p_name);
                println!("    Version:     {}", p_version);
                println!("    Description: {}", p_desc);
                println!("    Path:        {}", target.display());
            } else {
                let target = plugins_dir.join(&plugin_name);
                if target.exists() {
                    println!("Plugin '{}' is already installed.", plugin_name);
                    return Ok(());
                }
                // Registry lookup
                println!("  Looking up '{}' in plugin registry...", plugin_name);
                match reqwest::Client::new()
                    .get(&format!(
                        "https://plugins.hermes.run/api/v1/{}",
                        plugin_name
                    ))
                    .timeout(std::time::Duration::from_secs(10))
                    .send()
                    .await
                {
                    Ok(resp) if resp.status().is_success() => {
                        if let Ok(data) = resp.json::<serde_json::Value>().await {
                            let version = data
                                .get("version")
                                .and_then(|v| v.as_str())
                                .unwrap_or("latest");
                            let git_url = data.get("git_url").and_then(|v| v.as_str());
                            println!("  Found {} v{}", plugin_name, version);

                            if let Some(url) = git_url {
                                if !plugin_git_host_allowed(url, allow_untrusted_git_host) {
                                    println!(
                                        "  ✗ Registry git_url host is not allow-listed. Use --allow-untrusted-git-host or HERMES_PLUGIN_GIT_EXTRA_HOSTS."
                                    );
                                    return Ok(());
                                }
                                std::fs::create_dir_all(&plugins_dir)
                                    .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;

                                let output = tokio::process::Command::new("git")
                                    .args(["clone", "--depth", "1", url, &target.to_string_lossy()])
                                    .output()
                                    .await
                                    .map_err(|e| {
                                        hermes_core::AgentError::Io(format!(
                                            "git clone failed: {}",
                                            e
                                        ))
                                    })?;

                                if output.status.success() {
                                    if let Some(gr) = git_ref.as_deref() {
                                        println!("  Checking out ref: {} ...", gr);
                                        if let Err(e) = git_checkout_ref(&target, gr).await {
                                            println!("  ✗ {}", e);
                                            let _ = std::fs::remove_dir_all(&target);
                                            return Ok(());
                                        }
                                    }
                                    let suspicious = scan_plugin_security(&target);
                                    let hard_block = suspicious.iter().any(|s| {
                                        s.contains("curl piped to shell")
                                            || s.contains("wget piped to shell")
                                            || s.contains("curl|sh style install")
                                    });
                                    if hard_block && !allow_untrusted_git_host {
                                        println!("  ✗ High-risk patterns — removed clone.");
                                        let _ = std::fs::remove_dir_all(&target);
                                        return Ok(());
                                    }
                                    if !suspicious.is_empty() {
                                        println!("  ⚠ Security warnings: {}", suspicious.len());
                                        for w in &suspicious {
                                            println!("    - {}", w);
                                        }
                                    }
                                    println!(
                                        "  ✓ Plugin '{}' v{} installed.",
                                        plugin_name, version
                                    );
                                } else {
                                    let stderr = String::from_utf8_lossy(&output.stderr);
                                    println!("  ✗ Clone failed: {}", stderr.trim());
                                }
                            } else {
                                println!("  No git_url in registry response. Cannot install.");
                            }
                        }
                    }
                    _ => {
                        println!("  Plugin '{}' not found in registry.", plugin_name);
                        println!("  Try installing from a URL or GitHub repo instead:");
                        println!("    hermes plugins install https://github.com/user/repo");
                        println!("    hermes plugins install gh:user/repo");
                    }
                }
            }
        }
        Some("remove") | Some("uninstall") => {
            let plugin_name = name.ok_or_else(|| {
                hermes_core::AgentError::Config(
                    "Missing plugin name. Usage: hermes plugins remove <name>".into(),
                )
            })?;
            let target = resolve_local_plugin_path_by_name(&plugin_name)
                .unwrap_or_else(|| plugins_dir.join(&plugin_name));
            if target.exists() {
                std::fs::remove_dir_all(&target).map_err(|e| {
                    hermes_core::AgentError::Io(format!("Failed to remove plugin: {}", e))
                })?;
                println!("Plugin '{}' removed.", plugin_name);
            } else {
                println!("Plugin '{}' not found.", plugin_name);
            }
        }
        Some("update") => {
            let plugin_name = name.as_deref();
            let mut checked = 0u32;
            let mut updated = 0u32;
            if !plugins_dir.exists() {
                println!("No plugins installed.");
                return Ok(());
            }
            if let Ok(entries) = std::fs::read_dir(&plugins_dir) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let path = entry.path();
                    if !path.is_dir() {
                        continue;
                    }
                    let dir_name = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned();
                    if let Some(target) = plugin_name {
                        if dir_name != target {
                            continue;
                        }
                    }
                    let manifest = path.join("plugin.yaml");
                    if manifest.exists() {
                        checked += 1;
                        println!("  Checking updates for '{}'...", dir_name);

                        let git_dir = path.join(".git");
                        if !git_dir.exists() {
                            println!("    Skipped: plugin is not a git checkout.");
                            continue;
                        }

                        let path_s = path.to_string_lossy().to_string();
                        let before = tokio::process::Command::new("git")
                            .args(["-C", &path_s, "rev-parse", "HEAD"])
                            .output()
                            .await
                            .map_err(|e| {
                                hermes_core::AgentError::Io(format!(
                                    "git rev-parse failed for {}: {}",
                                    dir_name, e
                                ))
                            })?;
                        if !before.status.success() {
                            let stderr = String::from_utf8_lossy(&before.stderr);
                            println!(
                                "    Skipped: cannot read current revision ({})",
                                stderr.trim()
                            );
                            continue;
                        }
                        let before_sha = String::from_utf8_lossy(&before.stdout).trim().to_string();

                        let pull = tokio::process::Command::new("git")
                            .args(["-C", &path_s, "pull", "--ff-only"])
                            .output()
                            .await
                            .map_err(|e| {
                                hermes_core::AgentError::Io(format!(
                                    "git pull failed for {}: {}",
                                    dir_name, e
                                ))
                            })?;

                        if !pull.status.success() {
                            let stderr = String::from_utf8_lossy(&pull.stderr);
                            println!("    Update failed: {}", stderr.trim());
                            continue;
                        }

                        let after = tokio::process::Command::new("git")
                            .args(["-C", &path_s, "rev-parse", "HEAD"])
                            .output()
                            .await
                            .map_err(|e| {
                                hermes_core::AgentError::Io(format!(
                                    "git rev-parse failed for {} after update: {}",
                                    dir_name, e
                                ))
                            })?;
                        if !after.status.success() {
                            let stderr = String::from_utf8_lossy(&after.stderr);
                            println!(
                                "    Updated but could not read final revision ({})",
                                stderr.trim()
                            );
                            continue;
                        }
                        let after_sha = String::from_utf8_lossy(&after.stdout).trim().to_string();

                        if before_sha == after_sha {
                            println!("    Up to date ({})", short_sha(&after_sha));
                        } else {
                            updated += 1;
                            println!(
                                "    Updated: {} -> {}",
                                short_sha(&before_sha),
                                short_sha(&after_sha)
                            );
                        }
                    }
                }
            }
            if checked == 0 {
                if let Some(n) = plugin_name {
                    println!("Plugin '{}' not found.", n);
                } else {
                    println!("No plugins to update.");
                }
            } else {
                println!("Checked {} plugin(s); updated {}.", checked, updated);
            }
        }
        Some("inspect") | Some("info") => {
            let plugin_name = name.ok_or_else(|| {
                hermes_core::AgentError::Config(
                    "Missing plugin name. Usage: hermes plugins inspect <name>".into(),
                )
            })?;
            let surface_rows = discover_plugin_surface(true);
            if let Some(row) = surface_rows
                .iter()
                .find(|row| row.name.eq_ignore_ascii_case(&plugin_name))
            {
                println!("Plugin: {}", row.name);
                println!("Source: {}", row.source.label());
                println!(
                    "Status: {}",
                    if row.enabled { "enabled" } else { "disabled" }
                );
                let version = if row.version.trim().is_empty() {
                    "unknown"
                } else {
                    row.version.as_str()
                };
                println!("Version: {}", version);
                if let Some(kind) = row.kind.as_deref().filter(|k| !k.trim().is_empty()) {
                    println!("Kind: {}", kind);
                }
                if let Some(path) = row.path.as_deref() {
                    println!("Path: {}", path.display());
                }
                if let Some(value) = row
                    .entrypoint_value
                    .as_deref()
                    .filter(|v| !v.trim().is_empty())
                {
                    println!("Entrypoint: {}", value);
                }
                if let Some(dist) = row
                    .entrypoint_dist
                    .as_deref()
                    .filter(|d| !d.trim().is_empty())
                {
                    println!("Distribution: {}", dist);
                }
                if !row.description.trim().is_empty() {
                    println!("Description: {}", row.description.trim());
                }
            }
            let target = resolve_local_plugin_path_by_name(&plugin_name)
                .unwrap_or_else(|| plugins_dir.join(&plugin_name));
            if !target.exists() {
                if surface_rows
                    .iter()
                    .any(|row| row.name.eq_ignore_ascii_case(&plugin_name))
                {
                    return Ok(());
                }
                println!("Plugin '{}' not found.", plugin_name);
                return Ok(());
            }
            let manifest_path = target.join("plugin.yaml");
            if manifest_path.exists() {
                let content = std::fs::read_to_string(&manifest_path)
                    .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;
                println!("Plugin: {}", plugin_name);
                println!("Path:   {}", target.display());
                let disabled = target.join(".disabled").exists();
                println!("Status: {}", if disabled { "disabled" } else { "enabled" });
                println!("\n--- plugin.yaml ---");
                println!("{}", content);
            } else {
                println!("Plugin '{}' has no plugin.yaml manifest.", plugin_name);
            }
        }
        Some(other) => {
            println!("Plugins action '{}' is not recognized.", other);
            println!("Available: list, install, remove, enable, disable, update, inspect");
        }
    }
    Ok(())
}

/// Handle `hermes memory [action]`.
pub async fn handle_cli_memory(
    action: Option<String>,
    target: Option<String>,
    yes: bool,
) -> Result<(), hermes_core::AgentError> {
    let hermes_home = hermes_config::hermes_home();
    let memories_dir = hermes_home.join("memories");
    let memory_md = memories_dir.join("MEMORY.md");
    let user_md = memories_dir.join("USER.md");
    let legacy_memory_db = hermes_home.join("memory.db");
    let disabled_marker = hermes_home.join(".memory_disabled");

    match action.as_deref().unwrap_or("status") {
        "status" => {
            if disabled_marker.exists() {
                println!("Memory provider: disabled");
                println!("  Marker: {}", disabled_marker.display());
                println!("Run `hermes memory setup` to re-enable.");
                return Ok(());
            }

            if memory_md.exists() || user_md.exists() {
                let mem_size = std::fs::metadata(&memory_md).map(|m| m.len()).unwrap_or(0);
                let user_size = std::fs::metadata(&user_md).map(|m| m.len()).unwrap_or(0);
                println!("Memory provider: files (MEMORY.md + USER.md)");
                println!("  Directory: {}", memories_dir.display());
                println!(
                    "  MEMORY.md: {} ({:.1} KB)",
                    memory_md.display(),
                    mem_size as f64 / 1024.0
                );
                println!(
                    "  USER.md:   {} ({:.1} KB)",
                    user_md.display(),
                    user_size as f64 / 1024.0
                );
                if legacy_memory_db.exists() {
                    println!(
                        "  Legacy file detected (unused by current memory backend): {}",
                        legacy_memory_db.display()
                    );
                }
            } else if legacy_memory_db.exists() {
                let size = std::fs::metadata(&legacy_memory_db)
                    .map(|m| m.len())
                    .unwrap_or(0);
                println!("Memory provider: legacy sqlite artifact only");
                println!("  File: {}", legacy_memory_db.display());
                println!("  Size: {} KB", size / 1024);
                println!("Run `hermes memory setup` to initialize the current file backend.");
            } else {
                println!("Memory provider: not configured");
                println!("Run `hermes memory setup` to initialize.");
            }
        }
        "setup" => {
            println!("Memory Provider Setup");
            println!("---------------------");
            std::fs::create_dir_all(&memories_dir)
                .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;
            if !memory_md.exists() {
                std::fs::write(
                    &memory_md,
                    "# Hermes MEMORY\n\nStore durable assistant memory entries here.\n",
                )
                .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;
            }
            if !user_md.exists() {
                std::fs::write(
                    &user_md,
                    "# Hermes USER\n\nStore durable user profile entries here.\n",
                )
                .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;
            }
            if disabled_marker.exists() {
                let _ = std::fs::remove_file(&disabled_marker);
            }
            println!("Initialized file memory backend.");
            println!("  MEMORY.md: {}", memory_md.display());
            println!("  USER.md:   {}", user_md.display());
            println!("Memory is enabled for subsequent sessions.");
        }
        "off" => {
            std::fs::create_dir_all(&hermes_home)
                .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;
            std::fs::write(
                &disabled_marker,
                format!(
                    "disabled_at={}\nreason=hermes memory off\n",
                    chrono::Utc::now().to_rfc3339()
                ),
            )
            .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;
            println!("Memory provider disabled.");
            println!("  Marker: {}", disabled_marker.display());
            println!("Run `hermes memory setup` to re-enable.");
        }
        "reset" => {
            if !yes {
                return Err(hermes_core::AgentError::Config(
                    "memory reset requires confirmation flag: use `hermes memory reset [all|memory|user] -y`"
                        .into(),
                ));
            }
            let reset_target = target
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("all")
                .to_ascii_lowercase();
            let reset_memory = reset_target == "all" || reset_target == "memory";
            let reset_user = reset_target == "all" || reset_target == "user";
            if !reset_memory && !reset_user {
                return Err(hermes_core::AgentError::Config(format!(
                    "Unknown memory reset target '{}'. Use all|memory|user",
                    reset_target
                )));
            }
            std::fs::create_dir_all(&memories_dir)
                .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;
            if reset_memory && memory_md.exists() {
                let _ = std::fs::remove_file(&memory_md);
            }
            if reset_user && user_md.exists() {
                let _ = std::fs::remove_file(&user_md);
            }
            if reset_target == "all" && legacy_memory_db.exists() {
                let _ = std::fs::remove_file(&legacy_memory_db);
            }
            if disabled_marker.exists() {
                let _ = std::fs::remove_file(&disabled_marker);
            }
            if reset_memory {
                std::fs::write(
                    &memory_md,
                    "# Hermes MEMORY\n\nStore durable assistant memory entries here.\n",
                )
                .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;
            }
            if reset_user {
                std::fs::write(
                    &user_md,
                    "# Hermes USER\n\nStore durable user profile entries here.\n",
                )
                .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;
            }
            println!(
                "Memory reset complete (target={}). MEMORY.md={} USER.md={}",
                reset_target,
                if memory_md.exists() {
                    "present"
                } else {
                    "absent"
                },
                if user_md.exists() {
                    "present"
                } else {
                    "absent"
                }
            );
        }
        other => {
            println!("Unknown memory action '{}'.", other);
            println!("Available actions: status, setup, off, reset");
        }
    }
    Ok(())
}

/// Handle `hermes interest [list|status|clear|enable|preview|reject|pin]`.
pub async fn handle_cli_interest(
    action: Option<String>,
    mode: Option<String>,
    llm_on_session_end: bool,
    rest: Vec<String>,
) -> Result<(), hermes_core::AgentError> {
    let config = hermes_config::load_config(None).unwrap_or_default();
    let hermes_home = config
        .home_dir
        .as_ref()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(hermes_config::hermes_home);
    let db_path = hermes_home.join("interest.db");

    match action.as_deref().unwrap_or("list") {
        "status" | "list" => {
            if !config.interest.enabled {
                println!("User interest (POI): disabled in config (interest.enabled = false)");
                return Ok(());
            }
            println!("  Pipeline: Extract → Compare → Update (session-end commit)");
            println!("  Extract mode: {}", config.interest.extract_mode);
            println!(
                "  Per-turn buffer / persist: {} / {}",
                config.interest.per_turn_buffer, config.interest.per_turn_persist
            );
            println!(
                "  Session-end LLM: {}",
                if config.interest.session_end_llm_enabled() {
                    "on"
                } else {
                    "off"
                }
            );
            if !db_path.exists() {
                println!("User interest (POI): no topics yet");
                println!("  Database: {}", db_path.display());
                println!("  Topics are learned from conversations when interest.enabled is true.");
                return Ok(());
            }
            let store = hermes_agent::InterestStore::open(&db_path, config.interest.clone())
                .map_err(|e| hermes_core::AgentError::Io(e))?;
            let topics = store
                .list_for_cli(true)
                .map_err(|e| hermes_core::AgentError::Io(e))?;
            println!("User interest (POI): {} topic(s)", topics.len());
            println!("  Database: {}", db_path.display());
            for (idx, topic) in topics.iter().enumerate() {
                let pin = if topic.pinned { " pinned" } else { "" };
                println!(
                    "  {:>2}. [{:.2}] ({}{}) {} — {}",
                    idx + 1,
                    topic.weight,
                    topic.status.as_str(),
                    pin,
                    topic.label,
                    topic.summary
                );
                if !topic.tags.is_empty() {
                    println!("      tags: {}", topic.tags.join(", "));
                }
                println!("      id: {}", topic.id);
            }
        }
        "clear" => {
            if db_path.exists() {
                std::fs::remove_file(&db_path)
                    .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;
            }
            println!("Cleared interest store at {}", db_path.display());
        }
        "prune" => {
            if !db_path.exists() {
                println!("Nothing to prune (no interest.db).");
                return Ok(());
            }
            let store = hermes_agent::InterestStore::open(&db_path, config.interest.clone())
                .map_err(|e| hermes_core::AgentError::Io(e))?;
            let removed = store
                .prune_rejected_topics()
                .map_err(|e| hermes_core::AgentError::Io(e))?;
            println!(
                "Pruned {removed} non-POI topic row(s) from {}",
                db_path.display()
            );
        }
        "enable" => {
            let cfg_path = hermes_config::config_path();
            let mut disk = hermes_config::load_user_config_file(&cfg_path)
                .map_err(|e| hermes_core::AgentError::Config(e.to_string()))?;
            disk.interest.enabled = true;
            disk.interest.per_turn_buffer = true;
            disk.interest.per_turn_persist = false;
            if let Some(m) = mode.as_deref() {
                let m = m.trim().to_ascii_lowercase();
                if matches!(m.as_str(), "rules" | "hybrid" | "llm") {
                    disk.interest.extract_mode = m;
                } else {
                    return Err(hermes_core::AgentError::Config(format!(
                        "interest --mode must be rules, hybrid, or llm (got {m})"
                    )));
                }
            }
            if llm_on_session_end {
                disk.interest.llm_on_session_end = true;
            }
            hermes_config::save_config_yaml(&cfg_path, &disk)
                .map_err(|e| hermes_core::AgentError::Config(e.to_string()))?;
            println!("User interest (POI) extraction enabled (interest.enabled = true).");
            println!("  Extract mode: {}", disk.interest.extract_mode);
            println!(
                "  Session-end LLM: {}",
                if disk.interest.session_end_llm_enabled() {
                    "on"
                } else {
                    "off"
                }
            );
            println!("  Per-turn: buffer only (persist at session end)");
            println!("  Config: {}", cfg_path.display());
            if disk.interest.session_end_llm_enabled() {
                println!("  Note: user messages may be sent to the auxiliary LLM at session end.");
            }
        }
        "preview" => {
            use hermes_agent::{ExtractOptions, extract_signals_from_text};
            let sample = if rest.is_empty() {
                "Help me continue the Rust parity port in crates/hermes-parity-tests".to_string()
            } else {
                rest.join(" ")
            };
            let raw = extract_signals_from_text(&sample, 1.0, ExtractOptions::default());
            let filtered = hermes_agent::filter_persistable_signals(raw);
            println!("POI extract preview (not persisted):");
            println!("  Sample: {sample}");
            if filtered.is_empty() {
                println!("  No persistable signals after quality gate.");
            } else {
                for sig in &filtered {
                    println!(
                        "  - [{}] {} (conf {:.2}, Δweight {:.2})",
                        sig.source().as_str(),
                        sig.label,
                        sig.confidence,
                        sig.weight_delta
                    );
                }
            }
        }
        "reject" => {
            let topic_id = rest.first().map(String::as_str).ok_or_else(|| {
                hermes_core::AgentError::Config(
                    "usage: hermes interest reject <topic-id>".to_string(),
                )
            })?;
            let store = hermes_agent::InterestStore::open(&db_path, config.interest.clone())
                .map_err(|e| hermes_core::AgentError::Io(e))?;
            let ok = store
                .set_topic_status(topic_id, hermes_agent::TopicStatus::Rejected)
                .map_err(|e| hermes_core::AgentError::Io(e))?;
            if ok {
                println!("Rejected topic {topic_id}");
            } else {
                println!("Topic not found: {topic_id}");
            }
        }
        "pin" => {
            let topic_id = rest.first().map(String::as_str).ok_or_else(|| {
                hermes_core::AgentError::Config("usage: hermes interest pin <topic-id>".to_string())
            })?;
            let store = hermes_agent::InterestStore::open(&db_path, config.interest.clone())
                .map_err(|e| hermes_core::AgentError::Io(e))?;
            let ok = store
                .pin_topic(topic_id)
                .map_err(|e| hermes_core::AgentError::Io(e))?;
            if ok {
                println!("Pinned topic {topic_id} (active, always shown in prompt)");
            } else {
                println!("Topic not found: {topic_id}");
            }
        }
        "disable" => {
            let cfg_path = hermes_config::config_path();
            let mut disk = hermes_config::load_user_config_file(&cfg_path)
                .map_err(|e| hermes_core::AgentError::Config(e.to_string()))?;
            disk.interest.enabled = false;
            hermes_config::save_config_yaml(&cfg_path, &disk)
                .map_err(|e| hermes_core::AgentError::Config(e.to_string()))?;
            println!("User interest (POI) extraction disabled (interest.enabled = false).");
            println!("  Existing topics remain in {}", db_path.display());
            println!("  Config: {}", cfg_path.display());
        }
        other => {
            println!("Unknown interest action '{}'.", other);
            println!(
                "Available actions: list, status, clear, prune, enable, disable, preview, reject, pin"
            );
            println!("  enable flags: --mode rules|hybrid|llm  --llm-on-session-end");
        }
    }
    Ok(())
}

fn hermes_home_from_config(config: &hermes_config::GatewayConfig) -> std::path::PathBuf {
    config
        .home_dir
        .as_ref()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(hermes_config::hermes_home)
}

/// Handle `hermes contribute [status|enable|disable|preview|flush|reset|revoke]`.
pub async fn handle_cli_contribute(
    action: Option<String>,
    poi_only: bool,
    skills_only: bool,
    _last_session: bool,
    outbox_clear: bool,
) -> Result<(), hermes_core::AgentError> {
    let config = hermes_config::load_config(None).unwrap_or_default();
    let hermes_home = hermes_home_from_config(&config);
    let contribution = config.insights.contribution.clone();

    match action.as_deref().unwrap_or("status") {
        "status" | "list" => {
            println!("Insights contribution (domain work packages → ops server)");
            println!("  Master enabled: {}", contribution.enabled);
            println!("  On session end: {}", contribution.on_session_end);
            println!("  Min evidence tier: {}", contribution.min_evidence_tier);
            println!(
                "  Require skill binding: {}",
                contribution.require_skill_binding
            );
            println!("  Min work turns: {}", contribution.min_work_turns);
            println!("  Redacted body: {}", contribution.redacted_body);
            println!(
                "  Endpoint: {}",
                if contribution.endpoint.trim().is_empty() {
                    "(not set — outbox only)".to_string()
                } else {
                    contribution.endpoint.clone()
                }
            );
            let auth_set = contribution.effective_token().is_some();
            println!(
                "  Authorization (Bearer): {}",
                if auth_set {
                    "(configured)".to_string()
                } else {
                    "(not set — required for upload)".to_string()
                }
            );
            println!("  Upload ready: {}", contribution.upload_ready());
            let svc = hermes_insights::ContributionService::open(
                hermes_home.clone(),
                contribution.clone(),
            )
            .map_err(|e| hermes_core::AgentError::Io(e))?;
            let counts = svc
                .outbox_counts()
                .map_err(|e| hermes_core::AgentError::Io(e))?;
            println!(
                "  Outbox: {} pending, {} failed, {} sent",
                counts.pending, counts.failed, counts.sent
            );
            let install_id = hermes_insights::paths::load_or_create_installation_id(&hermes_home)
                .unwrap_or_else(|_| "(unknown)".to_string());
            println!("  Installation id: {install_id}");
            println!("  Local POI extraction: {}", config.interest.enabled);
        }
        "enable" | "on" => {
            let cfg_path = hermes_config::config_path();
            let mut disk = hermes_config::load_user_config_file(&cfg_path)
                .map_err(|e| hermes_core::AgentError::Config(e.to_string()))?;
            let _ = poi_only;
            let _ = skills_only;
            disk.insights.contribution.enabled = true;
            hermes_config::save_config_yaml(&cfg_path, &disk)
                .map_err(|e| hermes_core::AgentError::Config(e.to_string()))?;
            println!("Insights contribution updated.");
            println!(
                "  Consent version: {}",
                hermes_insights::INSIGHTS_CONSENT_VERSION
            );
            println!("  Upload type: domain_work_package (POI + skill + resolution verdict).");
            println!("  Config: {}", cfg_path.display());
            if disk.insights.contribution.endpoint.trim().is_empty() {
                println!("  Note: set endpoint via:");
                println!("    hermes config set insights.contribution.endpoint <url>");
                println!("    or env HERMES_INSIGHTS_ENDPOINT");
            }
            if disk.insights.contribution.effective_token().is_none() {
                println!(
                    "  Note: server requires Authorization Bearer (user JWT or flowy- API key):"
                );
                println!("    hermes config set insights.contribution.auth_token <jwt-or-api-key>");
                println!("    or export HERMES_INSIGHTS_TOKEN=...");
                println!("    (JWT may be hardcoded in config.yaml for now)");
            }
        }
        "disable" | "off" => {
            let cfg_path = hermes_config::config_path();
            let mut disk = hermes_config::load_user_config_file(&cfg_path)
                .map_err(|e| hermes_core::AgentError::Config(e.to_string()))?;
            let _ = poi_only;
            let _ = skills_only;
            disk.insights.contribution.enabled = false;
            hermes_config::save_config_yaml(&cfg_path, &disk)
                .map_err(|e| hermes_core::AgentError::Config(e.to_string()))?;
            println!(
                "Insights contribution settings saved to {}",
                cfg_path.display()
            );
        }
        "preview" => {
            let svc = hermes_insights::ContributionService::open(
                hermes_home.clone(),
                contribution.clone(),
            )
            .map_err(|e| hermes_core::AgentError::Io(e))?;
            let batch = svc.preview_batch_from_inputs(&[]);
            let json = serde_json::to_string_pretty(&batch)
                .map_err(|e| hermes_core::AgentError::Config(e.to_string()))?;
            println!("{json}");
            println!(
                "\n(preview — run a session with skill_manage + domain task to populate packages)"
            );
        }
        "flush" | "upload" => {
            if contribution.endpoint.trim().is_empty() {
                println!("No insights.contribution.endpoint configured; skipping upload.");
                println!("Pending items remain in the local outbox.");
                return Ok(());
            }
            if contribution.effective_token().is_none() {
                println!("No Authorization Bearer configured; skipping upload.");
                println!(
                    "Set: hermes config set insights.contribution.auth_token <jwt-or-api-key>"
                );
                println!(" or: export HERMES_INSIGHTS_TOKEN=...");
                return Ok(());
            }
            let svc = hermes_insights::ContributionService::open(hermes_home, contribution)
                .map_err(|e| hermes_core::AgentError::Io(e))?;
            match svc.flush().await {
                Ok(result) => {
                    if result.skipped_no_endpoint {
                        println!("Upload skipped (no endpoint).");
                    } else {
                        println!(
                            "Upload complete: {} accepted, {} duplicates, {} rejected",
                            result.uploaded, result.duplicates, result.rejected
                        );
                        if result.duplicates > 0 && result.uploaded == 0 {
                            println!(
                                "  Note: server dedupes by content_hash; rows were not updated."
                            );
                            println!(
                                "  Inspect local payload: ~/.hermes-agent-ultra/insights/last_batch.json"
                            );
                        } else {
                            println!(
                                "  Upload payload saved: ~/.hermes-agent-ultra/insights/last_batch.json"
                            );
                        }
                    }
                }
                Err(e) => {
                    return Err(hermes_core::AgentError::Io(e));
                }
            }
        }
        "revoke" => {
            if contribution.endpoint.trim().is_empty() {
                println!("No endpoint configured; cannot revoke installation on server.");
                return Ok(());
            }
            let svc = hermes_insights::ContributionService::open(hermes_home, contribution)
                .map_err(|e| hermes_core::AgentError::Io(e))?;
            svc.revoke_installation()
                .await
                .map_err(|e| hermes_core::AgentError::Io(e))?;
            println!("Installation revocation request sent to server.");
        }
        "reset" | "requeue" => {
            let svc = hermes_insights::ContributionService::open(hermes_home, contribution)
                .map_err(|e| hermes_core::AgentError::Io(e))?;
            let n = svc
                .reset_outbox(outbox_clear)
                .map_err(|e| hermes_core::AgentError::Io(e))?;
            if outbox_clear {
                println!("Outbox cleared ({n} row(s) deleted).");
            } else {
                println!("Outbox reset: {n} row(s) moved to pending (sent/failed → pending).");
            }
            let counts = svc
                .outbox_counts()
                .map_err(|e| hermes_core::AgentError::Io(e))?;
            println!(
                "  Outbox now: {} pending, {} failed, {} sent",
                counts.pending, counts.failed, counts.sent
            );
            println!("Run `hermes contribute flush` to upload again.");
        }
        other => {
            println!("Unknown contribute action '{}'.", other);
            println!("Available: status, enable, disable, preview, flush, reset, revoke");
            println!("Flags: --poi-only, --skills-only, --clear (with reset)");
        }
    }
    Ok(())
}

/// Handle `hermes mcp [action] [--server ...]`.
pub async fn handle_cli_mcp(
    action: Option<String>,
    name: Option<String>,
    server: Option<String>,
    url: Option<String>,
    command: Option<String>,
    parallel_tools: bool,
) -> Result<(), hermes_core::AgentError> {
    let config_dir = hermes_config::hermes_home();
    let mcp_config_path = config_dir.join("mcp_servers.json");
    let mcp_auth_path = config_dir.join("mcp_auth.json");
    let selected = name.clone().or(server.clone());

    match action.as_deref().unwrap_or("list") {
        "sentrux" | "setup-sentrux" | "sentrux-setup" => {
            let sentrux_present = upsert_sentrux_mcp_profile(&config_dir)?;
            if sentrux_present {
                println!(
                    "Detected '{}' on PATH. Configuring {} MCP profile...",
                    SENTRUX_MCP_COMMAND, SENTRUX_MCP_SERVER_NAME
                );
            } else {
                println!(
                    "Warning: '{}' is not currently on PATH. Adding MCP config anyway.",
                    SENTRUX_MCP_COMMAND
                );
                println!(
                    "Install sentrux, then run `hermes mcp test {}` to verify transport reachability.",
                    SENTRUX_MCP_SERVER_NAME
                );
            }

            println!(
                "Configured MCP server '{}' in:\n  - {}\n  - {}",
                SENTRUX_MCP_SERVER_NAME,
                mcp_config_path.display(),
                config_dir.join("config.yaml").display()
            );
            println!(
                "Runtime hint: use `/mcp` in-session to confirm, and `hermes mcp test {}` for transport checks.",
                SENTRUX_MCP_SERVER_NAME
            );
        }
        "sentrux-status" => {
            let (binary_on_path, from_json, from_yaml) = sentrux_mcp_status(&config_dir);
            println!(
                "Sentrux MCP status:\n  - binary_on_path: {}\n  - in_mcp_servers.json: {}\n  - in_config.yaml: {}",
                if binary_on_path { "yes" } else { "no" },
                yes_no(from_json),
                yes_no(from_yaml)
            );
        }
        "sentrux-remove" => {
            remove_sentrux_mcp_profile(&config_dir)?;
            println!(
                "Removed '{}' MCP profile from JSON + YAML config surfaces.",
                SENTRUX_MCP_SERVER_NAME
            );
        }
        "list" => {
            if !mcp_config_path.exists() {
                println!("No MCP servers configured ({})", mcp_config_path.display());
                println!("Add one with `hermes mcp add --server <name-or-url>`.");
                return Ok(());
            }
            let content = std::fs::read_to_string(&mcp_config_path)
                .map_err(|e| hermes_core::AgentError::Io(format!("Read error: {}", e)))?;
            let servers: serde_json::Value =
                serde_json::from_str(&content).unwrap_or(serde_json::json!({}));
            if let Some(obj) = servers.as_object() {
                if obj.is_empty() {
                    println!("No MCP servers configured.");
                } else {
                    println!("MCP servers ({}):", mcp_config_path.display());
                    for (name, cfg) in obj {
                        let url = cfg.get("url").and_then(|v| v.as_str()).unwrap_or("(stdio)");
                        let parallel = cfg
                            .get("supports_parallel_tool_calls")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        println!(
                            "  • {} — {}  [parallel_tool_calls:{}]",
                            name,
                            url,
                            if parallel { "on" } else { "off" }
                        );
                    }
                }
            }
        }
        "add" => {
            let (entry_name, entry, yaml_command, yaml_url, yaml_parallel) = if let Some(name) =
                name.as_deref().map(str::trim).filter(|s| !s.is_empty())
            {
                let entry = if let Some(url) = url.clone().filter(|v| !v.trim().is_empty()) {
                    serde_json::json!({
                        "url": url,
                        "enabled": true,
                        "supports_parallel_tool_calls": parallel_tools
                    })
                } else if let Some(command) = command.clone().filter(|v| !v.trim().is_empty()) {
                    serde_json::json!({
                        "command": command,
                        "enabled": true,
                        "supports_parallel_tool_calls": parallel_tools
                    })
                } else {
                    return Err(hermes_core::AgentError::Config(
                        "mcp add with positional name requires --url or --command".into(),
                    ));
                };
                (
                    name.to_string(),
                    entry,
                    command.clone().filter(|v| !v.trim().is_empty()),
                    url.clone().filter(|v| !v.trim().is_empty()),
                    parallel_tools,
                )
            } else {
                let srv = server
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| {
                        hermes_core::AgentError::Config(
                            "Missing server. Usage: hermes mcp add <name> --url <url> | --command <cmd> [--parallel-tools] (legacy: --server <name-or-url>)".into(),
                        )
                    })?;
                let (entry, yaml_url) = if srv.starts_with("http://") || srv.starts_with("https://")
                {
                    (
                        serde_json::json!({
                            "url": srv,
                            "enabled": true,
                            "supports_parallel_tool_calls": parallel_tools
                        }),
                        Some(srv.to_string()),
                    )
                } else {
                    (
                        serde_json::json!({
                            "url": srv,
                            "enabled": true,
                            "supports_parallel_tool_calls": parallel_tools
                        }),
                        Some(srv.to_string()),
                    )
                };
                (srv.to_string(), entry, None, yaml_url, parallel_tools)
            };
            println!("Adding MCP server: {}", entry_name);
            let mut servers: serde_json::Value = if mcp_config_path.exists() {
                let content = std::fs::read_to_string(&mcp_config_path)
                    .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;
                serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
            } else {
                serde_json::json!({})
            };
            if let Some(obj) = servers.as_object_mut() {
                obj.insert(entry_name.clone(), entry);
            }
            let json = serde_json::to_string_pretty(&servers)
                .map_err(|e| hermes_core::AgentError::Config(e.to_string()))?;
            std::fs::write(&mcp_config_path, json)
                .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;
            update_yaml_mcp_server(
                &config_dir,
                &entry_name,
                yaml_command,
                yaml_url,
                yaml_parallel,
                false,
            )?;
            println!(
                "MCP server '{}' added to {}",
                entry_name,
                mcp_config_path.display()
            );
            println!(
                "Synced MCP server '{}' into {}",
                entry_name,
                config_dir.join("config.yaml").display()
            );
        }
        "remove" => {
            let srv = selected.clone().ok_or_else(|| {
                hermes_core::AgentError::Config(
                    "Missing server name. Usage: hermes mcp remove <name>".into(),
                )
            })?;
            if !mcp_config_path.exists() {
                println!("No MCP config to modify.");
                return Ok(());
            }
            let content = std::fs::read_to_string(&mcp_config_path)
                .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;
            let mut servers: serde_json::Value =
                serde_json::from_str(&content).unwrap_or(serde_json::json!({}));
            if let Some(obj) = servers.as_object_mut() {
                if obj.remove(&srv).is_some() {
                    let json = serde_json::to_string_pretty(&servers)
                        .map_err(|e| hermes_core::AgentError::Config(e.to_string()))?;
                    std::fs::write(&mcp_config_path, json)
                        .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;
                    update_yaml_mcp_server(&config_dir, &srv, None, None, false, true)?;
                    println!("MCP server '{}' removed.", srv);
                    if mcp_auth_path.exists() {
                        let raw = std::fs::read_to_string(&mcp_auth_path).unwrap_or_default();
                        let mut auth: serde_json::Value =
                            serde_json::from_str(&raw).unwrap_or(serde_json::json!({}));
                        if let Some(auth_obj) = auth.as_object_mut() {
                            auth_obj.remove(&srv);
                            let out = serde_json::to_string_pretty(&auth).unwrap_or_default();
                            let _ = std::fs::write(&mcp_auth_path, out);
                        }
                    }
                } else {
                    println!("MCP server '{}' not found.", srv);
                }
            }
        }
        "serve" => {
            use hermes_skills::{FileSkillStore, SkillManager};
            use hermes_tools::ToolRegistry;

            eprintln!("Starting Hermes as MCP server on stdio...");

            let config = hermes_config::load_config(None)
                .map_err(|e| hermes_core::AgentError::Config(e.to_string()))?;
            let tool_registry = Arc::new(ToolRegistry::new());
            let terminal_backend = crate::terminal_backend::build_terminal_backend(&config);
            let skill_store = Arc::new(FileSkillStore::new(FileSkillStore::default_dir()));
            let skill_provider: Arc<dyn hermes_core::SkillProvider> =
                Arc::new(SkillManager::new(skill_store));
            hermes_tools::register_builtin_tools(&tool_registry, terminal_backend, skill_provider);

            let mcp_server = hermes_mcp::McpServer::new(tool_registry);
            let transport = Box::new(hermes_mcp::ServerStdioTransport::new());
            mcp_server
                .start(transport)
                .await
                .map_err(|e| hermes_core::AgentError::Io(format!("MCP server error: {}", e)))?;
        }
        "test" => {
            let srv = selected.clone().ok_or_else(|| {
                hermes_core::AgentError::Config(
                    "Missing server name. Usage: hermes mcp test <name>".into(),
                )
            })?;
            println!("Testing MCP server: {}...", srv);
            if !mcp_config_path.exists() {
                println!("No MCP config found.");
                return Ok(());
            }
            let content = std::fs::read_to_string(&mcp_config_path)
                .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;
            let servers: serde_json::Value =
                serde_json::from_str(&content).unwrap_or(serde_json::json!({}));
            match servers.get(&srv) {
                Some(cfg) => {
                    let url = cfg.get("url").and_then(|v| v.as_str()).unwrap_or("(stdio)");
                    let enabled = cfg.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
                    let parallel = cfg
                        .get("supports_parallel_tool_calls")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    println!("  Server: {}", srv);
                    println!("  URL: {}", url);
                    println!("  Enabled: {}", enabled);
                    println!(
                        "  Parallel tool calls: {}",
                        if parallel { "on" } else { "off" }
                    );
                    if url.starts_with("http") {
                        match reqwest::Client::new()
                            .get(url)
                            .timeout(std::time::Duration::from_secs(5))
                            .send()
                            .await
                        {
                            Ok(resp) => println!("  Status: {} (reachable)", resp.status()),
                            Err(e) => println!("  Status: unreachable ({})", e),
                        }
                    } else {
                        println!("  Status: stdio transport (not testable via HTTP)");
                    }
                }
                None => println!("Server '{}' not found in MCP config.", srv),
            }
        }
        "configure" => {
            let srv = selected.clone().ok_or_else(|| {
                hermes_core::AgentError::Config(
                    "Missing server name. Usage: hermes mcp configure <name>".into(),
                )
            })?;
            if !mcp_config_path.exists() {
                println!("No MCP config found. Add a server first with `hermes mcp add`.");
                return Ok(());
            }
            let content = std::fs::read_to_string(&mcp_config_path)
                .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;
            let servers: serde_json::Value =
                serde_json::from_str(&content).unwrap_or(serde_json::json!({}));
            match servers.get(&srv) {
                Some(cfg) => {
                    println!("Current config for '{}':", srv);
                    println!("{}", serde_json::to_string_pretty(cfg).unwrap_or_default());
                    println!("\nEdit {} to modify settings.", mcp_config_path.display());
                }
                None => println!("Server '{}' not found.", srv),
            }
        }
        "login" => {
            let srv = selected.clone().ok_or_else(|| {
                hermes_core::AgentError::Config(
                    "Missing server name. Usage: hermes mcp login <name>".into(),
                )
            })?;
            if !mcp_config_path.exists() {
                return Err(hermes_core::AgentError::Config(format!(
                    "No MCP config found at {}",
                    mcp_config_path.display()
                )));
            }
            let configured = std::fs::read_to_string(&mcp_config_path)
                .ok()
                .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
                .and_then(|v| v.get(&srv).cloned())
                .is_some();
            if !configured {
                return Err(hermes_core::AgentError::Config(format!(
                    "MCP server '{}' is not configured",
                    srv
                )));
            }

            let env_key = format!("MCP_{}_TOKEN", srv.to_uppercase().replace('-', "_"));
            let token_from_env = std::env::var(&env_key)
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty());
            let token = if let Some(v) = token_from_env {
                v
            } else {
                use std::io::{self, Write};
                print!("Token for '{}': ", srv);
                let _ = io::stdout().flush();
                let mut buf = String::new();
                io::stdin()
                    .read_line(&mut buf)
                    .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;
                buf.trim().to_string()
            };
            if token.is_empty() {
                return Err(hermes_core::AgentError::Config(
                    "Empty token; aborting mcp login".into(),
                ));
            }
            let mut auth: serde_json::Value = if mcp_auth_path.exists() {
                let raw = std::fs::read_to_string(&mcp_auth_path)
                    .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;
                serde_json::from_str(&raw).unwrap_or(serde_json::json!({}))
            } else {
                serde_json::json!({})
            };
            if let Some(obj) = auth.as_object_mut() {
                obj.insert(
                    srv.clone(),
                    serde_json::json!({
                        "token": token,
                        "updated_at": chrono::Utc::now().to_rfc3339(),
                    }),
                );
            }
            std::fs::write(
                &mcp_auth_path,
                serde_json::to_string_pretty(&auth)
                    .map_err(|e| hermes_core::AgentError::Config(e.to_string()))?,
            )
            .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;
            println!(
                "Stored MCP auth token for '{}' in {}",
                srv,
                mcp_auth_path.display()
            );
        }
        other => {
            println!("MCP action '{}' is not recognized.", other);
            println!(
                "Available actions: list, add, remove, serve, test, configure, login, sentrux, sentrux-status, sentrux-remove"
            );
        }
    }
    Ok(())
}

fn command_on_path(command: &str) -> bool {
    if command.trim().is_empty() {
        return false;
    }
    let candidate = Path::new(command);
    if candidate.components().count() > 1 {
        return candidate.exists();
    }
    std::env::var_os("PATH").is_some_and(|path_var| {
        std::env::split_paths(&path_var)
            .map(|p| p.join(command))
            .any(|p| p.exists())
    })
}

fn sentrux_entry() -> serde_json::Value {
    serde_json::json!({
        "command": SENTRUX_MCP_COMMAND,
        "args": [SENTRUX_MCP_ARG],
        "enabled": true,
        "supports_parallel_tool_calls": true
    })
}

fn update_yaml_mcp_server(
    config_dir: &Path,
    name: &str,
    command: Option<String>,
    url: Option<String>,
    supports_parallel_tool_calls: bool,
    remove: bool,
) -> Result<(), hermes_core::AgentError> {
    let cfg_path = config_dir.join("config.yaml");
    let mut cfg = hermes_config::load_user_config_file(&cfg_path)
        .map_err(|e| hermes_core::AgentError::Config(e.to_string()))?;
    cfg.mcp_servers.retain(|entry| entry.name != name);
    if !remove {
        cfg.mcp_servers.push(hermes_config::McpServerEntry {
            name: name.to_string(),
            command,
            url,
            supports_parallel_tool_calls,
        });
        cfg.mcp_servers.sort_by(|a, b| a.name.cmp(&b.name));
    }
    hermes_config::save_config_yaml(&cfg_path, &cfg)
        .map_err(|e| hermes_core::AgentError::Config(e.to_string()))
}

pub(crate) fn upsert_sentrux_mcp_profile(config_dir: &Path) -> Result<bool, hermes_core::AgentError> {
    let mcp_config_path = config_dir.join("mcp_servers.json");
    let mut servers: serde_json::Value = if mcp_config_path.exists() {
        let content = std::fs::read_to_string(&mcp_config_path)
            .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };
    if let Some(obj) = servers.as_object_mut() {
        obj.insert(SENTRUX_MCP_SERVER_NAME.to_string(), sentrux_entry());
    }
    let json = serde_json::to_string_pretty(&servers)
        .map_err(|e| hermes_core::AgentError::Config(e.to_string()))?;
    std::fs::write(&mcp_config_path, json)
        .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;
    update_yaml_mcp_server(
        config_dir,
        SENTRUX_MCP_SERVER_NAME,
        Some(format!("{SENTRUX_MCP_COMMAND} {SENTRUX_MCP_ARG}")),
        None,
        true,
        false,
    )?;
    Ok(command_on_path(SENTRUX_MCP_COMMAND))
}

pub(crate) fn remove_sentrux_mcp_profile(config_dir: &Path) -> Result<(), hermes_core::AgentError> {
    let mcp_config_path = config_dir.join("mcp_servers.json");
    if mcp_config_path.exists() {
        let content = std::fs::read_to_string(&mcp_config_path)
            .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;
        let mut servers: serde_json::Value =
            serde_json::from_str(&content).unwrap_or(serde_json::json!({}));
        if let Some(obj) = servers.as_object_mut() {
            obj.remove(SENTRUX_MCP_SERVER_NAME);
        }
        let json = serde_json::to_string_pretty(&servers)
            .map_err(|e| hermes_core::AgentError::Config(e.to_string()))?;
        std::fs::write(&mcp_config_path, json)
            .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;
    }
    update_yaml_mcp_server(config_dir, SENTRUX_MCP_SERVER_NAME, None, None, false, true)
}

fn sentrux_mcp_status(config_dir: &Path) -> (bool, bool, bool) {
    let mcp_config_path = config_dir.join("mcp_servers.json");
    let from_json = std::fs::read_to_string(&mcp_config_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|v| v.get(SENTRUX_MCP_SERVER_NAME).cloned())
        .is_some();
    let from_yaml = hermes_config::load_user_config_file(&config_dir.join("config.yaml"))
        .ok()
        .map(|cfg| {
            cfg.mcp_servers
                .iter()
                .any(|entry| entry.name == SENTRUX_MCP_SERVER_NAME)
        })
        .unwrap_or(false);
    (command_on_path(SENTRUX_MCP_COMMAND), from_json, from_yaml)
}

/// Handle `hermes sessions [action] [--id ...] [--name ...]`.
pub async fn handle_cli_sessions(
    action: Option<String>,
    id: Option<String>,
    name: Option<String>,
) -> Result<(), hermes_core::AgentError> {
    let sessions_dir = hermes_config::hermes_home().join("sessions");

    match action.as_deref().unwrap_or("list") {
        "list" => {
            if !sessions_dir.exists() {
                println!("No sessions directory found.");
                return Ok(());
            }
            let mut entries: Vec<(String, u64, std::time::SystemTime, bool, bool, usize)> =
                Vec::new();
            if let Ok(rd) = std::fs::read_dir(&sessions_dir) {
                for entry in rd.filter_map(|e| e.ok()) {
                    let path = entry.path();
                    if path.extension().map(|e| e == "json").unwrap_or(false) {
                        let stem = path
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned();
                        let meta = std::fs::metadata(&path);
                        let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
                        let modified = meta
                            .and_then(|m| m.modified())
                            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                        let integrity = session::inspect_snapshot_integrity(&path);
                        let canonical = session::is_canonical_snapshot_name(&stem, &integrity);
                        entries.push((
                            stem,
                            size,
                            modified,
                            canonical,
                            integrity.valid,
                            integrity.message_count,
                        ));
                    }
                }
            }
            entries.sort_by(|a, b| {
                b.3.cmp(&a.3)
                    .then_with(|| b.5.cmp(&a.5))
                    .then_with(|| b.2.cmp(&a.2))
                    .then_with(|| a.0.cmp(&b.0))
            });
            if entries.is_empty() {
                println!("No saved sessions.");
            } else {
                let canonical_count = entries.iter().filter(|entry| entry.3).count();
                let artifact_count = entries.len().saturating_sub(canonical_count);
                println!(
                    "Saved sessions ({} total; {} canonical; {} artifacts):",
                    entries.len(),
                    canonical_count,
                    artifact_count
                );
                for (name, size, _, canonical, valid, messages) in &entries {
                    let kind = if *canonical {
                        "session"
                    } else if *valid {
                        "artifact"
                    } else {
                        "invalid"
                    };
                    println!("  • {} ({} bytes, {} msgs, {})", name, size, messages, kind);
                }
            }
        }
        "export" => {
            let session_id = id.ok_or_else(|| {
                hermes_core::AgentError::Config(
                    "Missing session ID. Usage: hermes sessions export --id <id>".into(),
                )
            })?;
            let path = sessions_dir.join(format!("{}.json", session_id));
            if !path.exists() {
                println!("Session '{}' not found.", session_id);
                return Ok(());
            }
            let content = std::fs::read_to_string(&path)
                .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;
            println!("{}", content);
        }
        "delete" => {
            let session_id = id.ok_or_else(|| {
                hermes_core::AgentError::Config(
                    "Missing session ID. Usage: hermes sessions delete --id <id>".into(),
                )
            })?;
            let path = sessions_dir.join(format!("{}.json", session_id));
            if path.exists() {
                std::fs::remove_file(&path)
                    .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;
                println!("Session '{}' deleted.", session_id);
            } else {
                println!("Session '{}' not found.", session_id);
            }
        }
        "stats" => {
            if !sessions_dir.exists() {
                println!("No sessions directory.");
                return Ok(());
            }
            let mut total_files = 0u32;
            let mut total_size = 0u64;
            if let Ok(rd) = std::fs::read_dir(&sessions_dir) {
                for entry in rd.filter_map(|e| e.ok()) {
                    if entry
                        .path()
                        .extension()
                        .map(|e| e == "json")
                        .unwrap_or(false)
                    {
                        total_files += 1;
                        total_size += std::fs::metadata(entry.path())
                            .map(|m| m.len())
                            .unwrap_or(0);
                    }
                }
            }
            println!("Session statistics:");
            println!("  Total sessions: {}", total_files);
            println!("  Total size:     {} KB", total_size / 1024);
            println!("  Directory:      {}", sessions_dir.display());
        }
        "prune" => {
            let max_age_days: u64 = name
                .as_deref()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(30);
            println!("Pruning sessions older than {} days...", max_age_days);
            if !sessions_dir.exists() {
                println!("No sessions directory.");
                return Ok(());
            }
            let cutoff = std::time::SystemTime::now()
                .checked_sub(std::time::Duration::from_secs(max_age_days * 86400))
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            let mut pruned = 0u32;
            if let Ok(rd) = std::fs::read_dir(&sessions_dir) {
                for entry in rd.filter_map(|e| e.ok()) {
                    let path = entry.path();
                    if !path.extension().map(|e| e == "json").unwrap_or(false) {
                        continue;
                    }
                    if let Ok(meta) = std::fs::metadata(&path) {
                        let modified = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                        if modified < cutoff {
                            if std::fs::remove_file(&path).is_ok() {
                                let name = path.file_stem().unwrap_or_default().to_string_lossy();
                                println!("  Pruned: {}", name);
                                pruned += 1;
                            }
                        }
                    }
                }
            }
            println!("Pruned {} session(s).", pruned);
        }
        "rename" => {
            let session_id = id.ok_or_else(|| {
                hermes_core::AgentError::Config(
                    "Missing session ID. Usage: hermes sessions rename --id <id> --name <new>"
                        .into(),
                )
            })?;
            let new_name = name.ok_or_else(|| {
                hermes_core::AgentError::Config(
                    "Missing new name. Usage: hermes sessions rename --id <id> --name <new>".into(),
                )
            })?;
            let old_path = sessions_dir.join(format!("{}.json", session_id));
            let new_path = sessions_dir.join(format!("{}.json", new_name));
            if !old_path.exists() {
                println!("Session '{}' not found.", session_id);
                return Ok(());
            }
            std::fs::rename(&old_path, &new_path)
                .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;
            println!("Session renamed: {} -> {}", session_id, new_name);
        }
        "browse" => {
            if !sessions_dir.exists() {
                println!("No sessions directory found.");
                return Ok(());
            }
            println!("Session Browser");
            println!("===============\n");
            let mut entries: Vec<(String, u64, std::time::SystemTime, usize)> = Vec::new();
            if let Ok(rd) = std::fs::read_dir(&sessions_dir) {
                for entry in rd.filter_map(|e| e.ok()) {
                    let path = entry.path();
                    if !path.extension().map(|e| e == "json").unwrap_or(false) {
                        continue;
                    }
                    let stem = path
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned();
                    let meta = std::fs::metadata(&path);
                    let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
                    let modified = meta
                        .as_ref()
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                    let msg_count = std::fs::read_to_string(&path)
                        .ok()
                        .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
                        .and_then(|v| {
                            v.get("messages")
                                .and_then(|m| m.as_array())
                                .map(|a| a.len())
                        })
                        .unwrap_or(0);
                    entries.push((stem, size, modified, msg_count));
                }
            }
            entries.sort_by(|a, b| b.2.cmp(&a.2));
            if entries.is_empty() {
                println!("No sessions found.");
            } else {
                println!(
                    "{:3} {:30} {:>8} {:>6}  {}",
                    "#", "Session ID", "Size", "Msgs", "Modified"
                );
                println!("{}", "-".repeat(75));
                for (idx, (name, size, modified, msgs)) in entries.iter().enumerate() {
                    let age = modified.elapsed().unwrap_or_default();
                    let age_str = if age.as_secs() < 3600 {
                        format!("{}m ago", age.as_secs() / 60)
                    } else if age.as_secs() < 86400 {
                        format!("{}h ago", age.as_secs() / 3600)
                    } else {
                        format!("{}d ago", age.as_secs() / 86400)
                    };
                    println!(
                        "{:3} {:30} {:>6}KB {:>6}  {}",
                        idx + 1,
                        &name[..name.len().min(30)],
                        size / 1024,
                        msgs,
                        age_str,
                    );
                }
                println!("\nUse `hermes sessions export --id <id>` to view a session.");
            }
        }
        other => {
            println!("Sessions action '{}' is not recognized.", other);
            println!("Available actions: list, export, delete, prune, stats, rename, browse");
        }
    }
    Ok(())
}

/// Handle `hermes insights [--days N] [--source ...]`.
pub async fn handle_cli_insights(
    days: u32,
    source: Option<String>,
) -> Result<(), hermes_core::AgentError> {
    println!("Usage Insights (last {} days)", days);
    println!("=============================");
    if let Some(src) = &source {
        println!("Filter: source={}\n", src);
    }
    let sessions_dir = hermes_config::hermes_home().join("sessions");
    if !sessions_dir.exists() {
        println!("No sessions directory found.");
        return Ok(());
    }

    let cutoff = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(u64::from(days) * 86400))
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

    let mut total_sessions = 0u32;
    let mut total_messages = 0u64;
    let mut total_input_tokens = 0u64;
    let mut total_output_tokens = 0u64;
    let mut total_cost_cents = 0.0f64;
    let mut models_used: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    let mut daily_counts: std::collections::BTreeMap<String, u32> =
        std::collections::BTreeMap::new();

    if let Ok(rd) = std::fs::read_dir(&sessions_dir) {
        for entry in rd.filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.extension().map(|e| e == "json").unwrap_or(false) {
                continue;
            }
            let meta = std::fs::metadata(&path);
            let modified = meta
                .as_ref()
                .ok()
                .and_then(|m| m.modified().ok())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            if modified < cutoff {
                continue;
            }

            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(src_filter) = &source {
                        let session_source = data
                            .get("source")
                            .and_then(|s| s.as_str())
                            .unwrap_or("unknown");
                        if session_source != src_filter.as_str() {
                            continue;
                        }
                    }

                    total_sessions += 1;

                    if let Some(msgs) = data.get("messages").and_then(|m| m.as_array()) {
                        total_messages += msgs.len() as u64;
                    }

                    if let Some(usage) = data.get("usage") {
                        total_input_tokens += usage
                            .get("input_tokens")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                        total_output_tokens += usage
                            .get("output_tokens")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                        total_cost_cents +=
                            usage.get("cost").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    }

                    if let Some(model) = data.get("model").and_then(|m| m.as_str()) {
                        *models_used.entry(model.to_string()).or_insert(0) += 1;
                    }

                    let dur = modified
                        .duration_since(std::time::SystemTime::UNIX_EPOCH)
                        .unwrap_or_default();
                    let secs = dur.as_secs();
                    let day_secs = secs - (secs % 86400);
                    let day_key = format!("{}", day_secs / 86400);
                    *daily_counts.entry(day_key).or_insert(0) += 1;
                }
            }
        }
    }

    println!("Sessions:       {}", total_sessions);
    println!("Messages:       {}", total_messages);
    println!("Input tokens:   {}", total_input_tokens);
    println!("Output tokens:  {}", total_output_tokens);
    let total_tokens = total_input_tokens + total_output_tokens;
    println!("Total tokens:   {}", total_tokens);
    if total_cost_cents > 0.0 {
        println!("Estimated cost: ${:.4}", total_cost_cents / 100.0);
    }

    if !models_used.is_empty() {
        println!("\nModels Used:");
        let mut model_vec: Vec<_> = models_used.into_iter().collect();
        model_vec.sort_by(|a, b| b.1.cmp(&a.1));
        for (model, count) in &model_vec {
            println!("  {:30} {:>5} session(s)", model, count);
        }
    }

    if total_sessions > 0 {
        println!("\nAverages per session:");
        println!(
            "  Messages: {:.1}",
            total_messages as f64 / total_sessions as f64
        );
        println!(
            "  Tokens:   {:.0}",
            total_tokens as f64 / total_sessions as f64
        );
    }

    Ok(())
}

/// Handle `hermes login [provider]`.
pub async fn handle_cli_login(provider: Option<String>) -> Result<(), hermes_core::AgentError> {
    let provider = provider.unwrap_or_else(|| "openai".to_string());
    let creds_dir = hermes_config::hermes_home().join("credentials");
    std::fs::create_dir_all(&creds_dir).map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;

    println!("Login to: {}", provider);
    println!("----------{}", "-".repeat(provider.len()));

    match provider.as_str() {
        "openai" => {
            let env_key = std::env::var("HERMES_OPENAI_API_KEY")
                .ok()
                .or_else(|| std::env::var("OPENAI_API_KEY").ok());
            if let Some(key) = env_key {
                let masked = if key.len() > 8 {
                    format!("{}...{}", &key[..4], &key[key.len() - 4..])
                } else {
                    "****".to_string()
                };
                println!(
                    "Found HERMES_OPENAI_API_KEY/OPENAI_API_KEY in environment: {}",
                    masked
                );
                let cred_file = creds_dir.join("openai.json");
                let cred = serde_json::json!({
                    "provider": "openai",
                    "api_key_masked": masked,
                    "stored_at": chrono::Utc::now().to_rfc3339(),
                    "source": "env",
                });
                std::fs::write(
                    &cred_file,
                    serde_json::to_string_pretty(&cred).unwrap_or_default(),
                )
                .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;
                println!("Credential reference stored at {}", cred_file.display());
            } else {
                println!("No HERMES_OPENAI_API_KEY/OPENAI_API_KEY found in environment.");
                println!("Set it with: export HERMES_OPENAI_API_KEY=sk-...");
                println!("Or use: hermes config set openai_api_key <key>");
            }
        }
        "anthropic" => {
            let env_key = std::env::var("ANTHROPIC_API_KEY").ok();
            if let Some(key) = env_key {
                let masked = if key.len() > 8 {
                    format!("{}...{}", &key[..4], &key[key.len() - 4..])
                } else {
                    "****".to_string()
                };
                println!("Found ANTHROPIC_API_KEY in environment: {}", masked);
                let cred_file = creds_dir.join("anthropic.json");
                let cred = serde_json::json!({
                    "provider": "anthropic",
                    "api_key_masked": masked,
                    "stored_at": chrono::Utc::now().to_rfc3339(),
                    "source": "env",
                });
                std::fs::write(
                    &cred_file,
                    serde_json::to_string_pretty(&cred).unwrap_or_default(),
                )
                .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;
                println!("Credential reference stored at {}", cred_file.display());
            } else {
                println!("No ANTHROPIC_API_KEY found in environment.");
                println!("Set it with: export ANTHROPIC_API_KEY=sk-ant-...");
            }
        }
        other => {
            let env_var = format!("{}_API_KEY", other.to_uppercase().replace('-', "_"));
            let env_key = std::env::var(&env_var).ok();
            if let Some(key) = env_key {
                let masked = if key.len() > 8 {
                    format!("{}...{}", &key[..4], &key[key.len() - 4..])
                } else {
                    "****".to_string()
                };
                println!("Found {} in environment: {}", env_var, masked);
                let cred_file = creds_dir.join(format!("{}.json", other));
                let cred = serde_json::json!({
                    "provider": other,
                    "api_key_masked": masked,
                    "stored_at": chrono::Utc::now().to_rfc3339(),
                    "source": "env",
                });
                std::fs::write(
                    &cred_file,
                    serde_json::to_string_pretty(&cred).unwrap_or_default(),
                )
                .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;
                println!("Credential reference stored.");
            } else {
                println!("No {} found in environment.", env_var);
                println!("Set it with: export {}=<your-key>", env_var);
            }
        }
    }
    Ok(())
}

/// Handle `hermes logout [provider]`.
pub async fn handle_cli_logout(provider: Option<String>) -> Result<(), hermes_core::AgentError> {
    let creds_dir = hermes_config::hermes_home().join("credentials");

    match provider.as_deref() {
        Some(p) => {
            let cred_file = creds_dir.join(format!("{}.json", p));
            if cred_file.exists() {
                std::fs::remove_file(&cred_file)
                    .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;
                println!("Logged out from '{}'. Credential reference removed.", p);
            } else {
                println!("No stored credentials for '{}'.", p);
            }
            println!(
                "Note: Environment variables (e.g. {}_API_KEY) are not affected.",
                p.to_uppercase().replace('-', "_")
            );
        }
        None => {
            if creds_dir.exists() {
                let mut removed = 0u32;
                if let Ok(rd) = std::fs::read_dir(&creds_dir) {
                    for entry in rd.filter_map(|e| e.ok()) {
                        let path = entry.path();
                        if path.extension().map(|e| e == "json").unwrap_or(false) {
                            if std::fs::remove_file(&path).is_ok() {
                                let name = path.file_stem().unwrap_or_default().to_string_lossy();
                                println!("  Removed credential: {}", name);
                                removed += 1;
                            }
                        }
                    }
                }
                if removed == 0 {
                    println!("No stored credentials to remove.");
                } else {
                    println!("Logged out from {} provider(s).", removed);
                }
            } else {
                println!("No credentials directory found.");
            }
            println!("Note: Environment variables are not affected.");
        }
    }
    Ok(())
}

/// Handle `hermes whatsapp [action]`.
pub async fn handle_cli_whatsapp(action: Option<String>) -> Result<(), hermes_core::AgentError> {
    match action.as_deref().unwrap_or("setup") {
        "setup" | "" => crate::whatsapp_wizard::whatsapp_baileys_wizard().await,
        "status" => crate::whatsapp_wizard::whatsapp_baileys_status().await,
        "pair" | "qr" => crate::whatsapp_wizard::whatsapp_baileys_wizard().await,
        "cloud" => crate::whatsapp_wizard::whatsapp_cloud_setup().await,
        other => {
            println!("WhatsApp action '{}' is not recognized.", other);
            println!("Available actions: setup, status, pair, cloud");
            Ok(())
        }
    }
}

/// Cloud API setup (optional feature `whatsapp-cloud`).
pub(crate) async fn whatsapp_cloud_setup_impl() -> Result<(), hermes_core::AgentError> {
    whatsapp_cloud_setup_legacy().await
}

async fn whatsapp_cloud_setup_legacy() -> Result<(), hermes_core::AgentError> {
    use std::io::{self, BufRead, Write};

    println!("WhatsApp Cloud API Setup");
    println!("========================\n");
    println!("You will need credentials from the Meta developer dashboard:");
    println!("  https://developers.facebook.com/apps/\n");

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    print!("Phone Number ID: ");
    stdout.flush().ok();
    let phone_number_id = stdin
        .lock()
        .lines()
        .next()
        .and_then(|l| l.ok())
        .unwrap_or_default()
        .trim()
        .to_string();

    if phone_number_id.is_empty() {
        println!("Aborted: phone number ID is required.");
        return Ok(());
    }

    print!("Business Account ID: ");
    stdout.flush().ok();
    let business_account_id = stdin
        .lock()
        .lines()
        .next()
        .and_then(|l| l.ok())
        .unwrap_or_default()
        .trim()
        .to_string();

    if business_account_id.is_empty() {
        println!("Aborted: business account ID is required.");
        return Ok(());
    }

    print!("Access Token: ");
    stdout.flush().ok();
    let access_token = stdin
        .lock()
        .lines()
        .next()
        .and_then(|l| l.ok())
        .unwrap_or_default()
        .trim()
        .to_string();

    if access_token.is_empty() {
        println!("Aborted: access token is required.");
        return Ok(());
    }

    println!("\nVerifying token against WhatsApp Cloud API...");
    let url = format!(
        "https://graph.facebook.com/v21.0/{}/messages",
        phone_number_id
    );
    let client = reqwest::Client::new();
    match client
        .get(&url)
        .bearer_auth(&access_token)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status();
            if status.is_success() || status.as_u16() == 400 {
                // 400 means the endpoint is reachable (POST required for actual messages)
                println!("  API reachable (HTTP {}).", status);
            } else if status.as_u16() == 401 || status.as_u16() == 403 {
                println!("  Warning: API returned {} — token may be invalid.", status);
                println!("  Saving anyway; you can re-run setup later.");
            } else {
                println!("  API returned HTTP {}. Saving config anyway.", status);
            }
        }
        Err(e) => {
            println!("  Could not reach API: {}", e);
            println!("  Saving config anyway — verify network connectivity.");
        }
    }

    let config_path = hermes_config::hermes_home().join("config.yaml");
    let mut config: serde_yaml::Value = if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)
            .map_err(|e| hermes_core::AgentError::Io(format!("Read error: {}", e)))?;
        serde_yaml::from_str(&content).unwrap_or(serde_yaml::Value::Mapping(Default::default()))
    } else {
        serde_yaml::Value::Mapping(Default::default())
    };

    let platforms = config
        .as_mapping_mut()
        .unwrap()
        .entry(serde_yaml::Value::String("platforms".into()))
        .or_insert_with(|| serde_yaml::Value::Mapping(Default::default()));

    let wa = platforms
        .as_mapping_mut()
        .unwrap()
        .entry(serde_yaml::Value::String("whatsapp".into()))
        .or_insert_with(|| serde_yaml::Value::Mapping(Default::default()));

    let wa_map = wa.as_mapping_mut().unwrap();
    wa_map.insert(
        serde_yaml::Value::String("phone_number_id".into()),
        serde_yaml::Value::String(phone_number_id.clone()),
    );
    wa_map.insert(
        serde_yaml::Value::String("business_account_id".into()),
        serde_yaml::Value::String(business_account_id),
    );
    wa_map.insert(
        serde_yaml::Value::String("access_token".into()),
        serde_yaml::Value::String(access_token),
    );
    wa_map.insert(
        serde_yaml::Value::String("enabled".into()),
        serde_yaml::Value::Bool(true),
    );

    let yaml_str = serde_yaml::to_string(&config)
        .map_err(|e| hermes_core::AgentError::Config(e.to_string()))?;
    std::fs::create_dir_all(hermes_config::hermes_home())
        .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;
    std::fs::write(&config_path, &yaml_str)
        .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;

    println!(
        "\nWhatsApp configuration saved to {}",
        config_path.display()
    );
    println!("Phone Number ID: {}", phone_number_id);
    println!("\nRun `hermes whatsapp status` to verify.");
    Ok(())
}

/// Check whether WhatsApp is configured and verify connectivity.
async fn whatsapp_status() -> Result<(), hermes_core::AgentError> {
    let config_path = hermes_config::hermes_home().join("config.yaml");
    if !config_path.exists() {
        println!("WhatsApp: not configured");
        println!("Run `hermes whatsapp setup` to configure.");
        return Ok(());
    }

    let content = std::fs::read_to_string(&config_path)
        .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;
    let config: serde_yaml::Value =
        serde_yaml::from_str(&content).unwrap_or(serde_yaml::Value::Mapping(Default::default()));

    let wa = config.get("platforms").and_then(|p| p.get("whatsapp"));

    match wa {
        None => {
            println!("WhatsApp: not configured");
            println!("Run `hermes whatsapp setup` to configure.");
        }
        Some(wa_cfg) => {
            let phone_id = wa_cfg
                .get("phone_number_id")
                .and_then(|v| v.as_str())
                .unwrap_or("(not set)");
            let enabled = wa_cfg
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let has_token = wa_cfg
                .get("access_token")
                .and_then(|v| v.as_str())
                .map(|t| !t.is_empty())
                .unwrap_or(false);

            println!("WhatsApp Status");
            println!("---------------");
            println!("  Configured:     yes");
            println!("  Enabled:        {}", enabled);
            println!("  Phone Number ID: {}", phone_id);
            println!(
                "  Access Token:   {}",
                if has_token { "present" } else { "missing" }
            );

            if has_token {
                let token = wa_cfg.get("access_token").unwrap().as_str().unwrap();
                let url = format!("https://graph.facebook.com/v21.0/{}/messages", phone_id);
                print!("  API Connectivity: ");
                match reqwest::Client::new()
                    .get(&url)
                    .bearer_auth(token)
                    .timeout(std::time::Duration::from_secs(10))
                    .send()
                    .await
                {
                    Ok(resp) => println!("reachable (HTTP {})", resp.status()),
                    Err(e) => println!("unreachable ({})", e),
                }
            }
        }
    }
    Ok(())
}

/// Connect to local bridge, fetch QR data, and render in terminal.
async fn whatsapp_qr() -> Result<(), hermes_core::AgentError> {
    let config_path = hermes_config::hermes_home().join("config.yaml");
    let bridge_url = if config_path.exists() {
        let content = std::fs::read_to_string(&config_path).unwrap_or_default();
        let config: serde_yaml::Value = serde_yaml::from_str(&content)
            .unwrap_or(serde_yaml::Value::Mapping(Default::default()));
        config
            .get("platforms")
            .and_then(|p| p.get("whatsapp"))
            .and_then(|w| w.get("bridge_url"))
            .and_then(|u| u.as_str())
            .unwrap_or("http://localhost:3000")
            .to_string()
    } else {
        "http://localhost:3000".to_string()
    };

    let qr_url = format!("{}/qr", bridge_url);
    println!("Fetching QR code from {}...", qr_url);

    match reqwest::Client::new()
        .get(&qr_url)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            let body = resp
                .text()
                .await
                .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;

            let qr_data = if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
                json.get("qr")
                    .or_else(|| json.get("data"))
                    .and_then(|v| v.as_str())
                    .unwrap_or(&body)
                    .to_string()
            } else {
                body
            };

            println!();
            render_qr_to_terminal(&qr_data);
            println!();
            println!("Scan this QR code with WhatsApp on your phone:");
            println!("  WhatsApp → Settings → Linked Devices → Link a Device");
        }
        Ok(resp) => {
            println!(
                "Bridge returned HTTP {}. Is the bridge server running?",
                resp.status()
            );
            println!("Start it with: npx hermes-whatsapp-bridge");
        }
        Err(e) => {
            println!("Could not connect to bridge at {}: {}", bridge_url, e);
            println!("\nMake sure the WhatsApp Web bridge is running:");
            println!("  npx hermes-whatsapp-bridge");
            println!("  # or: docker run -p 3000:3000 hermes/whatsapp-bridge");
        }
    }
    Ok(())
}

/// Render QR data as Unicode block art in the terminal.
fn render_qr_to_terminal(data: &str) {
    let code = match qrcode::QrCode::new(data.as_bytes()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("QR 码生成失败: {}", e);
            return;
        }
    };
    let side = code.width() as usize;
    let modules = code.to_colors();
    let padded = side + 8;
    let is_dark = |r: usize, c: usize| modules[r * side + c] == qrcode::Color::Dark;
    let mut row = 0usize;
    while row < padded {
        let mut line = String::new();
        for col in 0..padded {
            let qr_row = row.wrapping_sub(4);
            let qr_col = col.wrapping_sub(4);
            let top = qr_row < side && qr_col < side && is_dark(qr_row, qr_col);
            let qr_row2 = (row + 1).wrapping_sub(4);
            let bottom = qr_row2 < side && qr_col < side && is_dark(qr_row2, qr_col);
            line.push(match (top, bottom) {
                (true, true) => '█',
                (true, false) => '▀',
                (false, true) => '▄',
                (false, false) => ' ',
            });
        }
        println!("  {}", line);
        row += 2;
    }
}

/// Handle `hermes pairing`.
///
/// Supports both:
/// - Legacy device pairing (`--device-id`)
/// - Python-compatible DM pairing (`approve <platform> <code>`)
pub async fn handle_cli_pairing(
    action: Option<String>,
    device_id: Option<String>,
    args: Vec<String>,
) -> Result<(), hermes_core::AgentError> {
    use crate::pairing_store::{PairingStatus, PairingStore};
    use hermes_gateway::DmPairingStore;

    let store = PairingStore::open_default();
    let dm_store = DmPairingStore::open_default();
    let action = action.unwrap_or_else(|| "list".to_string());

    match action.as_str() {
        "list" => {
            let devices = store.list().map_err(|e| hermes_core::AgentError::Io(e))?;
            if devices.is_empty() {
                println!("No paired devices.");
                println!("  Store: {}", PairingStore::default_path().display());
            } else {
                println!("Paired devices ({}):", devices.len());
                println!(
                    "  {:20} {:10} {:12} {}",
                    "Device ID", "Status", "Last Seen", "Name"
                );
                println!("  {}", "-".repeat(60));
                for d in &devices {
                    let last_seen = d.last_seen.as_deref().unwrap_or("never");
                    let name = d.name.as_deref().unwrap_or("(unnamed)");
                    let status_icon = match d.status {
                        PairingStatus::Pending => "⏳",
                        PairingStatus::Approved => "✓",
                        PairingStatus::Revoked => "✗",
                    };
                    println!(
                        "  {:20} {} {:8} {:12} {}",
                        d.device_id, status_icon, d.status, last_seen, name
                    );
                }
            }
            let pending = dm_store.list_pending(None);
            let approved = dm_store.list_approved(None);
            if pending.is_empty() && approved.is_empty() {
                println!("No DM pairing data found.");
            } else {
                if !pending.is_empty() {
                    println!("\nPending DM pairing requests ({}):", pending.len());
                    println!(
                        "  {:10} {:12} {:20} {:20} {}",
                        "Platform", "Code*", "User ID", "Name", "Age"
                    );
                    println!("  {}", "-".repeat(80));
                    for p in pending {
                        println!(
                            "  {:10} {:12} {:20} {:20} {}m",
                            p.platform, p.code, p.user_id, p.user_name, p.age_minutes
                        );
                    }
                    println!("  * code is hash prefix for display only");
                }
                if !approved.is_empty() {
                    println!("\nApproved DM users ({}):", approved.len());
                    println!("  {:10} {:24} {}", "Platform", "User ID", "Name");
                    println!("  {}", "-".repeat(60));
                    for a in approved {
                        println!("  {:10} {:24} {}", a.platform, a.user_id, a.user_name);
                    }
                }
            }
        }
        "approve" => {
            if let Some(did) = device_id {
                match store.approve(&did) {
                    Ok(dev) => {
                        println!("Device '{}' approved.", dev.device_id);
                        if let Some(secret) = &dev.shared_secret {
                            if secret_stdout_allowed() {
                                println!("  Shared secret: {}", secret);
                                println!(
                                    "  (plaintext output enabled via HERMES_ALLOW_SECRET_STDOUT=1)"
                                );
                            } else {
                                println!("  Shared secret: {}", mask_secret_value(secret));
                                println!(
                                    "  (set HERMES_ALLOW_SECRET_STDOUT=1 to reveal plaintext once)"
                                );
                            }
                            println!("  (Store this securely — it will not be shown again)");
                        }
                    }
                    Err(e) => println!("Failed to approve device: {}", e),
                }
            } else if args.len() >= 2 {
                let platform = &args[0];
                let code = &args[1];
                match dm_store
                    .approve_code(platform, code)
                    .map_err(hermes_core::AgentError::Io)?
                {
                    Some(user) => {
                        let display = if user.user_name.trim().is_empty() {
                            user.user_id.clone()
                        } else {
                            format!("{} ({})", user.user_name, user.user_id)
                        };
                        println!(
                            "Approved! User {} on {} can now use DM access.",
                            display, platform
                        );
                    }
                    None => {
                        println!(
                            "Code '{}' not found, expired, or locked out on '{}'.",
                            code, platform
                        );
                    }
                }
            } else {
                return Err(hermes_core::AgentError::Config(
                    "Missing args. Usage: hermes pairing approve --device-id <id> OR hermes pairing approve <platform> <code>".into(),
                ));
            }
        }
        "revoke" => {
            if let Some(did) = device_id {
                match store.revoke(&did) {
                    Ok(dev) => {
                        println!("Device '{}' revoked.", dev.device_id);
                        println!("  The device will no longer be able to connect.");
                    }
                    Err(e) => println!("Failed to revoke device: {}", e),
                }
            } else if args.len() >= 2 {
                let platform = &args[0];
                let user_id = &args[1];
                let revoked = dm_store
                    .revoke(platform, user_id)
                    .map_err(hermes_core::AgentError::Io)?;
                if revoked {
                    println!("Revoked DM access for {} on {}.", user_id, platform);
                } else {
                    println!("User {} was not approved on {}.", user_id, platform);
                }
            } else {
                return Err(hermes_core::AgentError::Config(
                    "Missing args. Usage: hermes pairing revoke --device-id <id> OR hermes pairing revoke <platform> <user_id>".into(),
                ));
            }
        }
        "clear-pending" => {
            match store.clear_pending() {
                Ok(count) => {
                    if count == 0 {
                        println!("No pending pairing requests to clear.");
                    } else {
                        println!("Cleared {} pending pairing request(s).", count);
                    }
                }
                Err(e) => println!("Failed to clear pending requests: {}", e),
            }
            let platform = args.first().map(|s| s.as_str());
            match dm_store.clear_pending(platform) {
                Ok(count) => {
                    if platform.is_some() {
                        println!("Cleared {} pending DM requests.", count);
                    } else {
                        println!(
                            "Cleared {} pending DM requests across all platforms.",
                            count
                        );
                    }
                }
                Err(e) => println!("Failed to clear DM pending requests: {}", e),
            }
        }
        other => {
            println!("Pairing action '{}' is not recognized.", other);
            println!("Available actions: list, approve, revoke, clear-pending");
        }
    }
    Ok(())
}

/// Handle `hermes claw [action]`.
pub async fn handle_cli_claw(action: Option<String>) -> Result<(), hermes_core::AgentError> {
    match action.as_deref().unwrap_or("status") {
        "migrate" => {
            claw_migrate_cmd()?;
        }
        "cleanup" => {
            claw_cleanup_cmd()?;
        }
        "status" => {
            claw_status_cmd();
        }
        other => {
            println!("Claw action '{}' is not recognized.", other);
            println!("Available actions: migrate, cleanup, status");
        }
    }
    Ok(())
}

/// Check for legacy OpenClaw artefacts and report findings.
fn claw_status_cmd() {
    use crate::claw_migrate::find_openclaw_dir;

    println!("OpenClaw Legacy Status");
    println!("======================\n");

    let home = dirs::home_dir();

    match find_openclaw_dir(None) {
        Some(dir) => {
            println!("  OpenClaw directory: {} (found)", dir.display());

            let config_yaml = dir.join("config.yaml");
            let sessions_dir = dir.join("sessions");
            let env_file = dir.join(".env");
            let skills_dir = dir.join("skills");

            println!(
                "  config.yaml:       {}",
                if config_yaml.exists() {
                    "present"
                } else {
                    "not found"
                }
            );
            println!(
                "  .env:              {}",
                if env_file.exists() {
                    "present"
                } else {
                    "not found"
                }
            );
            println!(
                "  skills/:           {}",
                if skills_dir.is_dir() {
                    "present"
                } else {
                    "not found"
                }
            );

            if sessions_dir.is_dir() {
                let count = std::fs::read_dir(&sessions_dir)
                    .map(|rd| rd.filter_map(|e| e.ok()).count())
                    .unwrap_or(0);
                println!("  sessions/:         {} file(s)", count);
            } else {
                println!("  sessions/:         not found");
            }

            println!("\n  Run `hermes claw migrate` to import into Hermes.");
            println!("  Run `hermes claw cleanup` to remove legacy files.");
        }
        None => {
            println!("  No OpenClaw directory found.");
            if let Some(h) = &home {
                println!(
                    "  Checked: ~/.openclaw, ~/.clawdbot, ~/.moldbot under {}",
                    h.display()
                );
            }
            println!("\n  Nothing to migrate.");
        }
    }

    // Also check for PATH entries in shell configs
    if let Some(h) = &home {
        let shell_files = [".bashrc", ".zshrc", ".profile", ".bash_profile"];
        let mut found_refs = Vec::new();
        for f in &shell_files {
            let path = h.join(f);
            if let Ok(content) = std::fs::read_to_string(&path) {
                if content.contains("openclaw") || content.contains("clawdbot") {
                    found_refs.push(f.to_string());
                }
            }
        }
        if !found_refs.is_empty() {
            println!("\n  Shell config references found:");
            for f in &found_refs {
                println!("    ~/{}", f);
            }
        }
    }
}

/// Run the full migration using `claw_migrate::run_migration`.
fn claw_migrate_cmd() -> Result<(), hermes_core::AgentError> {
    use crate::claw_migrate::{MigrateOptions, find_openclaw_dir, run_migration};

    println!("OpenClaw → Hermes Migration");
    println!("===========================\n");

    let source_dir = find_openclaw_dir(None);
    if source_dir.is_none() {
        println!("No OpenClaw directory found. Nothing to migrate.");
        return Ok(());
    }
    let source_dir = source_dir.unwrap();
    println!("Source: {}", source_dir.display());
    println!("Target: {}\n", hermes_config::hermes_home().display());

    // Also copy sessions if they exist
    let src_sessions = source_dir.join("sessions");
    let dst_sessions = hermes_config::hermes_home().join("sessions");
    let mut session_count = 0usize;

    if src_sessions.is_dir() {
        std::fs::create_dir_all(&dst_sessions).map_err(|e| {
            hermes_core::AgentError::Io(format!("Failed to create sessions dir: {}", e))
        })?;
        if let Ok(entries) = std::fs::read_dir(&src_sessions) {
            for entry in entries.flatten() {
                let src = entry.path();
                let dst = dst_sessions.join(entry.file_name());
                if src.is_file() && !dst.exists() {
                    if std::fs::copy(&src, &dst).is_ok() {
                        session_count += 1;
                    }
                }
            }
        }
    }

    let options = MigrateOptions {
        source: Some(source_dir),
        dry_run: false,
        preset: "full".to_string(),
        overwrite: false,
    };

    let result = run_migration(&options);

    if !result.migrated.is_empty() {
        println!("Migrated:");
        for item in &result.migrated {
            let src = item
                .source
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            let dst = item
                .destination
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            let extra = item.reason.as_deref().unwrap_or("");
            println!("  ✓ {} → {} {}", src, dst, extra);
        }
    }

    if !result.skipped.is_empty() {
        println!("Skipped:");
        for item in &result.skipped {
            let reason = item.reason.as_deref().unwrap_or("");
            println!("  ⊘ {} — {}", item.kind, reason);
        }
    }

    if !result.errors.is_empty() {
        println!("Errors:");
        for item in &result.errors {
            let reason = item.reason.as_deref().unwrap_or("unknown error");
            println!("  ✗ {} — {}", item.kind, reason);
        }
    }

    if session_count > 0 {
        println!("\nSessions copied: {}", session_count);
    }

    let total = result.migrated.len() + session_count;
    println!(
        "\nMigration complete: {} item(s) migrated, {} skipped, {} error(s).",
        total,
        result.skipped.len(),
        result.errors.len()
    );

    Ok(())
}

/// Remove legacy OpenClaw files after confirmation.
fn claw_cleanup_cmd() -> Result<(), hermes_core::AgentError> {
    use crate::claw_migrate::find_openclaw_dir;
    use std::io::{self, BufRead, Write};

    let source_dir = find_openclaw_dir(None);
    if source_dir.is_none() {
        println!("No OpenClaw directory found. Nothing to clean up.");
        return Ok(());
    }
    let source_dir = source_dir.unwrap();

    println!("OpenClaw Cleanup");
    println!("================\n");
    println!("The following will be PERMANENTLY deleted:");
    println!("  Directory: {}", source_dir.display());

    // Count contents
    let file_count = count_files_recursive(&source_dir);
    println!("  Contains:  ~{} file(s)\n", file_count);

    // Check shell configs
    let home = dirs::home_dir();
    let shell_files = [".bashrc", ".zshrc", ".profile", ".bash_profile"];
    let mut affected_shells: Vec<String> = Vec::new();
    if let Some(h) = &home {
        for f in &shell_files {
            let path = h.join(f);
            if let Ok(content) = std::fs::read_to_string(&path) {
                if content.contains("openclaw") || content.contains("clawdbot") {
                    affected_shells.push(f.to_string());
                    println!("  Shell config: ~/{} (contains openclaw references)", f);
                }
            }
        }
    }

    print!("\nProceed with cleanup? [y/N]: ");
    io::stdout().flush().ok();
    let answer = io::stdin()
        .lock()
        .lines()
        .next()
        .and_then(|l| l.ok())
        .unwrap_or_default();

    if !matches!(answer.trim().to_lowercase().as_str(), "y" | "yes") {
        println!("Cleanup cancelled.");
        return Ok(());
    }

    // Remove the directory
    match std::fs::remove_dir_all(&source_dir) {
        Ok(_) => println!("  ✓ Removed {}", source_dir.display()),
        Err(e) => println!("  ✗ Failed to remove {}: {}", source_dir.display(), e),
    }

    // Clean shell configs
    if let Some(h) = &home {
        for f in &affected_shells {
            let path = h.join(f);
            if let Ok(content) = std::fs::read_to_string(&path) {
                let cleaned: Vec<&str> = content
                    .lines()
                    .filter(|line| {
                        let lower = line.to_lowercase();
                        !lower.contains("openclaw") && !lower.contains("clawdbot")
                    })
                    .collect();
                let new_content = cleaned.join("\n") + "\n";
                match std::fs::write(&path, new_content) {
                    Ok(_) => println!("  ✓ Cleaned ~/{}", f),
                    Err(e) => println!("  ✗ Failed to clean ~/{}: {}", f, e),
                }
            }
        }
    }

    println!("\nCleanup complete.");
    Ok(())
}

/// Recursively count files in a directory.
fn count_files_recursive(dir: &std::path::Path) -> usize {
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                count += count_files_recursive(&path);
            } else {
                count += 1;
            }
        }
    }
    count
}

pub(crate) const ACP_MULTIMODAL_PREFIX: &str = "__hermes_acp_parts_json__:";

fn looks_like_openai_parts(parts: &[serde_json::Value]) -> bool {
    !parts.is_empty()
        && parts.iter().all(|part| {
            part.as_object()
                .and_then(|obj| obj.get("type"))
                .and_then(|v| v.as_str())
                .is_some()
        })
}

fn flatten_openai_parts_to_text(parts: &[serde_json::Value]) -> String {
    let mut chunks: Vec<String> = Vec::new();
    for part in parts {
        let Some(obj) = part.as_object() else {
            continue;
        };
        let kind = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match kind {
            "text" => {
                if let Some(text) = obj.get("text").and_then(|v| v.as_str()) {
                    if !text.is_empty() {
                        chunks.push(text.to_string());
                    }
                }
            }
            "image_url" | "input_image" => {
                let url = obj
                    .get("image_url")
                    .and_then(|v| v.get("url"))
                    .and_then(|v| v.as_str())
                    .or_else(|| obj.get("image_url").and_then(|v| v.as_str()))
                    .or_else(|| obj.get("url").and_then(|v| v.as_str()))
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if !url.is_empty() {
                    chunks.push(format!("[Attached image]\nURL: {url}"));
                }
            }
            _ => {
                if let Some(text) = obj.get("text").and_then(|v| v.as_str()) {
                    if !text.is_empty() {
                        chunks.push(text.to_string());
                    }
                }
            }
        }
    }
    chunks.join("\n")
}

pub(crate) fn acp_history_to_messages(
    history: &[serde_json::Value],
    fallback_user_text: &str,
) -> Vec<hermes_core::Message> {
    let mut messages = Vec::new();

    for item in history {
        let role = item.get("role").and_then(|v| v.as_str()).unwrap_or("");
        let content_value = item.get("content").or_else(|| item.get("text"));
        let content = match content_value {
            Some(serde_json::Value::String(s)) => s.to_string(),
            Some(serde_json::Value::Array(parts)) if looks_like_openai_parts(parts) => {
                if role == "user" {
                    match serde_json::to_string(parts) {
                        Ok(serialized) => format!("{ACP_MULTIMODAL_PREFIX}{serialized}"),
                        Err(_) => flatten_openai_parts_to_text(parts),
                    }
                } else {
                    flatten_openai_parts_to_text(parts)
                }
            }
            Some(serde_json::Value::Object(obj)) => obj
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            _ => String::new(),
        };

        match role {
            "system" if !content.is_empty() => messages.push(hermes_core::Message::system(content)),
            "user" if !content.is_empty() => messages.push(hermes_core::Message::user(content)),
            "assistant" => {
                if let Some(tool_calls_val) = item.get("tool_calls") {
                    if let Ok(tool_calls) =
                        serde_json::from_value::<Vec<hermes_core::ToolCall>>(tool_calls_val.clone())
                    {
                        let assistant = hermes_core::Message::assistant_with_tool_calls(
                            if content.is_empty() {
                                None
                            } else {
                                Some(content)
                            },
                            tool_calls,
                        );
                        messages.push(assistant);
                        continue;
                    }
                }
                if !content.is_empty() {
                    messages.push(hermes_core::Message::assistant(content));
                }
            }
            "tool" if !content.is_empty() => {
                let tool_call_id = item
                    .get("tool_call_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("tool_call");
                messages.push(hermes_core::Message::tool_result(tool_call_id, content));
            }
            _ => {}
        }
    }

    let has_user_tail = messages
        .last()
        .map(|m| matches!(m.role, hermes_core::MessageRole::User))
        .unwrap_or(false);
    if !has_user_tail && !fallback_user_text.trim().is_empty() {
        messages.push(hermes_core::Message::user(fallback_user_text));
    }

    messages
}

struct CliAcpPromptExecutor {
    config: Arc<hermes_config::GatewayConfig>,
    tool_registry: Arc<hermes_tools::ToolRegistry>,
    tool_schemas: Vec<hermes_core::ToolSchema>,
}

#[async_trait::async_trait]
impl hermes_acp::AcpPromptExecutor for CliAcpPromptExecutor {
    async fn execute_prompt(
        &self,
        session: &hermes_acp::SessionState,
        user_text: &str,
        history: &[serde_json::Value],
    ) -> Result<hermes_acp::PromptExecutionOutput, String> {
        let model = session
            .model
            .clone()
            .or_else(|| self.config.model.clone())
            .unwrap_or_else(|| "gpt-4o".to_string());

        let provider = crate::app::build_provider(&self.config, &model);
        let mut agent_config = crate::app::build_agent_config(&self.config, &model);
        agent_config.session_id = Some(session.session_id.clone());

        let agent_tools = Arc::new(crate::app::bridge_tool_registry(&self.tool_registry));
        let agent = hermes_agent::attach_agent_runtime(
            hermes_agent::AgentLoop::new(agent_config, agent_tools, provider)
                .with_async_tool_dispatch(crate::app::async_tool_dispatch_for(
                    self.tool_registry.clone(),
                )),
        );
        let messages = acp_history_to_messages(history, user_text);
        let (conversation_history, user_message) =
            split_messages_for_run_conversation(&messages)
                .ok_or_else(|| "ACP prompt has no user message for run_conversation".to_string())?;
        let task_id = Some(session.session_id.clone());
        let conv = agent
            .run_conversation(RunConversationParams {
                user_message,
                conversation_history,
                task_id,
                stream_callback: None,
                persist_user_message: None,
                tools: Some(self.tool_schemas.clone()),
                persist_session: false,
            })
            .await
            .map_err(|e| e.to_string())?;
        let result = conv.into_loop_result();
        let response_text = result
            .messages
            .iter()
            .rev()
            .find(|m| matches!(m.role, hermes_core::MessageRole::Assistant))
            .and_then(|m| m.content.clone())
            .unwrap_or_default();

        let usage = result.usage.map(|u| hermes_acp::Usage {
            input_tokens: u.prompt_tokens,
            output_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
            thought_tokens: None,
            cached_read_tokens: None,
        });

        Ok(hermes_acp::PromptExecutionOutput {
            response_text,
            usage,
            total_turns: Some(result.total_turns),
            events: Vec::new(),
        })
    }
}

/// Handle `hermes acp [action]`.
pub async fn handle_cli_acp(action: Option<String>) -> Result<(), hermes_core::AgentError> {
    match action.as_deref().unwrap_or("status") {
        "start" => {
            let config = hermes_config::load_config(None)
                .map_err(|e| hermes_core::AgentError::Config(e.to_string()))?;

            let model = config.model.clone().unwrap_or_else(|| "gpt-4o".to_string());
            let max_turns = config.max_turns as usize;

            println!(
                "Starting ACP server (model={}, max_turns={})...",
                model, max_turns
            );

            let tool_registry = Arc::new(hermes_tools::ToolRegistry::new());
            let terminal_backend = crate::terminal_backend::build_terminal_backend(&config);
            let skill_store = Arc::new(hermes_skills::FileSkillStore::new(
                hermes_skills::FileSkillStore::default_dir(),
            ));
            let skill_provider: Arc<dyn hermes_core::SkillProvider> =
                Arc::new(hermes_skills::SkillManager::new(skill_store));
            hermes_tools::register_builtin_tools(&tool_registry, terminal_backend, skill_provider);
            crate::runtime_tool_wiring::wire_stdio_clarify_backend(&tool_registry);
            let cron_data_dir = hermes_config::cron_dir();
            std::fs::create_dir_all(&cron_data_dir)
                .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;
            let cron_scheduler = Arc::new(hermes_cron::cron_scheduler_for_data_dir(cron_data_dir));
            cron_scheduler
                .load_persisted_jobs()
                .await
                .map_err(|e| hermes_core::AgentError::Config(format!("cron load: {e}")))?;
            cron_scheduler.start().await;
            crate::runtime_tool_wiring::wire_cron_scheduler_backend(
                &tool_registry,
                cron_scheduler,
                MessagingSessionContext::new(),
            );
            let tool_schemas = crate::platform_toolsets::resolve_platform_tool_schemas(
                &config,
                "cli",
                &tool_registry,
            );

            let prompt_executor = Arc::new(CliAcpPromptExecutor {
                config: Arc::new(config.clone()),
                tool_registry,
                tool_schemas,
            });

            let session_manager = Arc::new(hermes_acp::SessionManager::new());
            let event_sink = Arc::new(hermes_acp::EventSink::default());
            let permission_store = Arc::new(hermes_acp::PermissionStore::new());
            let handler = Arc::new(
                hermes_acp::HermesAcpHandler::new(
                    session_manager.clone(),
                    event_sink.clone(),
                    permission_store.clone(),
                )
                .with_prompt_executor(prompt_executor),
            );
            let server = hermes_acp::AcpServer::with_components(
                handler,
                session_manager,
                event_sink,
                permission_store,
            );

            server
                .run()
                .await
                .map_err(|e| hermes_core::AgentError::Io(format!("ACP server error: {}", e)))?;
        }
        "status" => {
            println!("ACP server: not running");
            println!("ACP runs as a stdio JSON-RPC server in the foreground.");
            println!("Start with `hermes acp start`.");
        }
        "stop" => {
            println!("ACP stop is not a separate command in stdio mode.");
            println!("If running, stop it by closing the parent process or sending Ctrl+C.");
        }
        "restart" => {
            println!("ACP restart in stdio mode is equivalent to stop + start.");
            println!("Use:");
            println!("  1) Stop the current process (Ctrl+C)");
            println!("  2) Run `hermes acp start`");
        }
        other => {
            println!("Unknown ACP action '{}'.", other);
            println!("Available actions: start, status, stop, restart");
        }
    }
    Ok(())
}

/// Handle `hermes backup [output]`.
pub async fn handle_cli_backup(output: Option<String>) -> Result<(), hermes_core::AgentError> {
    let hermes_dir = hermes_config::hermes_home();
    if !hermes_dir.exists() {
        println!(
            "Hermes home directory not found at {}",
            hermes_dir.display()
        );
        return Ok(());
    }
    let out = output.unwrap_or_else(|| {
        format!(
            "hermes-backup-{}.tar.gz",
            chrono::Utc::now().format("%Y%m%d-%H%M%S")
        )
    });
    println!("Backing up {} -> {}", hermes_dir.display(), out);

    let tar_gz = std::fs::File::create(&out)
        .map_err(|e| hermes_core::AgentError::Io(format!("Cannot create {}: {}", out, e)))?;
    let enc = flate2::write::GzEncoder::new(tar_gz, flate2::Compression::default());
    let mut tar = tar::Builder::new(enc);
    tar.append_dir_all("hermes", &hermes_dir)
        .map_err(|e| hermes_core::AgentError::Io(format!("Tar error: {}", e)))?;
    tar.finish()
        .map_err(|e| hermes_core::AgentError::Io(format!("Tar finish error: {}", e)))?;

    let size = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
    println!("Backup complete: {} ({} KB)", out, size / 1024);
    Ok(())
}

/// Handle `hermes import <path>`.
pub async fn handle_cli_import(path: String) -> Result<(), hermes_core::AgentError> {
    let src = std::path::Path::new(&path);
    if !src.exists() {
        return Err(hermes_core::AgentError::Io(format!(
            "Backup archive not found: {}",
            path
        )));
    }
    println!("Importing configuration from: {}", path);

    let hermes_dir = hermes_config::hermes_home();
    std::fs::create_dir_all(&hermes_dir).map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;

    let file = std::fs::File::open(src)
        .map_err(|e| hermes_core::AgentError::Io(format!("Cannot open {}: {}", path, e)))?;
    let dec = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(dec);
    archive
        .unpack(&hermes_dir)
        .map_err(|e| hermes_core::AgentError::Io(format!("Extract error: {}", e)))?;

    println!(
        "Import complete. Files restored to {}",
        hermes_dir.display()
    );
    Ok(())
}

/// Handle `hermes version`.
pub fn handle_cli_version() -> Result<(), hermes_core::AgentError> {
    println!("hermes {}", env!("CARGO_PKG_VERSION"));
    Ok(())
}

/// Handle `hermes meeting <action> [options]`.
///
/// Actions:
/// - `notes --audio <path> [--title "..."]`  — process an audio file offline
/// - `record [--mode offline|realtime] [--title "..."]`  — start live recording
pub async fn handle_cli_meeting(
    action: Option<String>,
    audio: Option<String>,
    title: Option<String>,
    mode: Option<String>,
    diarize: bool,
) -> Result<(), hermes_core::AgentError> {
    use hermes_config::{DiarizationProvider, MeetingConfig, MeetingTranscriptionMode, SttConfig};
    use hermes_tools::tools::meeting_notes::run_offline_pipeline;

    let hermes_home = hermes_config::hermes_home();
    let action = action.as_deref().unwrap_or("notes");

    match action {
        "notes" => {
            let audio_path = audio.ok_or_else(|| {
                hermes_core::AgentError::Config("meeting notes requires --audio <path>".into())
            })?;
            let title = title.unwrap_or_else(|| "会议".to_string());

            let mut meeting_cfg = MeetingConfig::default();
            if let Some(m) = mode.as_deref() {
                meeting_cfg.transcription_mode = Some(match m {
                    "realtime" => MeetingTranscriptionMode::Realtime,
                    _ => MeetingTranscriptionMode::Offline,
                });
            }
            if diarize {
                meeting_cfg.diarization_provider = Some(DiarizationProvider::Pyannote);
            }

            let llm_base = std::env::var("MEETING_LLM_BASE_URL")
                .or_else(|_| std::env::var("OPENAI_BASE_URL"))
                .unwrap_or_else(|_| "https://api.openai.com/v1".into());
            let llm_key = std::env::var("MEETING_LLM_API_KEY")
                .or_else(|_| std::env::var("OPENAI_API_KEY"))
                .unwrap_or_default();
            let llm_model =
                std::env::var("MEETING_LLM_MODEL").unwrap_or_else(|_| "gpt-4o-mini".into());

            println!("▶ Generating meeting notes for: {}", audio_path);
            let notes = run_offline_pipeline(
                &audio_path,
                &title,
                SttConfig::default(),
                meeting_cfg,
                &llm_base,
                &llm_key,
                &llm_model,
                &hermes_home,
                |state| {
                    use hermes_tools::tools::meeting_notes::SummarizeState;
                    match &state {
                        SummarizeState::Transcribing => println!("  ⟳ 转录中…"),
                        SummarizeState::Diarizing => println!("  ⟳ 说话人识别中…"),
                        SummarizeState::SummarizingChunk(i, n) => println!("  ⟳ 总结片段 {i}/{n}…"),
                        SummarizeState::MergingSummaries => println!("  ⟳ 合并摘要…"),
                        SummarizeState::WritingMemory => println!("  ⟳ 写入记忆…"),
                        SummarizeState::Done => println!("  ✓ 完成"),
                        SummarizeState::Warning(w) => println!("  ⚠ {w}"),
                    }
                },
            )
            .await
            .map_err(|e| hermes_core::AgentError::Config(e.to_string()))?;

            println!("\n# {}\n", notes.title);
            println!("**日期**: {}", notes.date);
            println!("\n## 摘要\n{}", notes.summary);

            if !notes.key_decisions.is_empty() {
                println!("\n## 关键决策");
                for d in &notes.key_decisions {
                    println!("- {d}");
                }
            }
            if !notes.action_items.is_empty() {
                println!("\n## 行动项");
                for a in &notes.action_items {
                    println!("- {a}");
                }
            }
            if !notes.risks.is_empty() {
                println!("\n## 风险");
                for r in &notes.risks {
                    println!("- {r}");
                }
            }
            if let Some(tf) = &notes.transcript_file {
                println!("\n📁 转录文件: {tf}");
            }
            println!("\n✓ 已写入记忆系统 (holographic facts + MEMORY.md)");
        }
        "record" => {
            println!("⚠ `hermes meeting record` requires a microphone source (Phase 2 runtime).");
            println!("  Run `hermes meeting notes --audio <recorded.wav>` after recording.");
        }
        _ => {
            println!("Unknown meeting action '{action}'. Available: notes, record");
        }
    }

    Ok(())
}
