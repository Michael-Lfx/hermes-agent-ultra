use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

use hermes_config::GatewayConfig;
use hermes_core::{tool_schema, JsonSchema, ToolError};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    core::session::SessionState,
    rpc::interaction::PendingInteractions,
};

/// Handoff state for session transfer to messaging platforms.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffState {
    pub state: String,
    pub platform: String,
    pub started_at: f64,
    pub completed_at: Option<f64>,
    pub error: Option<String>,
}

/// Global application state shared across all handlers.
#[derive(Clone)]
pub struct AppState {
    /// Runtime gateway configuration.
    pub config: Arc<RwLock<GatewayConfig>>,
    
    /// Path to HERMES_HOME directory.
    pub hermes_home: std::path::PathBuf,
    
    /// Session token for authentication.
    pub session_token: String,
    
    /// Active sessions (sid -> SessionState).
    pub sessions: Arc<RwLock<HashMap<String, SessionState>>>,
    
    /// Current active profile name.
    pub active_profile: Arc<RwLock<String>>,
    
    /// Pending interaction requests (approval, clarify, sudo, secret).
    pub pending_interactions: PendingInteractions,

    /// Broadcast channel for server-sent events.
    pub event_broadcast: broadcast::Sender<serde_json::Value>,

    /// Minimal agent tool registry (schemas only, used for LLM schema resolution and validation).
    pub tool_registry: Arc<hermes_agent::ToolRegistry>,

    /// Full-featured hermes_tools registry with all builtin tool implementations.
    /// Used by interaction_dispatch for async tool dispatch.
    pub tools_registry: Arc<hermes_tools::ToolRegistry>,

    /// Gateway subprocess handle.
    pub gateway_process: Arc<RwLock<Option<tokio::process::Child>>>,

    /// Path to cron data directory.
    pub cron_data_dir: std::path::PathBuf,

    /// Handoff states for session transfers to messaging platforms.
    pub handoff_states: Arc<RwLock<HashMap<String, HandoffState>>>,
}

/// Build a property entry for a tool schema.
fn str_prop(description: &str) -> Value {
    json!({"type": "string", "description": description})
}

/// Register the core built-in tools into the agent's ToolRegistry.
fn register_core_tools(registry: &mut hermes_agent::ToolRegistry) {
    use std::sync::Arc;

    // --- terminal ---
    registry.register(
        "terminal",
        tool_schema(
            "terminal",
            "Execute a terminal command and capture output.",
            JsonSchema::object(
                IndexMap::from([
                    ("command".into(), str_prop("The command to execute")),
                    ("cwd".into(), json!({"type": "string", "description": "Working directory"})),
                    ("timeout".into(), json!({"type": "number", "description": "Timeout in seconds"})),
                ]),
                vec!["command".into()],
            ),
        ),
        Arc::new(|params: Value| -> Result<String, ToolError> {
            let command = params.get("command")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidParams("Missing 'command'".into()))?;
            let cwd = params.get("cwd").and_then(|v| v.as_str());
            let timeout = params.get("timeout").and_then(|v| v.as_u64()).unwrap_or(120);

            let shell = if cfg!(target_os = "windows") { "powershell.exe" } else { "sh" };
            let arg = if cfg!(target_os = "windows") { "-Command" } else { "-c" };

            let mut cmd = std::process::Command::new(shell);
            cmd.arg(arg).arg(command);
            if let Some(dir) = cwd { cmd.current_dir(dir); }

            let output = cmd.output()
                .map_err(|e| ToolError::ExecutionFailed(format!("terminal: {}", e)))?;

            Ok(json!({
                "exit_code": output.status.code(),
                "stdout": String::from_utf8_lossy(&output.stdout),
                "stderr": String::from_utf8_lossy(&output.stderr),
            }).to_string())
        }),
    );

    // --- read_file ---
    registry.register(
        "read_file",
        tool_schema(
            "read_file",
            "Read a file from the local filesystem.",
            JsonSchema::object(
                IndexMap::from([
                    ("path".into(), str_prop("Absolute or relative file path")),
                    ("offset".into(), json!({"type": "number", "description": "Starting line (0-indexed)"})),
                    ("limit".into(), json!({"type": "number", "description": "Max lines to read"})),
                ]),
                vec!["path".into()],
            ),
        ),
        Arc::new(|params: Value| -> Result<String, ToolError> {
            let path = params.get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidParams("Missing 'path'".into()))?;
            let offset = params.get("offset").and_then(|v| v.as_u64()).unwrap_or(0);
            let limit = params.get("limit").and_then(|v| v.as_u64());

            let content = std::fs::read_to_string(path)
                .map_err(|e| ToolError::ExecutionFailed(format!("read_file: {}", e)))?;
            let lines: Vec<&str> = content.lines().collect();
            let start = offset as usize;
            if start >= lines.len() { return Ok(String::new()); }
            let end = limit.map(|l| start + l as usize).unwrap_or(lines.len());
            Ok(lines[start..end.min(lines.len())].join("\n"))
        }),
    );

    // --- write_file ---
    registry.register(
        "write_file",
        tool_schema(
            "write_file",
            "Write content to a file. Creates parent directories if needed.",
            JsonSchema::object(
                IndexMap::from([
                    ("path".into(), str_prop("File path to write")),
                    ("content".into(), str_prop("Content to write")),
                ]),
                vec!["path".into(), "content".into()],
            ),
        ),
        Arc::new(|params: Value| -> Result<String, ToolError> {
            let path = params.get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidParams("Missing 'path'".into()))?;
            let content = params.get("content")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidParams("Missing 'content'".into()))?;
            if let Some(parent) = std::path::Path::new(path).parent() {
                std::fs::create_dir_all(parent).ok();
            }
            std::fs::write(path, content)
                .map_err(|e| ToolError::ExecutionFailed(format!("write_file: {}", e)))?;
            Ok(json!({"ok": true, "path": path}).to_string())
        }),
    );

    // --- search_files (simple file listing) ---
    registry.register(
        "search_files",
        tool_schema(
            "search_files",
            "Search files matching a glob pattern.",
            JsonSchema::object(
                IndexMap::from([
                    ("pattern".into(), str_prop("Glob pattern (e.g. **/*.rs)")),
                    ("path".into(), json!({"type": "string", "description": "Search root"})),
                ]),
                vec!["pattern".into()],
            ),
        ),
        Arc::new(|params: Value| -> Result<String, ToolError> {
            let base_str = params.get("path").and_then(|v| v.as_str())
                .unwrap_or(".");
            let base = std::path::Path::new(base_str);

            let mut results = Vec::new();
            if let Ok(entries) = std::fs::read_dir(base) {
                for entry in entries.flatten() {
                    if results.len() >= 100 { break; }
                    results.push(entry.path().to_string_lossy().to_string());
                }
            }
            Ok(json!({"results": results, "total": results.len()}).to_string())
        }),
    );

    // --- grep (uses rg command) ---
    registry.register(
        "grep",
        tool_schema(
            "grep",
            "Search file contents with a regex pattern.",
            JsonSchema::object(
                IndexMap::from([
                    ("pattern".into(), str_prop("Regex pattern to search for")),
                    ("path".into(), json!({"type": "string", "description": "Directory to search"})),
                ]),
                vec!["pattern".into()],
            ),
        ),
        Arc::new(|params: Value| -> Result<String, ToolError> {
            let pattern = params.get("pattern")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidParams("Missing 'pattern'".into()))?;
            let path = params.get("path").and_then(|v| v.as_str()).unwrap_or(".");

            let mut cmd = std::process::Command::new("rg");
            cmd.arg("--line-number").arg("--color=never");
            cmd.arg(pattern).arg(path);
            let output = cmd.output()
                .map_err(|e| ToolError::ExecutionFailed(format!("grep: {}", e)))?;
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        }),
    );

    // --- bash (alias for terminal) ---
    registry.register(
        "bash",
        tool_schema(
            "bash",
            "Execute a bash/shell command.",
            JsonSchema::object(
                IndexMap::from([
                    ("command".into(), str_prop("The command to execute")),
                    ("cwd".into(), json!({"type": "string", "description": "Working directory"})),
                ]),
                vec!["command".into()],
            ),
        ),
        Arc::new(|params: Value| -> Result<String, ToolError> {
            let command = params.get("command")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidParams("Missing 'command'".into()))?;
            let shell = if cfg!(target_os = "windows") { "powershell.exe" } else { "sh" };
            let arg = if cfg!(target_os = "windows") { "-Command" } else { "-c" };
            let mut cmd = std::process::Command::new(shell);
            cmd.arg(arg).arg(command);
            if let Some(dir) = params.get("cwd").and_then(|v| v.as_str()) {
                cmd.current_dir(dir);
            }
            let output = cmd.output()
                .map_err(|e| ToolError::ExecutionFailed(format!("bash: {}", e)))?;
            Ok(json!({
                "exit_code": output.status.code(),
                "stdout": String::from_utf8_lossy(&output.stdout),
                "stderr": String::from_utf8_lossy(&output.stderr),
            }).to_string())
        }),
    );

    // --- edit (line-level file editing) ---
    registry.register(
        "edit",
        tool_schema(
            "edit",
            "Edit a file by replacing exact text.",
            JsonSchema::object(
                IndexMap::from([
                    ("path".into(), str_prop("File path")),
                    ("old_text".into(), str_prop("Exact text to find")),
                    ("new_text".into(), str_prop("Text to replace with")),
                ]),
                vec!["path".into(), "old_text".into(), "new_text".into()],
            ),
        ),
        Arc::new(|params: Value| -> Result<String, ToolError> {
            let path = params.get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidParams("Missing 'path'".into()))?;
            let old = params.get("old_text")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidParams("Missing 'old_text'".into()))?;
            let new = params.get("new_text")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidParams("Missing 'new_text'".into()))?;
            let content = std::fs::read_to_string(path)
                .map_err(|e| ToolError::ExecutionFailed(format!("edit read: {}", e)))?;
            if !content.contains(old) {
                return Err(ToolError::ExecutionFailed("old_text not found in file".into()));
            }
            let replaced = content.replacen(old, new, 1);
            std::fs::write(path, replaced)
                .map_err(|e| ToolError::ExecutionFailed(format!("edit write: {}", e)))?;
            Ok(json!({"ok": true, "path": path}).to_string())
        }),
    );

    // --- web_search (stub) ---
    registry.register(
        "web_search",
        tool_schema(
            "web_search",
            "Search the web for information.",
            JsonSchema::object(
                IndexMap::from([
                    ("query".into(), str_prop("Search query")),
                    ("num_results".into(), json!({"type": "number", "description": "Number of results (default 5)"})),
                ]),
                vec!["query".into()],
            ),
        ),
        Arc::new(|params: Value| -> Result<String, ToolError> {
            let _query = params.get("query")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidParams("Missing 'query'".into()))?;
            Ok(json!({"error": "Web search requires an API key. Configure EXA_API_KEY or BRAVE_API_KEY."}).to_string())
        }),
    );

    // --- web_extract (stub) ---
    registry.register(
        "web_extract",
        tool_schema(
            "web_extract",
            "Extract text content from a web URL.",
            JsonSchema::object(
                IndexMap::from([
                    ("url".into(), str_prop("URL to extract")),
                ]),
                vec!["url".into()],
            ),
        ),
        Arc::new(|params: Value| -> Result<String, ToolError> {
            let url = params.get("url")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidParams("Missing 'url'".into()))?;
            Ok(json!({"error": format!("Web extract requires an HTTP backend. URL: {}", url)}).to_string())
        }),
    );

    // --- memory (stub) ---
    registry.register(
        "memory",
        tool_schema(
            "memory",
            "Read or write to long-term memory.",
            JsonSchema::object(
                IndexMap::from([
                    ("action".into(), str_prop("Action: search, add, list, delete")),
                    ("query".into(), json!({"type": "string", "description": "Search or content"})),
                ]),
                vec!["action".into()],
            ),
        ),
        Arc::new(|params: Value| -> Result<String, ToolError> {
            let action = params.get("action")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidParams("Missing 'action'".into()))?;
            Ok(json!({"action": action, "message": "Memory provider not configured. Enable memory in config.yaml."}).to_string())
        }),
    );

    // --- todo (stub) ---
    registry.register(
        "todo",
        tool_schema(
            "todo",
            "Manage a task list.",
            JsonSchema::object(
                IndexMap::from([
                    ("todos".into(), json!({"type": "array", "description": "Task items"})),
                    ("merge".into(), json!({"type": "boolean", "description": "Merge with existing"})),
                ]),
                vec![],
            ),
        ),
        Arc::new(|params: Value| -> Result<String, ToolError> {
            let todos = params.get("todos").cloned().unwrap_or(Value::Null);
            Ok(json!({"todos": todos, "message": "Todo storage not yet implemented."}).to_string())
        }),
    );

    // --- delegate_task (stub) ---
    registry.register(
        "delegate_task",
        tool_schema(
            "delegate_task",
            "Delegate a task to a sub-agent.",
            JsonSchema::object(
                IndexMap::from([
                    ("goal".into(), str_prop("Task goal for sub-agent")),
                    ("context".into(), json!({"type": "string", "description": "Additional context"})),
                ]),
                vec!["goal".into()],
            ),
        ),
        Arc::new(|params: Value| -> Result<String, ToolError> {
            let goal = params.get("goal")
                .and_then(|v| v.as_str())
                .unwrap_or("no goal");
            Ok(json!({"error": format!("Delegation not yet supported. Goal: {}", goal)}).to_string())
        }),
    );

    // --- session_search (stub) ---
    registry.register(
        "session_search",
        tool_schema(
            "session_search",
            "Search past conversation sessions.",
            JsonSchema::object(
                IndexMap::from([
                    ("query".into(), str_prop("Search query")),
                ]),
                vec!["query".into()],
            ),
        ),
        Arc::new(|params: Value| -> Result<String, ToolError> {
            let query = params.get("query")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            Ok(json!({"results": [], "query": query, "message": "Session search not yet implemented."}).to_string())
        }),
    );

    // --- skill_manage (stub) ---
    registry.register(
        "skill_manage",
        tool_schema(
            "skill_manage",
            "Manage agent skills.",
            JsonSchema::object(
                IndexMap::from([
                    ("action".into(), str_prop("Action: list, enable, disable, install")),
                    ("skill".into(), json!({"type": "string", "description": "Skill name"})),
                ]),
                vec!["action".into()],
            ),
        ),
        Arc::new(|params: Value| -> Result<String, ToolError> {
            let action = params.get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("list");
            Ok(json!({"action": action, "message": "Skill management not yet implemented."}).to_string())
        }),
    );

    // --- code_execution (stub) ---
    registry.register(
        "code_execution",
        tool_schema(
            "code_execution",
            "Execute code in a sandboxed environment.",
            JsonSchema::object(
                IndexMap::from([
                    ("code".into(), str_prop("Code to execute")),
                    ("language".into(), json!({"type": "string", "description": "Language (python, js, etc.)"})),
                ]),
                vec!["code".into()],
            ),
        ),
        Arc::new(|params: Value| -> Result<String, ToolError> {
            let lang = params.get("language").and_then(|v| v.as_str()).unwrap_or("python");
            Ok(json!({"error": format!("Code execution requires a sandbox backend. Language: {}", lang)}).to_string())
        }),
    );

    // --- process (stub) ---
    registry.register(
        "process",
        tool_schema(
            "process",
            "Manage running processes.",
            JsonSchema::object(
                IndexMap::from([
                    ("action".into(), str_prop("Action: list, stop, status")),
                    ("pid".into(), json!({"type": "number", "description": "Process ID"})),
                ]),
                vec!["action".into()],
            ),
        ),
        Arc::new(|params: Value| -> Result<String, ToolError> {
            let action = params.get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("list");
            Ok(json!({"action": action, "processes": [], "message": "Process management not yet implemented."}).to_string())
        }),
    );
}

impl AppState {
    pub fn new(config: GatewayConfig, hermes_home: std::path::PathBuf) -> Self {
        // Priority: HERMES_DASHBOARD_SESSION_TOKEN env var > auto-generated
        let session_token = std::env::var("HERMES_DASHBOARD_SESSION_TOKEN")
            .unwrap_or_else(|_| generate_session_token());
        let (event_broadcast, _) = broadcast::channel(256);
        let cron_data_dir = hermes_home.join("cron");
        std::fs::create_dir_all(&cron_data_dir).ok();

        // 1. Create hermes_tools registry with all builtin tools
        let terminal: Arc<dyn hermes_core::TerminalBackend> = Arc::new(crate::backend::LocalTerminalBackend);
        let skills_dir = hermes_home.join("skills");
        let skill_provider: Arc<dyn hermes_core::SkillProvider> = Arc::new(crate::backend::DiskSkillProvider::new(skills_dir));
        let tools_registry = hermes_tools::ToolRegistry::new();
        hermes_agent::tools_wiring::register_builtin_tools(
            &tools_registry, terminal, skill_provider, None,
        );

        // 2. Populate hermes_agent::ToolRegistry with ALL schemas from hermes_tools
        //    (for LLM schema resolution, validation, and toolset display).
        //    The actual dispatch goes through hermes_tools::ToolRegistry::dispatch_async().
        let mut agent_registry = hermes_agent::ToolRegistry::new();
        let schemas = tools_registry.get_definitions();
        for schema in schemas {
            let name = schema.name.clone();
            // Register with a no-op handler; actual dispatch goes through hermes_tools async path
            agent_registry.register(
                name,
                schema,
                Arc::new(|_params: Value| -> Result<String, hermes_core::ToolError> {
                    Ok("handled by hermes_tools async dispatch".to_string())
                }),
            );
        }

        Self {
            config: Arc::new(RwLock::new(config)),
            hermes_home,
            session_token,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            active_profile: Arc::new(RwLock::new("default".to_string())),
            pending_interactions: crate::rpc::interaction::new_pending_interactions(),
            event_broadcast,
            tool_registry: Arc::new(agent_registry),
            tools_registry: Arc::new(tools_registry),
            gateway_process: Arc::new(RwLock::new(None)),
            cron_data_dir,
            handoff_states: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Returns the current session token (for loopback auth).
    pub fn session_token(&self) -> &str {
        &self.session_token
    }
    
    /// Returns the current active profile name.
    pub async fn active_profile_name(&self) -> String {
        self.active_profile.read().await.clone()
    }
    
    /// Returns the path to the config.yaml file for the active profile.
    pub fn config_path(&self) -> std::path::PathBuf {
        self.profile_home(None).join("config.yaml")
    }
    
    /// Returns the path to the .env file for the active profile.
    pub fn env_path(&self) -> std::path::PathBuf {
        self.profile_home(None).join(".env")
    }
    
    /// Returns the home directory for the given profile.
    pub fn profile_home(&self, profile: Option<&str>) -> std::path::PathBuf {
        match profile {
            Some(name) if name != "default" => {
                self.hermes_home.join("profiles").join(name)
            }
            _ => self.hermes_home.clone(),
        }
    }
    
    /// Returns a SessionPersistence instance for the given profile.
    pub fn profile_persistence(
        &self,
        profile: Option<&str>,
    ) -> Result<hermes_agent::session_persistence::SessionPersistence, hermes_core::AgentError> {
        let home = self.profile_home(profile);
        if !home.exists() {
            std::fs::create_dir_all(&home)
                .map_err(|e| hermes_core::AgentError::Io(format!("create profile dir: {}", e)))?;
        }
        Ok(hermes_agent::session_persistence::SessionPersistence::new(&home))
    }
    
    /// Returns a SessionPersistence instance for the active profile.
    pub fn session_persistence(&self) -> Result<hermes_agent::session_persistence::SessionPersistence, hermes_core::AgentError> {
        // Use default profile path for backward compatibility
        // Profile-aware code should use profile_persistence() with explicit profile name
        Ok(hermes_agent::session_persistence::SessionPersistence::new(&self.hermes_home))
    }
    
    /// Invalidate all cached AgentLoop instances so they are rebuilt
    /// with the latest config on the next prompt.submit.
    pub async fn invalidate_agent_caches(&self) {
        let sessions = self.sessions.read().await;
        for (_, session) in sessions.iter() {
            if let Ok(mut guard) = session.agent.lock() {
                *guard = None;
            }
        }
        tracing::info!("Invalidated agent caches for all sessions");
    }

    /// Subscribe to the event broadcast channel.
    pub fn subscribe_events(&self) -> broadcast::Receiver<serde_json::Value> {
        self.event_broadcast.subscribe()
    }

    /// Publish an event to all subscribers.
    pub fn publish_event(&self, event: serde_json::Value) {
        let _ = self.event_broadcast.send(event);
    }
}

/// Generate a random session token for authentication.
fn generate_session_token() -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                             abcdefghijklmnopqrstuvwxyz\
                             0123456789";
    let mut rng = rand::thread_rng();
    (0..32)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}
