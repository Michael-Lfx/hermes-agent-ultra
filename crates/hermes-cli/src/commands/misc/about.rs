//! `/about` slash command handler and repo discovery helpers.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use hermes_core::AgentError;

use crate::commands::{CommandResult, emit_command_output, yes_no};

pub(crate) fn discover_repo_root_for_about() -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var("HERMES_REPO_ROOT") {
        let path = PathBuf::from(explicit.trim());
        if path.exists() {
            return Some(path);
        }
    }

    let mut probes: Vec<PathBuf> = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        probes.push(cwd);
    }
    probes.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")));

    for probe in probes {
        for candidate in probe.ancestors() {
            if candidate.join("docs/parity").exists() && candidate.join("README.md").exists() {
                return Some(candidate.to_path_buf());
            }
        }
    }
    None
}

pub(crate) fn read_json_file(path: &Path) -> Option<serde_json::Value> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<serde_json::Value>(&raw).ok()
}

fn json_value_at_path<'a>(
    value: &'a serde_json::Value,
    path: &[&str],
) -> Option<&'a serde_json::Value> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    Some(current)
}

fn json_str_at_path(value: &serde_json::Value, path: &[&str]) -> Option<String> {
    json_value_at_path(value, path)?
        .as_str()
        .map(|s| s.to_string())
}

fn json_u64_at_path(value: &serde_json::Value, path: &[&str]) -> Option<u64> {
    json_value_at_path(value, path)?.as_u64()
}

fn latest_upstream_sync_report(report_dir: &Path) -> Option<PathBuf> {
    let mut reports: Vec<PathBuf> = std::fs::read_dir(report_dir)
        .ok()?
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            let name = path.file_name()?.to_string_lossy();
            if name.starts_with("upstream-sync-") && name.ends_with(".txt") {
                Some(path)
            } else {
                None
            }
        })
        .collect();
    reports.sort();
    reports.into_iter().last()
}

fn parse_sync_report_metadata(path: &Path) -> (HashMap<String, String>, usize) {
    let mut meta = HashMap::new();
    let mut pending_commit_lines = 0usize;
    let raw = std::fs::read_to_string(path).unwrap_or_default();

    let mut in_pending_section = false;
    let mut in_pending_block = false;
    for line in raw.lines() {
        let trimmed = line.trim();
        if !in_pending_section {
            if trimmed.starts_with("## Pending Upstream Commits") {
                in_pending_section = true;
                continue;
            }
            if let Some((k, v)) = line.split_once(':') {
                let key = k.trim();
                if !key.is_empty()
                    && key
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
                {
                    meta.insert(key.to_string(), v.trim().to_string());
                }
            }
            continue;
        }

        if trimmed == "```" {
            if !in_pending_block {
                in_pending_block = true;
            } else {
                break;
            }
            continue;
        }
        if in_pending_block && !trimmed.is_empty() {
            pending_commit_lines = pending_commit_lines.saturating_add(1);
        }
    }

    (meta, pending_commit_lines)
}

pub(crate) fn handle_about_command(
    host: &mut impl crate::app::SlashCommandHost,
) -> Result<CommandResult, AgentError> {
    let mut out = String::new();
    let _ = writeln!(out, "Hermes Agent Ultra — About");
    let _ = writeln!(out, "  Version:         {}", env!("CARGO_PKG_VERSION"));
    let _ = writeln!(out, "  Session model:   {}", host.current_model());
    let _ = writeln!(
        out,
        "  Personality:     {}",
        host.current_personality().unwrap_or("(none)")
    );
    if let Ok(exe) = std::env::current_exe() {
        let _ = writeln!(out, "  Binary:          {}", exe.display());
    }
    if let Ok(cwd) = std::env::current_dir() {
        let _ = writeln!(out, "  Current dir:     {}", cwd.display());
    }

    let raw_mode = host.tool_registry().raw_mode_state();
    let policy_mode = std::env::var("HERMES_TOOL_POLICY_MODE")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "enforce".to_string());
    let policy_preset = std::env::var("HERMES_TOOL_POLICY_PRESET")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "off".to_string());

    let has_contextlattice_mcp = host.config().mcp_servers.iter().any(|entry| {
        let name_hit = entry.name.to_ascii_lowercase().contains("contextlattice");
        let url_hit = entry
            .url
            .as_ref()
            .map(|u| u.to_ascii_lowercase().contains("contextlattice"))
            .unwrap_or(false);
        name_hit || url_hit
    });

    let _ = writeln!(out);
    let _ = writeln!(out, "Enabled Ultra Features:");
    let _ = writeln!(
        out,
        "  - RTK raw-mode: enabled={} once={}",
        yes_no(raw_mode.enabled),
        yes_no(raw_mode.once_pending)
    );
    let _ = writeln!(
        out,
        "  - Tool policy: mode={} preset={}",
        policy_mode, policy_preset
    );
    let _ = writeln!(
        out,
        "  - Code indexing: {} (max_files={}, max_symbols={})",
        yes_no(host.config().agent.code_index_enabled),
        host.config().agent.code_index_max_files,
        host.config().agent.code_index_max_symbols
    );
    let _ = writeln!(
        out,
        "  - LSP context injection: {} (max_chars={})",
        yes_no(host.config().agent.lsp_context_enabled),
        host.config().agent.lsp_context_max_chars
    );
    let _ = writeln!(
        out,
        "  - Background review loop: {}",
        yes_no(host.config().agent.background_review_enabled)
    );
    let _ = writeln!(out, "  - Multi-registry skills: yes");
    let _ = writeln!(out, "  - Skill security scanning: yes");
    let _ = writeln!(
        out,
        "  - ContextLattice MCP configured: {}",
        yes_no(has_contextlattice_mcp)
    );

    if let Some(repo_root) = discover_repo_root_for_about() {
        let report_dir = repo_root.join(".sync-reports");
        let workstream_path = repo_root.join("docs/parity/workstream-status.json");
        let queue_path = repo_root.join("docs/parity/upstream-missing-queue.json");
        let proof_path = repo_root.join("docs/parity/global-parity-proof.json");

        let mut upstream_ref = String::from("unknown");
        let mut upstream_sha = String::from("unknown");
        let mut workstream_generated = String::from("unknown");
        if let Some(workstream) = read_json_file(&workstream_path) {
            if let Some(v) = json_str_at_path(&workstream, &["upstream_ref"]) {
                upstream_ref = v;
            }
            if let Some(v) = json_str_at_path(&workstream, &["upstream_sha"]) {
                upstream_sha = v;
            }
            if let Some(v) = json_str_at_path(&workstream, &["generated_at_utc"]) {
                workstream_generated = v;
            }
        }

        let mut queue_pending = 0u64;
        let mut queue_ported = 0u64;
        let mut queue_superseded = 0u64;
        if let Some(queue) = read_json_file(&queue_path) {
            queue_pending =
                json_u64_at_path(&queue, &["summary", "by_disposition", "pending"]).unwrap_or(0);
            queue_ported =
                json_u64_at_path(&queue, &["summary", "by_disposition", "ported"]).unwrap_or(0);
            queue_superseded =
                json_u64_at_path(&queue, &["summary", "by_disposition", "superseded"]).unwrap_or(0);
        }

        let mut release_gate_pass = String::from("unknown");
        let mut ci_gate_pass = String::from("unknown");
        if let Some(proof) = read_json_file(&proof_path) {
            if let Some(v) =
                json_value_at_path(&proof, &["release_gate", "pass"]).and_then(|v| v.as_bool())
            {
                release_gate_pass = yes_no(v).to_string();
            }
            if let Some(v) =
                json_value_at_path(&proof, &["ci_gate", "pass"]).and_then(|v| v.as_bool())
            {
                ci_gate_pass = yes_no(v).to_string();
            }
        }

        let mut latest_report_name = String::from("none");
        let mut latest_origin_sha = String::from("unknown");
        let mut latest_upstream_sha = String::from("unknown");
        let mut latest_timestamp = String::from("unknown");
        let mut latest_pending_count = 0usize;
        if let Some(report_path) = latest_upstream_sync_report(&report_dir) {
            latest_report_name = report_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| report_path.display().to_string());
            let (meta, pending_count) = parse_sync_report_metadata(&report_path);
            latest_pending_count = pending_count;
            if let Some(v) = meta.get("origin_sha") {
                latest_origin_sha = v.clone();
            }
            if let Some(v) = meta.get("upstream_sha") {
                latest_upstream_sha = v.clone();
            }
            if let Some(v) = meta.get("timestamp_utc") {
                latest_timestamp = v.clone();
            }
        }

        let _ = writeln!(out);
        let _ = writeln!(out, "Parity Snapshot:");
        let _ = writeln!(out, "  - Repo root: {}", repo_root.display());
        let _ = writeln!(out, "  - Upstream ref: {}", upstream_ref);
        let _ = writeln!(out, "  - Upstream sha: {}", upstream_sha);
        let _ = writeln!(
            out,
            "  - Workstream report generated_at: {}",
            workstream_generated
        );
        let _ = writeln!(
            out,
            "  - Queue (pending/ported/superseded): {}/{}/{}",
            queue_pending, queue_ported, queue_superseded
        );
        let _ = writeln!(
            out,
            "  - Gate status (release/ci): {}/{}",
            release_gate_pass, ci_gate_pass
        );
        let _ = writeln!(out, "  - Latest sync report: {}", latest_report_name);
        let _ = writeln!(out, "  - Latest sync timestamp_utc: {}", latest_timestamp);
        let _ = writeln!(out, "  - Latest report origin_sha: {}", latest_origin_sha);
        let _ = writeln!(
            out,
            "  - Latest report upstream_sha: {}",
            latest_upstream_sha
        );
        let _ = writeln!(
            out,
            "  - Pending upstream commits in latest report: {}",
            latest_pending_count
        );
    } else {
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "Parity Snapshot: unavailable (run from a source checkout to load docs/parity + .sync-reports)."
        );
    }

    emit_command_output(host, out.trim_end());
    Ok(CommandResult::Handled)
}
