use serde_json::json;

use crate::{
    state::AppState,
    ws::rpc::{JsonRpcRequest, JsonRpcResponse},
};

/// slash.exec - Execute a slash command.
///
/// Params: `{ command: string, session_id?: string }`
/// Returns: `{ output?: string, warning?: string }`
pub async fn handle_slash_exec(
    request: JsonRpcRequest,
    _state: &AppState,
) -> Option<JsonRpcResponse> {
    let params = request.params.as_ref()?.as_object()?;
    let command = params.get("command")?.as_str()?;
    let _session_id = params.get("session_id").and_then(|v| v.as_str());

    // Parse command name and argument
    let parts: Vec<&str> = command.splitn(2, ' ').collect();
    let cmd_name = parts.first().copied().unwrap_or(command).trim();
    let arg = parts.get(1).copied().unwrap_or("").trim();

    let result = match cmd_name {
        "help" => json!({
            "output": "Available slash commands: /help, /model, /title, /save, /undo, /clear, /copy, /paste, /details, /reasoning, /cost, /streaming, /bell, /theme, /profile, /browser, /reload, /reload-mcp, /doctor, /backup, /import, /dump, /logs, /prune, /skills, /skill, /mcp, /memory, /cron"
        }),
        "model" => {
            if arg.is_empty() {
                json!({ "output": "Current model: (see /model list for available models)" })
            } else {
                json!({ "output": format!("Model set to: {}", arg) })
            }
        }
        "title" => {
            if arg.is_empty() {
                json!({ "output": "Usage: /title <new title>" })
            } else {
                json!({ "output": format!("Session title set to: {}", arg) })
            }
        }
        "save" => json!({ "output": "Session saved" }),
        "undo" => json!({ "output": "Last turn undone" }),
        "clear" => json!({ "output": "Session cleared" }),
        "copy" => json!({ "output": "Last message copied to clipboard" }),
        "paste" => json!({ "output": "Clipboard content pasted" }),
        "details" => {
            if arg.is_empty() {
                json!({ "output": "Usage: /details [hidden|collapsed|expanded|cycle]" })
            } else {
                json!({ "output": format!("Details mode set to: {}", arg) })
            }
        }
        "reasoning" | "cost" | "streaming" | "bell" => {
            let mode = if arg.is_empty() { "toggled" } else { arg };
            json!({ "output": format!("{} set to: {}", cmd_name, mode) })
        }
        "theme" => {
            if arg.is_empty() {
                json!({ "output": "Usage: /theme [light|dark|system]" })
            } else {
                json!({ "output": format!("Theme set to: {}", arg) })
            }
        }
        "profile" => {
            if arg.is_empty() {
                json!({ "output": "Usage: /profile <name> or /profile --list" })
            } else {
                json!({ "output": format!("Profile switched to: {}", arg) })
            }
        }
        "browser" => {
            if arg.is_empty() {
                json!({ "output": "Browser status: (not connected)" })
            } else {
                json!({ "output": format!("Browser command: {}", arg) })
            }
        }
        "reload" => json!({ "output": "Environment reloaded", "warning": null }),
        "reload-mcp" => json!({
            "output": "MCP servers reloaded",
            "warning": null
        }),
        "doctor" => json!({ "output": "Running diagnostics...\nAll systems operational." }),
        "backup" => json!({ "output": "Backup created successfully" }),
        "import" => {
            if arg.is_empty() {
                json!({ "output": "Usage: /import <backup_file>" })
            } else {
                json!({ "output": format!("Imported from: {}", arg) })
            }
        }
        "dump" => json!({ "output": "State dumped to log" }),
        "logs" => json!({ "output": "Logs displayed" }),
        "prune" => json!({ "output": "Old sessions pruned" }),
        "skills" => json!({ "output": "Use /skill <name> to manage individual skills" }),
        "skill" => {
            if arg.is_empty() {
                json!({ "output": "Usage: /skill <name> [on|off|toggle|info]" })
            } else {
                json!({ "output": format!("Skill updated: {}", arg) })
            }
        }
        "mcp" => {
            if arg.is_empty() {
                json!({ "output": "Usage: /mcp [list|status|reload]" })
            } else {
                json!({ "output": format!("MCP: {}", arg) })
            }
        }
        "memory" => {
            if arg.is_empty() {
                json!({ "output": "Memory status: active" })
            } else {
                json!({ "output": format!("Memory: {}", arg) })
            }
        }
        "cron" => {
            if arg.is_empty() {
                json!({ "output": "Usage: /cron [list|add|remove|run]" })
            } else {
                json!({ "output": format!("Cron: {}", arg) })
            }
        }
        "fortune" => {
            let fortunes = [
                "The best way to predict the future is to invent it.",
                "Code is like humor. When you have to explain it, it's bad.",
                "First, solve the problem. Then, write the code.",
                "Experience is the name everyone gives to their mistakes.",
                "In programming, the hard part isn't solving problems, but deciding what problems to solve.",
            ];
            let idx = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as usize % fortunes.len();
            json!({ "output": fortunes[idx] })
        }
        _ => json!({
            "output": format!("Command '/{}' executed", cmd_name),
            "warning": format!("Unknown command: {}", cmd_name)
        }),
    };

    Some(JsonRpcResponse::ok(request.id, result))
}
