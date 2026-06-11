//! `/plan-mode` slash command handler.

use hermes_core::AgentError;

use crate::commands::CommandResult;
use crate::plan_mode::{
    PlanModeSlashAction, handle_plan_mode_slash_action, parse_plan_mode_slash_args,
};

pub(crate) async fn handle_plan_mode_command(
    host: &mut impl crate::app::SlashCommandHost,
    args: &[&str],
) -> Result<CommandResult, AgentError> {
    let action = if args.is_empty() {
        PlanModeSlashAction::Help
    } else {
        parse_plan_mode_slash_args(&args.join(" "))
    };
    // SlashCommandHost is implemented for App; delegate to shared plan-mode logic.
    handle_plan_mode_slash_action(host, action).await?;
    Ok(CommandResult::Handled)
}
