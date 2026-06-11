use std::path::{Path, PathBuf};

use serde_json::json;

use crate::{
    state::AppState,
    ws::rpc::{JsonRpcRequest, JsonRpcResponse},
};

/// complete.path - Provide file path completions.
///
/// Params: `{ word: string }`
/// Returns: `{ items: [{ display, text, meta? }], replace_from?: number }`
pub async fn handle_complete_path(
    request: JsonRpcRequest,
    _state: &AppState,
) -> Option<JsonRpcResponse> {
    let params = request.params.as_ref()?.as_object()?;
    let word = params.get("word")?.as_str()?;

    let mut items = Vec::new();

    // Expand ~ to home directory
    let expanded = if word.starts_with("~/") {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map(PathBuf::from)
            .unwrap_or_default();
        home.join(&word[2..]).to_string_lossy().to_string()
    } else {
        word.to_string()
    };

    let path = Path::new(&expanded);
    let (dir_path, prefix) = if expanded.ends_with('/') || expanded.ends_with('\\') {
        (path, "")
    } else {
        let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        match parent {
            Some(p) => (p, file_name),
            None => (Path::new("."), &expanded as &str),
        }
    };

    if let Ok(entries) = std::fs::read_dir(dir_path) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !prefix.is_empty() && !name.to_lowercase().starts_with(&prefix.to_lowercase()) {
                continue;
            }

            let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
            let suffix = if is_dir { "/" } else { "" };
            let meta = if is_dir { "dir" } else { "file" };

            items.push(json!({
                "display": format!("{}{}", name, suffix),
                "text": format!("{}{}", name, suffix),
                "meta": meta,
            }));
        }
    }

    // Sort: directories first, then alphabetically
    items.sort_by(|a, b| {
        let a_dir = a.get("meta").and_then(|m| m.as_str()) == Some("dir");
        let b_dir = b.get("meta").and_then(|m| m.as_str()) == Some("dir");
        match (b_dir, a_dir) {
            (true, false) => std::cmp::Ordering::Greater,
            (false, true) => std::cmp::Ordering::Less,
            _ => {
                let a_name = a.get("text").and_then(|t| t.as_str()).unwrap_or("");
                let b_name = b.get("text").and_then(|t| t.as_str()).unwrap_or("");
                a_name.cmp(b_name)
            }
        }
    });

    Some(JsonRpcResponse::ok(request.id, json!({
        "items": items,
        "replace_from": params.get("replace_from").and_then(|v| v.as_i64()).unwrap_or(0),
    })))
}

/// complete.slash - Provide slash command completions.
///
/// Params: `{ text: string }`
/// Returns: `{ items: [{ display, text, meta? }], replace_from?: number }`
pub async fn handle_complete_slash(
    request: JsonRpcRequest,
    _state: &AppState,
) -> Option<JsonRpcResponse> {
    let params = request.params.as_ref()?.as_object()?;
    let text = params.get("text")?.as_str()?;

    // Extract the command name after the slash
    let query = text.strip_prefix('/').unwrap_or(text).trim().to_lowercase();

    let commands = [
        ("help", "list commands + hotkeys"),
        ("model", "switch model"),
        ("title", "set session title"),
        ("save", "save session"),
        ("undo", "undo last turn"),
        ("clear", "clear session"),
        ("copy", "copy last message"),
        ("paste", "paste from clipboard"),
        ("details", "set detail visibility"),
        ("reasoning", "toggle reasoning display"),
        ("cost", "toggle cost display"),
        ("streaming", "toggle streaming"),
        ("bell", "toggle completion bell"),
        ("theme", "set color theme"),
        ("profile", "switch profile"),
        ("browser", "manage browser"),
        ("reload", "reload .env"),
        ("reload-mcp", "reload MCP servers"),
        ("doctor", "run diagnostics"),
        ("backup", "create backup"),
        ("import", "import backup"),
        ("dump", "dump state"),
        ("logs", "show logs"),
        ("prune", "prune old sessions"),
        ("skills", "list skills"),
        ("skill", "manage skill"),
        ("mcp", "manage MCP"),
        ("memory", "manage memory"),
        ("cron", "manage cron jobs"),
        ("fortune", "show a fortune"),
    ];

    let items: Vec<serde_json::Value> = commands
        .iter()
        .filter(|(name, _)| name.starts_with(&query))
        .map(|(name, desc)| {
            json!({
                "display": format!("/{} — {}", name, desc),
                "text": format!("/{}", name),
                "meta": "cmd",
            })
        })
        .collect();

    Some(JsonRpcResponse::ok(request.id, json!({
        "items": items,
        "replace_from": 1i64, // Replace from after the slash
    })))
}
