//! `/tools` slash command handler.

use std::fmt::Write as _;

use crate::commands::{CommandResult, emit_command_output};
use hermes_core::AgentError;

pub(crate) fn handle_tools_command(
    host: &mut impl crate::app::SlashCommandHost,
    args: &[&str],
) -> Result<CommandResult, AgentError> {
    if args
        .first()
        .is_some_and(|sub| sub.eq_ignore_ascii_case("trust"))
    {
        let counters = host.tool_registry().policy_counters();
        let tools = host.tool_registry().list_tools();
        let mut risk: Vec<(String, i32, String)> = tools
            .iter()
            .map(|tool| {
                let mut score = 100i32;
                if !tool.env_deps.is_empty() {
                    score -= 15;
                }
                if matches!(
                    tool.name.as_str(),
                    "terminal" | "execute_code" | "shell_exec" | "bash" | "python_exec"
                ) {
                    score -= 35;
                }
                if tool.toolset.eq_ignore_ascii_case("network")
                    || tool.name.contains("webhook")
                    || tool.name.contains("http")
                {
                    score -= 20;
                }
                if tool.name.contains("secrets")
                    || tool.name.contains("token")
                    || tool.name.contains("oauth")
                {
                    score -= 25;
                }
                score = score.clamp(0, 100);
                let tier = if score >= 80 {
                    "low-risk"
                } else if score >= 55 {
                    "moderate-risk"
                } else {
                    "high-risk"
                };
                (tool.name.clone(), score, tier.to_string())
            })
            .collect();
        risk.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let mut out = String::new();
        out.push_str("Tool trust scorecard (heuristic)\n");
        out.push_str("--------------------------------\n");
        let _ = writeln!(
            out,
            "policy_counters: allow={} deny={} audit_only={} simulate={} would_block={}",
            counters.allow,
            counters.deny,
            counters.audit_only,
            counters.simulate,
            counters.would_block
        );
        let _ = writeln!(out, "registered_tools={}", risk.len());
        for (name, score, tier) in risk.iter().take(20) {
            let _ = writeln!(out, "- {name:<28} score={score:>3} tier={tier}");
        }
        out.push_str("\nUse `/ops status` and `/raw trace verify` for live enforcement + trace integrity signals.");
        emit_command_output(host, out.trim_end());
        return Ok(CommandResult::Handled);
    }

    let tools = host.tool_registry().list_tools();
    if tools.is_empty() {
        emit_command_output(host, "No tools registered.");
    } else {
        let mut out = format!("Registered tools ({}):\n", tools.len());
        for tool in &tools {
            out.push_str(&format!("- `{}` — {}\n", tool.name, tool.description));
        }
        out.push_str("\n\nUse `/tools trust` for a risk/score summary.");
        emit_command_output(host, out.trim_end());
    }
    Ok(CommandResult::Handled)
}
