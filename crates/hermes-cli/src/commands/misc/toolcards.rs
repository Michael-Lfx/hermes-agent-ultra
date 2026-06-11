//! `/toolcards` slash command handler.

use crate::commands::{CommandResult, emit_command_output};
use hermes_core::AgentError;

pub(crate) fn handle_toolcards_command(
    host: &mut impl crate::app::SlashCommandHost,
    args: &[&str],
) -> Result<CommandResult, AgentError> {
    let action = args.first().copied().unwrap_or("help");
    let msg = match action {
        "export" => {
            "Tool-card export is handled by the interactive TUI modal loop. In TUI, run `/toolcards export` to write `~/.hermes-agent-ultra/logs/toolcards-export.txt`.".to_string()
        }
        _ => "Tool-card controls:\n  /toolcards export   Export current tool-card transcript".to_string(),
    };
    emit_command_output(host, msg);
    Ok(CommandResult::Handled)
}
