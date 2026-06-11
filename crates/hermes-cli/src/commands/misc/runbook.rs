//! `/runbook` slash command handler.

use crate::commands::{CommandResult, emit_command_output};
use hermes_core::AgentError;

pub(crate) fn handle_runbook_command(
    host: &mut impl crate::app::SlashCommandHost,
    args: &[&str],
) -> Result<CommandResult, AgentError> {
    let action = args.first().copied().unwrap_or("list").to_ascii_lowercase();
    if action == "list" || action == "status" {
        emit_command_output(
            host,
            "Runbooks\n- auth-refresh: provider auth/session rejected\n- model-not-found: catalog drift / unknown model\n- contextlattice-connect: local memory integration bootstrap\n- tool-policy-deny: blocked by policy or sandbox profile\n- stream-finalization: stream done but transcript not finalized\n\nUse `/runbook show <name>`.",
        );
        return Ok(CommandResult::Handled);
    }
    if action == "show" {
        let Some(name) = args.get(1).map(|v| v.to_ascii_lowercase()) else {
            emit_command_output(host, "Usage: /runbook show <name>");
            return Ok(CommandResult::Handled);
        };
        let body = match name.as_str() {
            "auth-refresh" => {
                "Runbook: auth-refresh\n1) `/auth status`\n2) `/auth refresh`\n3) retry prompt\n4) if still failing, run `/model` and confirm provider/model pair is valid for your account."
            }
            "model-not-found" => {
                "Runbook: model-not-found\n1) `/model` and select a valid catalog model\n2) retry request\n3) if provider alias was stale, run `/auth verify` and re-check."
            }
            "contextlattice-connect" => {
                "Runbook: contextlattice-connect\n1) ensure contextlattice tools are registered via `/tools`\n2) ask agent to run `contextlattice_search` first (not shell command `contextlattice`)\n3) checkpoint verified integration via `contextlattice_write`."
            }
            "tool-policy-deny" => {
                "Runbook: tool-policy-deny\n1) inspect denial reason in tool card `[remediation]` section\n2) remove secret-like args from inline command payload\n3) retry with safer params or approved tool route (`/tools`)."
            }
            "stream-finalization" => {
                "Runbook: stream-finalization\n1) wait for final transcript writeback (status shows `Finalizing response…`)\n2) avoid submitting a new prompt until finalization completes\n3) if UI appears stale, use Ctrl+G to refresh and jump latest."
            }
            _ => {
                emit_command_output(
                    host,
                    format!(
                        "Unknown runbook `{}`. Use `/runbook list` for available entries.",
                        name
                    ),
                );
                return Ok(CommandResult::Handled);
            }
        };
        emit_command_output(host, body);
        return Ok(CommandResult::Handled);
    }
    emit_command_output(host, "Usage: /runbook [list|show <name>]");
    Ok(CommandResult::Handled)
}
