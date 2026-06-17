mod cli;
mod slash;
mod tier;

pub use cli::handle_cli_skills;
pub(crate) use slash::handle_skills_command;
pub(crate) use tier::{SkillsExecutionTier, skills_execution_tier, skills_tier_bypass_enabled};

pub(crate) fn reconcile_bundled_after_update() {
    cli::sync_cmd::run_reconcile_quiet();
}
