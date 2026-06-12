//! File mutation verifier — parity with Python `_file_mutation_verifier_*`.

use std::path::PathBuf;

const FILE_MUTATING_TOOLS: &[&str] = &["write_file", "patch"];

/// Tracks write_file/patch outcomes for end-of-turn advisory footer.
#[derive(Debug, Default)]
pub struct FileMutationTracker {
    pub enabled: bool,
    attempted: Vec<PathBuf>,
    failed: Vec<(PathBuf, String)>,
}

impl FileMutationTracker {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            ..Default::default()
        }
    }

    pub fn record_tool_result(
        &mut self,
        tool_name: &str,
        args: &serde_json::Value,
        output: &str,
        is_error: bool,
    ) {
        if !self.enabled || !FILE_MUTATING_TOOLS.contains(&tool_name) {
            return;
        }
        let paths = extract_mutation_targets(tool_name, args);
        for path in paths {
            self.attempted.push(path.clone());
            if is_error {
                let preview = error_preview(output);
                self.failed.push((path, preview));
            }
        }
    }

    pub fn has_failures(&self) -> bool {
        self.enabled && !self.failed.is_empty()
    }

    pub fn format_advisory_footer(&self) -> String {
        if !self.has_failures() {
            return String::new();
        }
        let mut lines = vec![
            "\n\n---".to_string(),
            "**File mutation advisory**".to_string(),
            "Some file changes in this turn did not succeed:".to_string(),
        ];
        for (path, err) in &self.failed {
            lines.push(format!("- `{}`: {}", path.display(), err));
        }
        lines.push(
            "Verify with `git status` or re-run the failed edits before assuming success.".into(),
        );
        lines.join("\n")
    }
}

fn extract_mutation_targets(tool_name: &str, args: &serde_json::Value) -> Vec<PathBuf> {
    match tool_name {
        "write_file" => args
            .get("path")
            .and_then(|v| v.as_str())
            .map(|p| vec![PathBuf::from(p)])
            .unwrap_or_default(),
        "patch" => {
            let mode = args
                .get("mode")
                .and_then(|v| v.as_str())
                .unwrap_or("replace");
            if mode == "replace" {
                return args
                    .get("path")
                    .and_then(|v| v.as_str())
                    .map(|p| vec![PathBuf::from(p)])
                    .unwrap_or_default();
            }
            if mode == "patch" {
                let body = args.get("patch").and_then(|v| v.as_str()).unwrap_or("");
                let re =
                    regex::Regex::new(r"(?m)^\*\*\*\s+(?:Update|Add|Delete)\s+File:\s*(.+)$").ok();
                if let Some(re) = re {
                    return re
                        .captures_iter(body)
                        .filter_map(|cap| cap.get(1))
                        .map(|m| PathBuf::from(m.as_str().trim()))
                        .collect();
                }
            }
            Vec::new()
        }
        _ => Vec::new(),
    }
}

fn error_preview(result: &str) -> String {
    let text: String = result.split_whitespace().collect::<Vec<_>>().join(" ");
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
        if let Some(err) = v.get("error").and_then(|e| e.as_str()) {
            let s: String = err.split_whitespace().collect::<Vec<_>>().join(" ");
            return truncate(&s, 180);
        }
    }
    truncate(&text, 180)
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let end = s
        .char_indices()
        .nth(max.saturating_sub(1))
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    format!("{}…", &s[..end])
}
