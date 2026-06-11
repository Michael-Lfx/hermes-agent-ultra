//! Slash-command dispatch router.

use hermes_core::AgentError;

use super::autocomplete::{canonical_command, expand_quick_alias_command};
use super::catalog::{
    handle_commands_catalog_command, handle_experiment_command, handle_feedback_command,
    handle_restart_command, handle_update_command, print_help,
};
use super::{CommandResult, emit_command_output};
/// Handle a slash command.
///
/// `cmd` is the full command token including the `/` prefix
/// (e.g. `/model`, `/new`). `args` are the remaining tokens.
pub async fn handle_slash_command(
    host: &mut (impl crate::app::SlashCommandHost + crate::app::AcpServerRuntime),
    cmd: &str,
    args: &[&str],
) -> Result<CommandResult, AgentError> {
    let (resolved_cmd, arg_storage) =
        match expand_quick_alias_command(&host.config().quick_commands, cmd, args) {
            Ok(expanded) => expanded,
            Err(message) => {
                emit_command_output(host, message);
                return Ok(CommandResult::Handled);
            }
        };
    let arg_refs: Vec<&str> = arg_storage.iter().map(|part| part.as_str()).collect();
    let args = arg_refs.as_slice();
    let cmd = resolved_cmd.as_str();
    match canonical_command(cmd) {
        "/new" => {
            host.new_session();
            let msg = if cmd.eq_ignore_ascii_case("/reset") {
                format!("[Session reset: {}]", host.session_id())
            } else {
                format!("[New session started: {}]", host.session_id())
            };
            emit_command_output(host, msg);
            Ok(CommandResult::Handled)
        }
        "/retry" => {
            host.retry_last().await?;
            Ok(CommandResult::Handled)
        }
        "/undo" => {
            host.undo_last();
            emit_command_output(host, "[Last exchange undone]");
            Ok(CommandResult::Handled)
        }
        "/history" => super::misc::handle_history_command(host),
        "/recap" => super::misc::handle_recap_command(host, args),
        "/context" => super::misc::handle_context_command(host, args).await,
        "/title" => {
            super::session::handle_session_compat_command(host, canonical_command(cmd), args)
        }
        "/branch" => super::session::handle_branch_command(host, args),
        "/timetravel" => super::session::handle_timetravel_command(host, args),
        "/snapshot" => super::session::handle_snapshot_command(host, args),
        "/rollback" => super::session::handle_rollback_command(host, args),
        "/queue" => super::background::handle_queue_command(host, args),
        "/handoff" => super::objective::handle_handoff_command(host, args),
        "/steer" => super::objective::handle_steer_command(host, args),
        "/btw" => super::objective::handle_btw_command(host, args),
        "/subgoal" => super::objective::handle_subgoal_command(host, args),
        "/sethome" => super::objective::handle_sethome_command(host, args),
        "/evolve" => super::ops::handle_ops_evolve_command(host, args).await,
        "/objective" => super::objective::handle_objective_command(host, args),
        "/claims" => super::claims::handle_claims_command(host, args),
        "/quorum" => super::quorum::handle_quorum_command(host, args).await,
        "/swarm" => super::swarm::handle_swarm_command(host, args).await,
        "/simulate" => super::ops::handle_simulate_command(host, args),
        "/specpatch" => super::studio_ops::handle_specpatch_command(host, args).await,
        "/heatmap" => super::studio_ops::handle_heatmap_command(host, args).await,
        "/studio" => super::studio_ops::handle_studio_command(host, args).await,
        "/ask" => super::diagnostics::handle_interactive_question_command(host, args),
        "/model" => super::model::handle_model_command(host, args).await,
        "/auth" => super::auth_cmd::handle_auth_command(host, args).await,
        "/provider" => super::misc::handle_provider_command(host).await,
        "/personality" => super::misc::handle_personality_command(host, args),
        "/profile" | "/whoami" => super::runtime_ui::handle_profile_command(host),
        "/fast" | "/skin" | "/voice" => {
            super::runtime_ui::handle_runtime_ui_mode_command(host, canonical_command(cmd), args)
        }
        "/pet" => super::runtime_ui::handle_pet_command(host, args),
        "/skills" => super::skills::handle_skills_command(host, args).await,
        "/curator" => super::misc::handle_curator_command(host, args).await,
        "/tools" => super::misc::handle_tools_command(host, args),
        "/toolcards" => super::misc::handle_toolcards_command(host, args),
        "/toolsets" => super::infra::handle_toolsets_command(host),
        "/plugins" => super::infra::handle_plugins_command(host),
        "/mcp" => super::infra::handle_mcp_command(host),
        "/reload" | "/reload-mcp" => {
            super::infra::handle_reload_command(host, canonical_command(cmd))
        }
        "/cron" => super::infra::handle_cron_command(host),
        "/agents" => super::infra::handle_agents_command(host, args),
        "/kanban" => super::kanban::handle_kanban_command(host, args),
        "/plan" => super::plan::handle_plan_command(host, args),
        "/lsp" => super::infra::handle_lsp_command(host, args),
        "/graph" => super::infra::handle_graph_command(host, args).await,
        "/qos" => super::ops::handle_qos_command(host, args).await,
        "/image" => super::diagnostics::handle_image_command(host, args),
        "/config" => super::misc::handle_config_command(host, args),
        "/autocompact" => super::compress::handle_autocompact_command(host, args).await,
        "/compress" => super::compress::handle_compress_command(host, args).await,
        "/clear-queue" => super::background::handle_clear_queue_command(host),
        "/usage" => super::misc::handle_usage_command(host),
        "/insights" => super::diagnostics::handle_insights_command(host),
        "/stop" => super::misc::handle_stop_command(host),
        "/status" => super::misc::handle_status_command(host),
        "/about" => super::misc::handle_about_command(host),
        "/ops" => super::ops::handle_ops_command(host, args).await,
        "/telemetry" => super::auth_cmd::handle_telemetry_command(host, args),
        "/runbook" => super::misc::handle_runbook_command(host, args),
        "/eval" => super::ops::handle_ops_eval_command(host, args).await,
        "/autopilot" => super::ops::handle_ops_autopilot_command(host, args).await,
        "/mission" => super::background::handle_mission_command(host, args).await,
        "/dashboard" => super::ops::handle_dashboard_command(host, args).await,
        "/platforms" => super::integrations::handle_platforms_command(host),
        "/integrations" => super::integrations::handle_integrations_command(host, args).await,
        "/commands" => handle_commands_catalog_command(host, args),
        "/boot" => super::policy::handle_boot_command(host, args).await,
        "/walkthrough" => super::policy::handle_walkthrough_command(host, args),
        "/triage" => super::misc::handle_trigger_triage_command(host, args),
        "/subconscious" => super::misc::handle_subconscious_command(host, args),
        "/log" => super::diagnostics::handle_log_command(host),
        "/debug-dump" => super::diagnostics::handle_debug_dump_command(host, args),
        "/dump-format" => super::diagnostics::handle_dump_format_command(host),
        "/experiment" => handle_experiment_command(host, args),
        "/feedback" => handle_feedback_command(host, args),
        "/restart" => handle_restart_command(host, args),
        "/update" => handle_update_command(host, args).await,
        "/redraw" => super::runtime_ui::handle_redraw_command(host),
        "/paste" => super::runtime_ui::handle_paste_command(host, args),
        "/gquota" => super::approval::handle_gquota_command(host, args).await,
        "/approve" => super::approval::handle_approve_command(host, args),
        "/deny" => super::approval::handle_deny_command(host, args),
        "/copy" => super::runtime_ui::handle_copy_command(host),
        "/save" => super::session::handle_save_command(host, args),
        "/load" => super::session::handle_load_command(host, args),
        "/resume" => super::session::handle_resume_command(host, args),
        "/sessions" => super::session::handle_sessions_command(host, args),
        "/background" => super::background::handle_background_command(host, args),
        "/mouse" => super::runtime_ui::handle_mouse_command(host, args),
        "/verbose" => super::misc::handle_verbose_command(host),
        "/statusbar" => super::runtime_ui::handle_statusbar_command(host),
        "/yolo" => super::misc::handle_yolo_command(host),
        "/browser" => super::browser::handle_browser_command(host, args).await,
        "/reasoning" => super::misc::handle_reasoning_command(host, args),
        "/raw" => super::misc::handle_raw_command(host, args),
        "/policy" => super::policy::handle_policy_command(host, args),
        "/help" => {
            print_help(host);
            Ok(CommandResult::Handled)
        }
        "/acp_server" => crate::acp_command::handle_acp_command(host, args).await,
        "/quit" | "/exit" => {
            emit_command_output(host, "Goodbye!");
            Ok(CommandResult::Quit)
        }
        _ => {
            emit_command_output(
                host,
                format!(
                    "Unknown command: {}. Type /help for available commands.",
                    cmd
                ),
            );
            Ok(CommandResult::Handled)
        }
    }
}
