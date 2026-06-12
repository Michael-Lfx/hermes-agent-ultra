//! Shared plan-mode parsing and turn preparation (TUI/CLI + messaging gateway).

use hermes_agent::AgentLoop;
use hermes_tools::PlanPhase;

/// Parsed `/plan-mode` slash subcommand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanModeSlashAction {
    Help,
    On,
    Off,
    Status,
    Approve,
    Reject { feedback: String },
    Edit { plan: String },
    Task { task: String },
}

/// Plain-text approval while a plan is pending (non-slash reply).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanApprovalReply {
    Approve,
    Reject { feedback: Option<String> },
    Edit { plan: String },
}

/// How to run the next agent turn w.r.t. plan mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanTurnPrep {
    /// Run agent with the provided user message.
    Run { user_message: String },
    /// Do not invoke the agent; surface this reply to the user immediately.
    ReplyOnly { text: String },
}

/// Stricter plain-text approval matching for messaging channels vs TUI/CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanApprovalParseStyle {
    Interactive,
    Channel,
}

const PLAN_MODE_SUBCOMMANDS: &[&str] = &[
    "on", "enable", "off", "disable", "status", "show", "approve", "accept", "a", "reject", "deny",
    "r", "edit", "e", "help", "usage",
];

/// Whether the first token of a `/plan-mode` task is a reserved subcommand.
pub fn is_plan_mode_subcommand(word: &str) -> bool {
    PLAN_MODE_SUBCOMMANDS.contains(&word.to_ascii_lowercase().as_str())
}

pub fn parse_plan_mode_slash_args(args: &str) -> PlanModeSlashAction {
    let trimmed = args.trim();
    if trimmed.is_empty() {
        return PlanModeSlashAction::Help;
    }
    let mut parts = trimmed.splitn(2, char::is_whitespace);
    let sub = parts.next().unwrap_or("").to_ascii_lowercase();
    let rest = parts.next().unwrap_or("").trim().to_string();
    match sub.as_str() {
        "help" | "usage" | "?" => PlanModeSlashAction::Help,
        "on" | "enable" => {
            if rest.is_empty() {
                PlanModeSlashAction::On
            } else {
                PlanModeSlashAction::Task { task: rest }
            }
        }
        "off" | "disable" => PlanModeSlashAction::Off,
        "status" | "show" => PlanModeSlashAction::Status,
        "approve" | "accept" | "a" => PlanModeSlashAction::Approve,
        "reject" | "deny" | "r" => PlanModeSlashAction::Reject { feedback: rest },
        "edit" | "e" => PlanModeSlashAction::Edit { plan: rest },
        _ => PlanModeSlashAction::Task {
            task: trimmed.to_string(),
        },
    }
}

pub fn parse_plan_approval_reply(
    text: &str,
    style: PlanApprovalParseStyle,
) -> Option<PlanApprovalReply> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    let approve = match style {
        PlanApprovalParseStyle::Interactive => {
            matches!(
                lower.as_str(),
                "approve"
                    | "approved"
                    | "accept"
                    | "a"
                    | "yes"
                    | "y"
                    | "ok"
                    | "好的"
                    | "批准"
                    | "同意"
                    | "通过"
            )
        }
        PlanApprovalParseStyle::Channel => matches!(
            lower.as_str(),
            "approve" | "approved" | "accept" | "好的" | "批准" | "同意" | "通过"
        ),
    };
    if approve {
        return Some(PlanApprovalReply::Approve);
    }

    let bare_reject = match style {
        PlanApprovalParseStyle::Interactive => {
            matches!(
                lower.as_str(),
                "reject" | "deny" | "r" | "no" | "n" | "拒绝" | "驳回"
            )
        }
        PlanApprovalParseStyle::Channel => {
            matches!(
                lower.as_str(),
                "reject" | "deny" | "no" | "n" | "拒绝" | "驳回"
            )
        }
    };
    if bare_reject {
        return Some(PlanApprovalReply::Reject { feedback: None });
    }

    if lower.starts_with("reject ") || lower.starts_with("拒绝") {
        let feedback = trimmed
            .splitn(2, char::is_whitespace)
            .nth(1)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        return Some(PlanApprovalReply::Reject { feedback });
    }

    if lower.starts_with("edit ") || lower.starts_with("修订") {
        let plan = trimmed
            .splitn(2, char::is_whitespace)
            .nth(1)
            .unwrap_or("")
            .trim()
            .to_string();
        if plan.is_empty() {
            return None;
        }
        return Some(PlanApprovalReply::Edit { plan });
    }

    None
}

pub fn plan_mode_help_text() -> &'static str {
    "Plan mode (plan-then-execute):\n\
     /plan-mode <task>     Plan with read-only tools, then wait for approval\n\
     /plan-mode on         Enable plan mode for the next task\n\
     /plan-mode off        Disable plan mode\n\
     /plan-mode status     Show current phase\n\
     /plan-mode approve    Approve pending plan and execute\n\
     /plan-mode reject [feedback]  Reject plan\n\
     /plan-mode edit <text>  Revise plan and execute\n\
     While awaiting approval you can also reply: 批准 / approve / 拒绝 / reject"
}

pub fn plan_mode_status_text(agent: &AgentLoop) -> String {
    let phase = agent.plan_phase();
    let pending = agent
        .pending_plan()
        .map(|p| format!("\nPending plan ({} chars).", p.chars().count()))
        .unwrap_or_default();
    format!("Plan mode phase: {}{}", phase.as_str(), pending)
}

pub fn format_plan_pending_reply(plan: &str) -> String {
    format!(
        "📋 Plan submitted — review below, then reply:\n\
         • /plan-mode approve  (or: 批准 / approve)\n\
         • /plan-mode reject [feedback]  (or: 拒绝)\n\
         • /plan-mode edit <revised plan>\n\n\
         ---\n{plan}\n---"
    )
}

pub fn plan_awaiting_approval_reminder(pending_plan: Option<&str>) -> String {
    let plan_hint = pending_plan
        .filter(|p| !p.trim().is_empty())
        .map(|p| format!("\n\n(Pending plan: {} chars)", p.chars().count()))
        .unwrap_or_default();
    format!(
        "⏸ Plan awaiting your approval.{plan_hint}\n\
         Reply with: /plan-mode approve | reject [feedback] | edit <plan>\n\
         Or send: 批准 / approve | 拒绝 / reject"
    )
}

pub fn prepare_plan_turn(
    agent: &AgentLoop,
    user_message: &str,
    style: PlanApprovalParseStyle,
) -> PlanTurnPrep {
    if agent.plan_phase() != PlanPhase::AwaitingApproval {
        return PlanTurnPrep::Run {
            user_message: user_message.to_string(),
        };
    }

    let Some(action) = parse_plan_approval_reply(user_message, style) else {
        return PlanTurnPrep::ReplyOnly {
            text: plan_awaiting_approval_reminder(agent.pending_plan().as_deref()),
        };
    };

    match action {
        PlanApprovalReply::Approve => {
            agent.set_plan_phase(PlanPhase::Executing);
            PlanTurnPrep::Run {
                user_message: "Plan approved. Proceed with execution.".to_string(),
            }
        }
        PlanApprovalReply::Reject { feedback } => {
            agent.set_plan_phase(PlanPhase::Planning);
            agent.set_pending_plan(None);
            if feedback.as_ref().is_none_or(|s| s.trim().is_empty()) {
                return PlanTurnPrep::ReplyOnly {
                    text: "Plan rejected. Revise your request or send a new task with /plan-mode."
                        .to_string(),
                };
            }
            PlanTurnPrep::Run {
                user_message: format!(
                    "Plan rejected. User feedback: {}",
                    feedback.unwrap_or_default()
                ),
            }
        }
        PlanApprovalReply::Edit { plan } => {
            agent.set_pending_plan(Some(plan.clone()));
            agent.set_plan_phase(PlanPhase::Executing);
            PlanTurnPrep::Run {
                user_message: format!("Plan updated and approved:\n{plan}"),
            }
        }
    }
}

/// Execute a parsed `/plan-mode` slash action in the interactive TUI/CLI app.
pub async fn handle_plan_mode_slash_action(
    host: &mut impl crate::app::SlashCommandHost,
    action: PlanModeSlashAction,
) -> Result<(), hermes_core::AgentError> {
    use hermes_core::Message;

    match action {
        PlanModeSlashAction::Help => {
            crate::commands::emit_command_output(host, plan_mode_help_text());
        }
        PlanModeSlashAction::On => {
            host.agent().set_plan_phase(PlanPhase::Planning);
            crate::commands::emit_command_output(
                host,
                "Plan mode ON: agent will research with read-only tools, submit a plan, and wait for approval.",
            );
        }
        PlanModeSlashAction::Off => {
            host.agent().set_plan_phase(PlanPhase::Off);
            host.agent().set_pending_plan(None);
            crate::commands::emit_command_output(host, "Plan mode OFF.");
        }
        PlanModeSlashAction::Status => {
            crate::commands::emit_command_output(host, plan_mode_status_text(host.agent()));
        }
        PlanModeSlashAction::Approve => {
            if host.agent().plan_phase() != PlanPhase::AwaitingApproval {
                crate::commands::emit_command_output(
                    host,
                    "No plan awaiting approval. Use /plan-mode <task> or /plan-mode on first.",
                );
                return Ok(());
            }
            host.agent().set_plan_phase(PlanPhase::Executing);
            host.messages_mut()
                .push(Message::user("Plan approved. Proceed with execution."));
            host.run_agent_turn().await?;
        }
        PlanModeSlashAction::Reject { feedback } => {
            host.agent().set_plan_phase(PlanPhase::Planning);
            host.agent().set_pending_plan(None);
            if feedback.trim().is_empty() {
                crate::commands::emit_command_output(
                    host,
                    "Plan rejected. Revise your request or run /plan-mode on again.",
                );
            } else {
                host.messages_mut().push(Message::user(format!(
                    "Plan rejected. User feedback: {feedback}"
                )));
                host.run_agent_turn().await?;
            }
        }
        PlanModeSlashAction::Edit { plan } => {
            if plan.trim().is_empty() {
                crate::commands::emit_command_output(
                    host,
                    "Usage: /plan-mode edit <revised plan text>",
                );
                return Ok(());
            }
            host.agent().set_pending_plan(Some(plan.clone()));
            host.agent().set_plan_phase(PlanPhase::Executing);
            host.messages_mut()
                .push(Message::user(format!("Plan updated and approved:\n{plan}")));
            host.run_agent_turn().await?;
        }
        PlanModeSlashAction::Task { task } => {
            if task.trim().is_empty() {
                crate::commands::emit_command_output(host, plan_mode_help_text());
                return Ok(());
            }
            host.agent().set_plan_phase(PlanPhase::Planning);
            host.messages_mut().push(Message::user(task));
            host.run_agent_turn().await?;
        }
    }
    Ok(())
}

pub fn finalize_plan_agent_reply(
    agent: &AgentLoop,
    conv: &hermes_agent::ConversationResult,
) -> String {
    if conv.turn_exit_reason() == "plan_awaiting_approval" {
        if let Some(plan) = agent.pending_plan() {
            return format_plan_pending_reply(&plan);
        }
    }
    conv.final_response
        .as_ref()
        .filter(|s| !s.trim().is_empty())
        .cloned()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slash_task_one_liner() {
        assert_eq!(
            parse_plan_mode_slash_args("帮我做推广"),
            PlanModeSlashAction::Task {
                task: "帮我做推广".to_string()
            }
        );
    }

    #[test]
    fn slash_on_with_task() {
        assert_eq!(
            parse_plan_mode_slash_args("on 写测试"),
            PlanModeSlashAction::Task {
                task: "写测试".to_string()
            }
        );
    }

    #[test]
    fn approval_reply_chinese_channel() {
        assert_eq!(
            parse_plan_approval_reply("批准", PlanApprovalParseStyle::Channel),
            Some(PlanApprovalReply::Approve)
        );
        assert_eq!(
            parse_plan_approval_reply("拒绝 太复杂", PlanApprovalParseStyle::Channel),
            Some(PlanApprovalReply::Reject {
                feedback: Some("太复杂".to_string())
            })
        );
    }

    #[test]
    fn channel_rejects_short_approval_aliases() {
        assert_eq!(
            parse_plan_approval_reply("ok", PlanApprovalParseStyle::Channel),
            None
        );
        assert_eq!(
            parse_plan_approval_reply("y", PlanApprovalParseStyle::Channel),
            None
        );
    }

    #[test]
    fn interactive_allows_short_approval_aliases() {
        assert_eq!(
            parse_plan_approval_reply("ok", PlanApprovalParseStyle::Interactive),
            Some(PlanApprovalReply::Approve)
        );
    }

    #[test]
    fn plan_mode_subcommand_detection() {
        assert!(is_plan_mode_subcommand("on"));
        assert!(is_plan_mode_subcommand("APPROVE"));
        assert!(!is_plan_mode_subcommand("帮我做推广"));
    }

    #[test]
    fn awaiting_approval_reminder_mentions_approve() {
        let text = plan_awaiting_approval_reminder(Some("step 1"));
        assert!(text.contains("approve"));
        assert!(text.contains("批准"));
    }

    #[test]
    fn unrecognized_reply_not_parsed_as_approval() {
        assert_eq!(
            parse_plan_approval_reply("再改一下第二点", PlanApprovalParseStyle::Channel),
            None
        );
    }
}
