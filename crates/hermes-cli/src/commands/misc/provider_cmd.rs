//! `/provider` slash command handler.

use std::fmt::Write as _;

use crate::commands::{CommandResult, emit_command_output};
use crate::model_switch::{curated_provider_slugs, provider_catalog_entries};
use hermes_core::AgentError;

pub(crate) async fn handle_provider_command(
    host: &mut impl crate::app::SlashCommandHost,
) -> Result<CommandResult, AgentError> {
    let providers = curated_provider_slugs();
    if providers.is_empty() {
        emit_command_output(host, "No providers registered.");
        return Ok(CommandResult::Handled);
    }
    let entries = provider_catalog_entries(&providers, 4).await;
    if entries.is_empty() {
        emit_command_output(
            host,
            format!(
                "Configured providers: {}\nCurrent model: {}",
                providers.join(", "),
                host.current_model()
            ),
        );
        return Ok(CommandResult::Handled);
    }
    let mut out = format!("Current model: {}\n\nProviders:\n", host.current_model());
    for entry in entries {
        let preview = entry.models.join(", ");
        let suffix = if entry.total_models > entry.models.len() {
            format!(" (+{} more)", entry.total_models - entry.models.len())
        } else {
            String::new()
        };
        let _ = writeln!(out, "  - {:<14} {}{}", entry.provider, preview, suffix);
    }
    emit_command_output(host, out.trim_end());
    Ok(CommandResult::Handled)
}
