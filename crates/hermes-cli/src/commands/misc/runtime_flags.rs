//! Runtime flag slash commands: usage, stop, status, verbose, yolo.

use std::sync::Arc;

use hermes_core::{AgentError, MessageRole};

use crate::commands::{CommandResult, emit_command_output};

pub(crate) fn handle_usage_command(
    host: &mut impl crate::app::SlashCommandHost,
) -> Result<CommandResult, AgentError> {
    let display = host.agent().session_usage_display();
    let mut body = hermes_agent::format_usage_command_text(&display);
    if display.calls == 0 {
        let estimated_tokens: usize = host
            .messages()
            .iter()
            .map(|m| m.content.as_ref().map_or(0, |c| c.len()) / 4)
            .sum();
        body.push_str(&format!(
            "\n\n(Transcript heuristic ~{} tokens — no provider usage yet.)",
            estimated_tokens
        ));
    }
    emit_command_output(host, body);
    Ok(CommandResult::Handled)
}

// ---------------------------------------------------------------------------
// /stop
// ---------------------------------------------------------------------------

pub(crate) fn handle_stop_command(
    host: &mut impl crate::app::SlashCommandHost,
) -> Result<CommandResult, AgentError> {
    host.interrupt_controller_mut().interrupt(None);
    emit_command_output(
        host,
        "[Stopping current agent execution]\nAgent execution halted. You can continue typing or use /retry.",
    );
    Ok(CommandResult::Handled)
}

// ---------------------------------------------------------------------------
// /status
// ---------------------------------------------------------------------------

pub(crate) fn handle_status_command(
    host: &mut impl crate::app::SlashCommandHost,
) -> Result<CommandResult, AgentError> {
    let msg_count = host.messages().len();
    let turns = host
        .messages()
        .iter()
        .filter(|m| m.role == MessageRole::User)
        .count();
    let usage = host.agent().session_usage_metrics();
    let token_line = if usage.api_calls > 0 {
        format!(
            "  Session tokens: {} total ({} in / {} out, {} API calls)",
            usage.total_tokens, usage.input_tokens, usage.output_tokens, usage.api_calls
        )
    } else {
        let estimated_tokens: usize = host
            .messages()
            .iter()
            .map(|m| m.content.as_ref().map_or(0, |c| c.len()) / 4)
            .sum();
        format!("  Est. tokens:   ~{} (no API calls yet)", estimated_tokens)
    };

    emit_command_output(
        host,
        format!(
            "Session Status\n  ID:            {}\n  Model:         {}\n  Personality:   {}\n  Turns:         {}\n  Messages:      {}\n{}\n  Max turns:     {}",
            host.session_id(),
            host.current_model(),
            host.current_personality().unwrap_or("(none)"),
            turns,
            msg_count,
            token_line,
            host.config().max_turns
        ),
    );
    Ok(CommandResult::Handled)
}

pub(crate) fn handle_verbose_command(
    host: &mut impl crate::app::SlashCommandHost,
) -> Result<CommandResult, AgentError> {
    let current = tracing::enabled!(tracing::Level::DEBUG);
    if current {
        emit_command_output(
            host,
            "Verbose mode: OFF (switching to info level)\n(Runtime log level changes require restart — use `hermes -v` for verbose)",
        );
    } else {
        emit_command_output(
            host,
            "Verbose mode: ON (switching to debug level)\n(Runtime log level changes require restart — use `hermes -v` for verbose)",
        );
    }
    Ok(CommandResult::Handled)
}

// ---------------------------------------------------------------------------
// /yolo
// ---------------------------------------------------------------------------

pub(crate) fn handle_yolo_command(
    host: &mut impl crate::app::SlashCommandHost,
) -> Result<CommandResult, AgentError> {
    let currently_required = host.config().approval.require_approval;
    let new_val = !currently_required;

    host.set_config(Arc::new({
        let mut cfg = host.config().as_ref().clone();
        cfg.approval.require_approval = new_val;
        cfg
    }));

    if !new_val {
        emit_command_output(
            host,
            "YOLO mode: ON — tool executions will not require approval.\nBe careful! The agent can now execute tools without confirmation.",
        );
    } else {
        emit_command_output(
            host,
            "YOLO mode: OFF — tool executions will require approval.",
        );
    }
    Ok(CommandResult::Handled)
}
