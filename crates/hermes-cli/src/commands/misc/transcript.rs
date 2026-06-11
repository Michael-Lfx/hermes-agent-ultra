//! Transcript slash commands: history, recap, context.

use std::fmt::Write as _;

use hermes_core::MessageRole;

use crate::commands::compress;
use crate::commands::session;
use crate::commands::{CommandResult, emit_command_output, truncate_chars};
use hermes_core::AgentError;

pub(crate) fn handle_history_command(
    host: &mut impl crate::app::SlashCommandHost,
) -> Result<CommandResult, AgentError> {
    let sp = session::session_db(host);
    let db_messages = if sp.ensure_db().is_ok() && !host.session_id().is_empty() {
        sp.load_session(host.session_id()).ok()
    } else {
        None
    };

    let transcript: Vec<_> = db_messages
        .as_ref()
        .filter(|m| !m.is_empty())
        .cloned()
        .unwrap_or_else(|| host.transcript_messages());

    if transcript.is_empty() {
        emit_command_output(host, "No conversation history yet.");
        return Ok(CommandResult::Handled);
    }

    let source_note = if db_messages.is_some() {
        " (from state.db)"
    } else {
        ""
    };
    let mut out = format!("Recent conversation history{source_note}:\n");
    for (idx, msg) in transcript.iter().enumerate().rev().take(12).rev() {
        let role = match msg.role {
            MessageRole::User => "USER",
            MessageRole::Assistant => "HERMES",
            MessageRole::System => "SYSTEM",
            MessageRole::Tool => "TOOL",
        };
        let preview =
            hermes_agent::session_persistence::decode_content_preview(msg.content.as_deref());
        let preview = preview.lines().next().unwrap_or("").trim();
        let clipped = if preview.chars().count() > 96 {
            let mut s: String = preview.chars().take(95).collect();
            s.push('…');
            s
        } else {
            preview.to_string()
        };
        let _ = writeln!(out, "{:>3}. {:<7} {}", idx + 1, role, clipped);
    }
    emit_command_output(host, out.trim_end());
    Ok(CommandResult::Handled)
}

// ---------------------------------------------------------------------------
// /recap
// ---------------------------------------------------------------------------

pub(crate) fn handle_recap_command(
    host: &mut impl crate::app::SlashCommandHost,
    args: &[&str],
) -> Result<CommandResult, AgentError> {
    let requested = args
        .first()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .unwrap_or(24)
        .clamp(1, 200);
    let transcript = host.transcript_messages();
    if transcript.is_empty() {
        emit_command_output(host, "No activity yet. Start with a prompt first.");
        return Ok(CommandResult::Handled);
    }

    let start = transcript.len().saturating_sub(requested);
    let window = &transcript[start..];
    let mut user_msgs = 0usize;
    let mut assistant_msgs = 0usize;
    let mut tool_msgs = 0usize;
    let mut system_msgs = 0usize;
    let mut tool_call_count = 0usize;
    let mut char_count = 0usize;

    for msg in window {
        match msg.role {
            MessageRole::User => user_msgs += 1,
            MessageRole::Assistant => assistant_msgs += 1,
            MessageRole::Tool => tool_msgs += 1,
            MessageRole::System => system_msgs += 1,
        }
        tool_call_count += msg.tool_calls.as_ref().map(|c| c.len()).unwrap_or(0);
        char_count += msg.content.as_deref().map(str::len).unwrap_or(0);
    }

    let latest_user = window
        .iter()
        .rev()
        .find(|m| matches!(m.role, MessageRole::User))
        .and_then(|m| m.content.as_deref())
        .map(|c| truncate_chars(c.trim(), 120))
        .unwrap_or_else(|| "(none)".to_string());
    let latest_assistant = window
        .iter()
        .rev()
        .find(|m| matches!(m.role, MessageRole::Assistant))
        .and_then(|m| m.content.as_deref())
        .map(|c| truncate_chars(c.trim(), 120))
        .unwrap_or_else(|| "(none)".to_string());

    let approx_tokens = (char_count / 4).max(1);
    emit_command_output(
        host,
        format!(
            "Session recap (last {} messages)\n\
             model: {}\n\
             roles: user={} assistant={} tool={} system={}\n\
             tool_calls: {}\n\
             approx_tokens: {}\n\
             latest_user: {}\n\
             latest_hermes: {}",
            window.len(),
            host.current_model(),
            user_msgs,
            assistant_msgs,
            tool_msgs,
            system_msgs,
            tool_call_count,
            approx_tokens,
            latest_user,
            latest_assistant
        ),
    );
    Ok(CommandResult::Handled)
}

// ---------------------------------------------------------------------------
// /context
// ---------------------------------------------------------------------------

pub(crate) async fn handle_context_command(
    host: &mut impl crate::app::SlashCommandHost,
    args: &[&str],
) -> Result<CommandResult, AgentError> {
    let action = args
        .first()
        .copied()
        .unwrap_or("status")
        .to_ascii_lowercase();
    match action.as_str() {
        "status" => {
            let transcript = host.transcript_messages();
            let total_chars: usize = transcript
                .iter()
                .map(|m| m.content.as_deref().map(str::len).unwrap_or(0))
                .sum();
            let approx_tokens = (total_chars / 4).max(1);
            let context_files = if host.config().agent.skip_context_files {
                "disabled"
            } else {
                "enabled"
            };
            emit_command_output(
                host,
                format!(
                    "Context status\n\
                     model: {}\n\
                     transcript_messages: {}\n\
                     approx_tokens: {}\n\
                     context_files: {}\n\
                     hint: run `/context breakdown` for per-message footprint or `/context compress` for immediate compaction",
                    host.current_model(),
                    transcript.len(),
                    approx_tokens,
                    context_files
                ),
            );
        }
        "breakdown" => {
            let transcript = host.transcript_messages();
            if transcript.is_empty() {
                emit_command_output(host, "No transcript yet.");
                return Ok(CommandResult::Handled);
            }
            let mut out = String::from("Context breakdown (recent)\n");
            for (idx, msg) in transcript.iter().enumerate().rev().take(20).rev() {
                let role = match msg.role {
                    MessageRole::User => "USER",
                    MessageRole::Assistant => "HERMES",
                    MessageRole::Tool => "TOOL",
                    MessageRole::System => "SYSTEM",
                };
                let chars = msg.content.as_deref().map(str::len).unwrap_or(0);
                let est_tokens = (chars / 4).max(1);
                let preview = msg
                    .content
                    .as_deref()
                    .unwrap_or("")
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim();
                let _ = writeln!(
                    out,
                    "{:>3}. {:<7} chars={:<5} tok≈{:<5} {}",
                    idx + 1,
                    role,
                    chars,
                    est_tokens,
                    truncate_chars(preview, 70)
                );
            }
            emit_command_output(host, out.trim_end());
        }
        "compress" | "compact" => {
            return compress::handle_compress_command(host, &[]).await;
        }
        _ => {
            emit_command_output(
                host,
                "Usage: /context [status|breakdown|compress]\nAlias: /summary -> /recap",
            );
        }
    }
    Ok(CommandResult::Handled)
}
