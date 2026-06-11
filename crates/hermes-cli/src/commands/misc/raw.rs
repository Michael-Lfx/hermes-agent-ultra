//! `/raw` slash command handler and replay trace helpers.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::commands::{
    CommandResult, emit_command_output, replay_log_path_for_session, replay_trace_integrity,
    truncate_chars,
};
use crate::env_vars;
use hermes_core::AgentError;

pub(crate) fn handle_raw_command(
    host: &mut impl crate::app::SlashCommandHost,
    args: &[&str],
) -> Result<CommandResult, AgentError> {
    if args
        .first()
        .is_some_and(|sub| sub.eq_ignore_ascii_case("trace"))
    {
        let replay_path = replay_log_path_for_session(host.session_id());
        let sub = args.get(1).map(|s| s.trim().to_ascii_lowercase());
        match sub.as_deref() {
            None | Some("status") => {
                emit_command_output(
                    host,
                    format!(
                        "Replay trace: {}{}\nSession: {}\nPath: {}\nUsage: /raw trace [on|off|toggle|status|tail [N]|focus <trace-id> [N]|graph [N]|verify|export [N] [PATH]|path]",
                        if replay_enabled_runtime() {
                            "ON"
                        } else {
                            "OFF"
                        },
                        if replay_path.exists() {
                            ""
                        } else {
                            " (no log yet)"
                        },
                        host.session_id(),
                        replay_path.display()
                    ),
                );
            }
            Some("path") => {
                emit_command_output(host, format!("Replay path: {}", replay_path.display()));
            }
            Some("tail") => {
                let limit = args
                    .get(2)
                    .and_then(|raw| raw.trim().parse::<usize>().ok())
                    .unwrap_or(20)
                    .clamp(1, 200);
                if !replay_path.exists() {
                    emit_command_output(
                        host,
                        format!(
                            "Replay log not found for current session yet: {}",
                            replay_path.display()
                        ),
                    );
                    return Ok(CommandResult::Handled);
                }
                let rendered = render_replay_trace_tail(&replay_path, limit)?;
                emit_command_output(host, rendered);
            }
            Some("focus") => {
                let Some(trace_id) = args.get(2).copied() else {
                    emit_command_output(host, "Usage: /raw trace focus <trace-id> [N]");
                    return Ok(CommandResult::Handled);
                };
                let limit = args
                    .get(3)
                    .and_then(|raw| raw.trim().parse::<usize>().ok())
                    .unwrap_or(150)
                    .clamp(1, 1000);
                if !replay_path.exists() {
                    emit_command_output(
                        host,
                        format!(
                            "Replay log not found for current session yet: {}",
                            replay_path.display()
                        ),
                    );
                    return Ok(CommandResult::Handled);
                }
                let rendered = render_replay_trace_focus(&replay_path, trace_id, limit)?;
                emit_command_output(host, rendered);
            }
            Some("graph") => {
                let limit = args
                    .get(2)
                    .and_then(|raw| raw.trim().parse::<usize>().ok())
                    .unwrap_or(80)
                    .clamp(1, 500);
                if !replay_path.exists() {
                    emit_command_output(
                        host,
                        format!(
                            "Replay log not found for current session yet: {}",
                            replay_path.display()
                        ),
                    );
                    return Ok(CommandResult::Handled);
                }
                let rendered = render_replay_trace_graph(&replay_path, limit)?;
                emit_command_output(host, rendered);
            }
            Some("verify") => {
                if !replay_path.exists() {
                    emit_command_output(
                        host,
                        format!(
                            "Replay log not found for current session yet: {}",
                            replay_path.display()
                        ),
                    );
                    return Ok(CommandResult::Handled);
                }
                let (entries, parse_errors, chain_breaks) = replay_trace_integrity(&replay_path)?;
                let ok = parse_errors == 0 && chain_breaks == 0;
                emit_command_output(
                    host,
                    format!(
                        "Replay integrity: {}\nentries: {}\nparse_errors: {}\nchain_breaks: {}\npath: {}",
                        if ok { "PASS" } else { "FAIL" },
                        entries,
                        parse_errors,
                        chain_breaks,
                        replay_path.display()
                    ),
                );
            }
            Some("export") => {
                let limit = args
                    .get(2)
                    .and_then(|raw| raw.trim().parse::<usize>().ok())
                    .unwrap_or(100)
                    .clamp(1, 1000);
                let output_path = args.get(3).map(PathBuf::from).unwrap_or_else(|| {
                    hermes_config::hermes_home()
                        .join("logs")
                        .join("replay")
                        .join("exports")
                        .join(format!("{}-tail.json", host.session_id()))
                });
                if !replay_path.exists() {
                    emit_command_output(
                        host,
                        format!(
                            "Replay log not found for current session yet: {}",
                            replay_path.display()
                        ),
                    );
                    return Ok(CommandResult::Handled);
                }
                let written = export_replay_trace_json(&replay_path, limit, &output_path)?;
                emit_command_output(
                    host,
                    format!(
                        "Replay export written.\nrows: {}\nsource: {}\noutput: {}",
                        written,
                        replay_path.display(),
                        output_path.display()
                    ),
                );
            }
            Some("on") | Some("off") | Some("toggle") => {
                let next = match sub.as_deref().unwrap_or("status") {
                    "on" => true,
                    "off" => false,
                    "toggle" => !replay_enabled_runtime(),
                    _ => replay_enabled_runtime(),
                };
                env_vars::set_var("HERMES_REPLAY_ENABLED", if next { "1" } else { "0" });
                emit_command_output(
                    host,
                    format!(
                        "Replay trace mode: {}.\nThis applies to new turns in the current process.",
                        if next { "ON" } else { "OFF" }
                    ),
                );
            }
            Some("help") | Some("--help") | Some("-h") => emit_command_output(
                host,
                "Replay trace controls:\n  /raw trace status              Show enabled state + current log path\n  /raw trace on|off              Enable or disable deterministic replay trace logs\n  /raw trace toggle              Toggle replay trace logs\n  /raw trace tail [N]            Show latest trace events with lineage hashes\n  /raw trace focus <id> [N]      Filter replay rows by trace_id\n  /raw trace graph [N]           Show lineage edges for recent rows\n  /raw trace verify              Validate replay hash-chain integrity\n  /raw trace export [N] [PATH]   Export tail events to JSON\n  /raw trace path                Show trace log file for current session",
            ),
            _ => emit_command_output(
                host,
                "Usage: /raw trace [on|off|toggle|status|tail [N]|focus <trace-id> [N]|graph [N]|verify|export [N] [PATH]|path]",
            ),
        }
        return Ok(CommandResult::Handled);
    }

    let state = host.tool_registry().raw_mode_state();
    let log_dir = host.tool_registry().rtk_log_dir();
    if args.is_empty() || args[0].eq_ignore_ascii_case("status") {
        emit_command_output(
            host,
            format!(
                "RTK raw mode: {}{}\nDual logs: {}\nReplay trace: {}\nUsage: /raw [on|off|toggle|once|status|trace]",
                if state.enabled { "ON" } else { "OFF" },
                if state.once_pending {
                    " (one-shot pending)"
                } else {
                    ""
                },
                log_dir.display(),
                if replay_enabled_runtime() {
                    "ON"
                } else {
                    "OFF"
                }
            ),
        );
        return Ok(CommandResult::Handled);
    }

    match args[0].trim().to_ascii_lowercase().as_str() {
        "help" => emit_command_output(
            host,
            "RTK raw controls:\n  /raw status        Show current mode + log path\n  /raw on            Disable output filtering for all tool calls\n  /raw off           Re-enable RTK output filtering\n  /raw toggle        Toggle global raw mode\n  /raw once          Raw pass-through for next tool call only\n  /raw trace ...     Deterministic replay trace controls",
        ),
        "once" => {
            host.tool_registry().set_raw_mode_once();
            emit_command_output(
                host,
                "RTK raw mode armed for next tool call only. It auto-resets after one dispatch.",
            );
        }
        "on" | "off" | "toggle" | "true" | "false" | "yes" | "no" | "1" | "0" => {
            let next = match args[0].trim().to_ascii_lowercase().as_str() {
                "on" | "true" | "yes" | "1" => true,
                "off" | "false" | "no" | "0" => false,
                "toggle" => !state.enabled,
                _ => state.enabled,
            };
            host.tool_registry().set_raw_mode(next);
            env_vars::set_var("HERMES_RTK_RAW", if next { "1" } else { "0" });
            emit_command_output(
                host,
                format!(
                    "RTK raw mode: {} (dual logs: {})",
                    if next { "ON" } else { "OFF" },
                    log_dir.display()
                ),
            );
        }
        _ => emit_command_output(host, "Usage: /raw [on|off|toggle|once|status|trace]"),
    }
    Ok(CommandResult::Handled)
}

pub(crate) fn replay_enabled_runtime() -> bool {
    std::env::var("HERMES_REPLAY_ENABLED")
        .ok()
        .is_some_and(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
}

fn render_replay_trace_tail(path: &Path, limit: usize) -> Result<String, AgentError> {
    let raw = std::fs::read_to_string(path).map_err(|e| {
        AgentError::Io(format!(
            "Failed to read replay log {}: {}",
            path.display(),
            e
        ))
    })?;
    let lines: Vec<&str> = raw
        .lines()
        .rev()
        .filter(|l| !l.trim().is_empty())
        .take(limit)
        .collect();
    let mut out = format!("Replay trace tail ({} lines)\n", lines.len());
    for line in lines.iter().rev() {
        let _ = writeln!(out, "{}", line);
    }
    Ok(out)
}

fn replay_entries(path: &Path, limit: usize) -> Result<Vec<serde_json::Value>, AgentError> {
    let raw = std::fs::read_to_string(path).map_err(|e| {
        AgentError::Io(format!(
            "Failed to read replay log {}: {}",
            path.display(),
            e
        ))
    })?;
    Ok(raw
        .lines()
        .rev()
        .filter(|l| !l.trim().is_empty())
        .take(limit)
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .collect())
}

fn render_replay_trace_focus(
    path: &Path,
    trace_id: &str,
    limit: usize,
) -> Result<String, AgentError> {
    let trace_filter = trace_id.trim();
    if trace_filter.is_empty() {
        return Ok("Usage: /raw trace focus <trace-id> [N]".to_string());
    }
    let rows = replay_entries(path, limit)?;
    let filtered: Vec<serde_json::Value> = rows
        .into_iter()
        .filter(|row| {
            row.get("trace_id")
                .and_then(|v| v.as_str())
                .is_some_and(|id| id.contains(trace_filter))
        })
        .collect();
    let mut out = format!(
        "Replay trace focus ({} rows match `{}`)\n",
        filtered.len(),
        trace_filter
    );
    for row in &filtered {
        let seq = row.get("seq").and_then(|v| v.as_u64()).unwrap_or(0);
        let event = row.get("event").and_then(|v| v.as_str()).unwrap_or("?");
        let preview = row
            .get("payload")
            .map(|v| truncate_chars(&v.to_string(), 120))
            .unwrap_or_default();
        let _ = writeln!(out, "#{} [{}] {} {}", seq, event, trace_filter, preview);
    }
    Ok(out)
}

fn render_replay_trace_graph(path: &Path, limit: usize) -> Result<String, AgentError> {
    let rows = replay_entries(path, limit)?;
    if rows.is_empty() {
        return Ok("Replay graph: no entries in current window.".to_string());
    }
    let mut out = String::new();
    let _ = writeln!(out, "Replay lineage graph");
    let _ = writeln!(out, "--------------------");
    let _ = writeln!(out, "window={} path={}", rows.len(), path.display());
    for row in rows {
        let seq = row.get("seq").and_then(|v| v.as_u64()).unwrap_or(0);
        let event = row.get("event").and_then(|v| v.as_str()).unwrap_or("?");
        let tid = row.get("trace_id").and_then(|v| v.as_str()).unwrap_or("?");
        let prev = row
            .get("prev_hash")
            .and_then(|v| v.as_str())
            .unwrap_or("seed");
        let curr = row
            .get("event_hash")
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        let _ = writeln!(
            out,
            "#{} [{}] tid={} {} → {}",
            seq,
            event,
            tid,
            truncate_chars(prev, 16),
            truncate_chars(curr, 16)
        );
    }
    Ok(out)
}

fn export_replay_trace_json(
    replay_path: &Path,
    limit: usize,
    output_path: &Path,
) -> Result<usize, AgentError> {
    let raw = std::fs::read_to_string(replay_path).map_err(|e| {
        AgentError::Io(format!(
            "Failed to read replay log {}: {}",
            replay_path.display(),
            e
        ))
    })?;
    let rows: Vec<serde_json::Value> = raw
        .lines()
        .rev()
        .filter(|l| !l.trim().is_empty())
        .take(limit)
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .collect();
    let count = rows.len();
    let export = serde_json::json!({ "rows": rows });
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AgentError::Io(format!("Failed to create {}: {}", parent.display(), e)))?;
    }
    std::fs::write(
        output_path,
        serde_json::to_string_pretty(&export)
            .map_err(|e| AgentError::Io(format!("Failed to serialize export: {}", e)))?,
    )
    .map_err(|e| AgentError::Io(format!("Failed to write {}: {}", output_path.display(), e)))?;
    Ok(count)
}
