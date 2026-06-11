//! Trigger triage types, helpers, and `/triage` handler.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::subconscious::{
    SubconsciousTask, load_subconscious_state, save_subconscious_state, score_subconscious_task,
};
use crate::commands::background;
use crate::commands::{CommandResult, emit_command_output, truncate_chars};
use hermes_core::AgentError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum TriggerTriageDecision {
    Drop,
    Notify,
    Escalate,
    AgentRun,
}

impl TriggerTriageDecision {
    fn as_str(self) -> &'static str {
        match self {
            Self::Drop => "drop",
            Self::Notify => "notify",
            Self::Escalate => "escalate",
            Self::AgentRun => "agent-run",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TriggerTriageAssessment {
    pub(crate) source: String,
    pub(crate) payload: String,
    pub(crate) severity: i32,
    pub(crate) decision: TriggerTriageDecision,
    pub(crate) requires_approval: bool,
    pub(crate) reasons: Vec<String>,
}

fn trigger_triage_mode() -> String {
    std::env::var("HERMES_TRIGGER_TRIAGE_MODE")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(|v| v.trim().to_ascii_lowercase())
        .unwrap_or_else(|| "off".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TriggerTriageLearningEntry {
    at: String,
    source: String,
    outcome: String,
    decision: String,
    severity: i32,
    bias_delta: i32,
    note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct TriggerTriageLearningState {
    #[serde(default)]
    entries: Vec<TriggerTriageLearningEntry>,
}

pub(crate) fn trigger_triage_learning_state_path() -> PathBuf {
    hermes_config::hermes_home()
        .join("triage")
        .join("learning.json")
}

fn load_trigger_triage_learning_state() -> TriggerTriageLearningState {
    let path = trigger_triage_learning_state_path();
    let raw = std::fs::read_to_string(path).unwrap_or_default();
    serde_json::from_str::<TriggerTriageLearningState>(&raw).unwrap_or_default()
}

fn save_trigger_triage_learning_state(
    state: &TriggerTriageLearningState,
) -> Result<(), AgentError> {
    let path = trigger_triage_learning_state_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AgentError::Io(format!("Failed to create {}: {}", parent.display(), e)))?;
    }
    let payload = serde_json::to_string_pretty(state)
        .map_err(|e| AgentError::Io(format!("Failed to encode triage learning state: {}", e)))?;
    std::fs::write(&path, payload)
        .map_err(|e| AgentError::Io(format!("Failed to write {}: {}", path.display(), e)))?;
    Ok(())
}

fn triage_feedback_delta(outcome: &str) -> Option<i32> {
    match outcome.trim().to_ascii_lowercase().as_str() {
        "critical" | "escalate" | "confirmed" | "true-positive" | "tp" => Some(2),
        "useful" | "good" | "notify" | "watch" => Some(1),
        "neutral" | "mixed" => Some(0),
        "false-positive" | "fp" | "noise" | "noisy" => Some(-2),
        "drop" | "ignore" | "spam" => Some(-1),
        _ => None,
    }
}

pub(crate) fn triage_learning_bias(source: &str, payload: &str) -> (i32, Vec<String>) {
    let source_l = source.trim().to_ascii_lowercase();
    let payload_l = payload.trim().to_ascii_lowercase();
    let state = load_trigger_triage_learning_state();
    let mut total = 0i32;
    let mut reasons = Vec::new();
    for entry in state.entries.iter().rev().take(120) {
        if entry.source.eq_ignore_ascii_case(&source_l) {
            total += entry.bias_delta;
            if reasons.len() < 3 {
                reasons.push(format!(
                    "source feedback {} ({})",
                    entry.outcome, entry.bias_delta
                ));
            }
            continue;
        }
        if !entry.note.trim().is_empty()
            && payload_l.contains(entry.note.trim().to_ascii_lowercase().as_str())
        {
            total += entry.bias_delta.signum();
            if reasons.len() < 3 {
                reasons.push(format!("matched prior note '{}'", entry.note));
            }
        }
    }
    (total.clamp(-3, 3), reasons)
}

pub(crate) fn evaluate_trigger_triage(source: &str, payload: &str) -> TriggerTriageAssessment {
    let source_l = source.trim().to_ascii_lowercase();
    let payload_l = payload.trim().to_ascii_lowercase();
    let mode = trigger_triage_mode();
    let mut severity = 0i32;
    let mut reasons = Vec::new();

    for (needle, score, reason) in [
        ("panic", 4, "runtime panic or crash"),
        ("outage", 4, "service outage signal"),
        ("secret", 5, "secret exposure indicator"),
        ("key leak", 5, "key leak indicator"),
        ("drawdown", 4, "drawdown or loss event"),
        ("halt", 3, "trading halt or critical gate"),
        ("blocked", 2, "policy or sandbox block"),
        ("timeout", 1, "timeout/retry pressure"),
        ("latency", 1, "latency degradation"),
        ("error", 2, "error signal"),
    ] {
        if payload_l.contains(needle) || source_l.contains(needle) {
            severity += score;
            reasons.push(reason.to_string());
        }
    }

    if source_l.contains("webhook") {
        severity += 1;
        reasons.push("external webhook trigger".to_string());
    }
    if source_l.contains("cron") {
        severity += 1;
        reasons.push("scheduled trigger".to_string());
    }

    let (learning_bias, learning_reasons) = triage_learning_bias(source, payload);
    if learning_bias != 0 {
        severity += learning_bias;
        reasons.push(format!("learning bias applied ({:+})", learning_bias));
        reasons.extend(learning_reasons);
    }

    if mode == "strict" {
        severity += 1;
    } else if mode == "relaxed" {
        severity = severity.saturating_sub(1);
    }

    let (decision, requires_approval) = if severity >= 7 {
        (TriggerTriageDecision::Escalate, true)
    } else if severity >= 4 {
        (TriggerTriageDecision::AgentRun, false)
    } else if severity >= 2 {
        (TriggerTriageDecision::Notify, false)
    } else if payload_l.len() < 6 {
        (TriggerTriageDecision::Drop, false)
    } else {
        (TriggerTriageDecision::Notify, false)
    };

    TriggerTriageAssessment {
        source: source.trim().to_string(),
        payload: payload.trim().to_string(),
        severity,
        decision,
        requires_approval,
        reasons,
    }
}

fn render_trigger_triage_assessment(assessment: &TriggerTriageAssessment) -> String {
    let mut out = String::new();
    out.push_str("Trigger triage assessment\n");
    out.push_str("------------------------\n");
    let _ = writeln!(out, "source: {}", assessment.source);
    let _ = writeln!(out, "payload: {}", truncate_chars(&assessment.payload, 220));
    let _ = writeln!(out, "severity: {}", assessment.severity);
    let _ = writeln!(out, "decision: {}", assessment.decision.as_str());
    let _ = writeln!(out, "requires_approval: {}", assessment.requires_approval);
    if assessment.reasons.is_empty() {
        out.push_str("reasons: none\n");
    } else {
        out.push_str("reasons:\n");
        for reason in &assessment.reasons {
            let _ = writeln!(out, "- {}", reason);
        }
    }
    out
}

pub(crate) fn append_triage_learning_feedback(
    source: &str,
    payload: &str,
    outcome: &str,
    assessment: &TriggerTriageAssessment,
) -> Result<TriggerTriageLearningEntry, AgentError> {
    let delta = triage_feedback_delta(outcome).ok_or_else(|| {
        AgentError::Config(
            "Unknown triage feedback outcome. Use critical|confirmed|useful|neutral|false-positive|drop."
                .to_string(),
        )
    })?;
    let note = payload
        .split_whitespace()
        .take(10)
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string();
    let entry = TriggerTriageLearningEntry {
        at: chrono::Utc::now().to_rfc3339(),
        source: source.trim().to_ascii_lowercase(),
        outcome: outcome.trim().to_ascii_lowercase(),
        decision: assessment.decision.as_str().to_string(),
        severity: assessment.severity,
        bias_delta: delta,
        note,
    };
    let mut state = load_trigger_triage_learning_state();
    state.entries.push(entry.clone());
    if state.entries.len() > 400 {
        let remove = state.entries.len().saturating_sub(400);
        state.entries.drain(0..remove);
    }
    save_trigger_triage_learning_state(&state)?;
    Ok(entry)
}

fn render_trigger_triage_learning_status() -> String {
    let state = load_trigger_triage_learning_state();
    let mut by_source: HashMap<String, i32> = HashMap::new();
    for entry in &state.entries {
        *by_source.entry(entry.source.clone()).or_insert(0) += entry.bias_delta;
    }
    let mut ranked = by_source.into_iter().collect::<Vec<_>>();
    ranked.sort_by(|a, b| b.1.cmp(&a.1));
    let mut out = String::new();
    out.push_str("Trigger triage learning\n");
    out.push_str("----------------------\n");
    let _ = writeln!(out, "entries: {}", state.entries.len());
    if ranked.is_empty() {
        out.push_str("source_bias: none\n");
    } else {
        out.push_str("source_bias:\n");
        for (source, bias) in ranked.into_iter().take(6) {
            let _ = writeln!(out, "- {} => {:+}", source, bias);
        }
    }
    if let Some(last) = state.entries.last() {
        let _ = writeln!(
            out,
            "last_feedback: {} source={} outcome={} delta={:+}",
            last.at, last.source, last.outcome, last.bias_delta
        );
    }
    out
}

pub(crate) fn handle_trigger_triage_command(
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
            emit_command_output(
                host,
                format!(
                    "Trigger triage mode: {}\n{}\nUsage: /triage eval <source> <payload> | /triage queue <source> <payload> | /triage feedback <source> <outcome> <payload>",
                    trigger_triage_mode(),
                    render_trigger_triage_learning_status().trim_end()
                ),
            );
        }
        "list" | "rules" => {
            emit_command_output(
                host,
                "Trigger triage heuristics\n\
                 - high severity: panic/outage/secret leak/drawdown/halt -> escalate\n\
                 - medium severity: repeated errors/blocked/timeout -> agent-run\n\
                 - low severity: notify\n\
                 - empty/noise payload -> drop\n\
                 Mode override: HERMES_TRIGGER_TRIAGE_MODE={strict|balanced|relaxed}\n\
                 Feedback loop: `/triage feedback <source> <outcome> <payload>` updates persistent bias.",
            );
        }
        "feedback" => {
            let Some(source) = args.get(1).copied() else {
                emit_command_output(host, "Usage: /triage feedback <source> <outcome> <payload>");
                return Ok(CommandResult::Handled);
            };
            let Some(outcome) = args.get(2).copied() else {
                emit_command_output(host, "Usage: /triage feedback <source> <outcome> <payload>");
                return Ok(CommandResult::Handled);
            };
            let payload = args.get(3..).unwrap_or(&[]).join(" ").trim().to_string();
            if payload.is_empty() {
                emit_command_output(host, "Usage: /triage feedback <source> <outcome> <payload>");
                return Ok(CommandResult::Handled);
            }
            let assessment = evaluate_trigger_triage(source, &payload);
            let entry = append_triage_learning_feedback(source, &payload, outcome, &assessment)?;
            let (bias_now, _) = triage_learning_bias(source, &payload);
            emit_command_output(
                host,
                format!(
                    "Recorded triage feedback.\nsource={} outcome={} delta={:+} decision={} severity={}\nsource_bias_now={:+}",
                    entry.source,
                    entry.outcome,
                    entry.bias_delta,
                    entry.decision,
                    entry.severity,
                    bias_now
                ),
            );
        }
        "eval" | "queue" => {
            let Some(source) = args.get(1).copied() else {
                emit_command_output(
                    host,
                    "Usage: /triage eval <source> <payload>\nUsage: /triage queue <source> <payload>",
                );
                return Ok(CommandResult::Handled);
            };
            let payload = args.get(2..).unwrap_or(&[]).join(" ");
            if payload.trim().is_empty() {
                emit_command_output(host, "Payload cannot be empty.");
                return Ok(CommandResult::Handled);
            }
            let assessment = evaluate_trigger_triage(source, &payload);
            let mut out = render_trigger_triage_assessment(&assessment);
            if action == "queue" {
                match assessment.decision {
                    TriggerTriageDecision::Drop => {
                        out.push_str("\n\nqueue_action: dropped");
                    }
                    TriggerTriageDecision::Notify => {
                        out.push_str("\n\nqueue_action: notify-only (no agent run queued)");
                    }
                    TriggerTriageDecision::Escalate => {
                        let mut state = load_subconscious_state();
                        let id = format!(
                            "sc-{}",
                            Uuid::new_v4()
                                .simple()
                                .to_string()
                                .chars()
                                .take(8)
                                .collect::<String>()
                        );
                        state.tasks.push(SubconsciousTask {
                            id: id.clone(),
                            source: source.to_string(),
                            prompt: payload.trim().to_string(),
                            score: score_subconscious_task(&payload),
                            risk: "high".to_string(),
                            requires_approval: true,
                            status: "pending-approval".to_string(),
                            job_id: None,
                            created_at: chrono::Utc::now().to_rfc3339(),
                            updated_at: chrono::Utc::now().to_rfc3339(),
                        });
                        save_subconscious_state(&state)?;
                        let _ = write!(
                            out,
                            "\n\nqueue_action: escalated to subconscious queue as {} (requires approval)",
                            id
                        );
                    }
                    TriggerTriageDecision::AgentRun => {
                        let job = background::queue_background_job(payload.trim())?;
                        let _ = write!(
                            out,
                            "\n\nqueue_action: background job queued id={} status_file={}",
                            job.id,
                            job.status_path.display()
                        );
                    }
                }
            }
            emit_command_output(host, out);
        }
        _ => emit_command_output(
            host,
            "Usage: /triage [status|list|eval <source> <payload>|queue <source> <payload>|feedback <source> <outcome> <payload>]",
        ),
    }
    Ok(CommandResult::Handled)
}
