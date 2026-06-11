//! `/subconscious` slash command handler and queue state.

use std::fmt::Write as _;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::alpha_runtime::utility_terms_from_contract;
use crate::commands::background;
use crate::commands::{CommandResult, emit_command_output, truncate_chars};
use crate::env_vars;
use hermes_core::AgentError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SubconsciousTask {
    pub(crate) id: String,
    pub(crate) source: String,
    pub(crate) prompt: String,
    pub(crate) score: f64,
    pub(crate) risk: String,
    pub(crate) requires_approval: bool,
    pub(crate) status: String,
    #[serde(default)]
    pub(crate) job_id: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct SubconsciousQueueState {
    #[serde(default)]
    pub(crate) tasks: Vec<SubconsciousTask>,
}

fn subconscious_state_path() -> PathBuf {
    hermes_config::hermes_home()
        .join("subconscious")
        .join("queue.json")
}

pub(crate) fn load_subconscious_state() -> SubconsciousQueueState {
    let path = subconscious_state_path();
    let raw = std::fs::read_to_string(path).unwrap_or_default();
    serde_json::from_str::<SubconsciousQueueState>(&raw).unwrap_or_default()
}

pub(crate) fn save_subconscious_state(state: &SubconsciousQueueState) -> Result<(), AgentError> {
    let path = subconscious_state_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AgentError::Io(format!("Failed to create {}: {}", parent.display(), e)))?;
    }
    let payload = serde_json::to_string_pretty(state)
        .map_err(|e| AgentError::Io(format!("Failed to encode subconscious state: {}", e)))?;
    std::fs::write(&path, payload)
        .map_err(|e| AgentError::Io(format!("Failed to write {}: {}", path.display(), e)))?;
    Ok(())
}

#[cfg(test)]
pub(crate) fn subconscious_test_high_risk_state() -> SubconsciousQueueState {
    let now = chrono::Utc::now().to_rfc3339();
    SubconsciousQueueState {
        tasks: vec![SubconsciousTask {
            id: "sc-risky".to_string(),
            source: "test".to_string(),
            prompt: "rotate key and deploy to prod".to_string(),
            score: 4.2,
            risk: "high".to_string(),
            requires_approval: false,
            status: "pending".to_string(),
            job_id: None,
            created_at: now.clone(),
            updated_at: now,
        }],
    }
}

pub(crate) fn score_subconscious_task(prompt: &str) -> f64 {
    let text = prompt.to_ascii_lowercase();
    let mut score = 1.0f64;
    if text.contains("profit")
        || text.contains("wallet")
        || text.contains("sol")
        || text.contains("latency")
        || text.contains("regression")
    {
        score += 1.2;
    }
    if text.contains("fix") || text.contains("verify") || text.contains("test") {
        score += 0.8;
    }
    if let Ok(terms) = utility_terms_from_contract() {
        let mut overlap = 0.0f64;
        for (term, weight) in terms {
            if text.contains(&term.to_ascii_lowercase()) {
                overlap += weight.max(0.0);
            }
        }
        score += overlap.min(2.5);
    }
    score
}

fn risk_for_prompt(prompt: &str) -> (&'static str, bool) {
    let text = prompt.to_ascii_lowercase();
    if text.contains("rm -rf")
        || text.contains("delete ")
        || text.contains("rotate key")
        || text.contains("prod")
        || text.contains("mainnet")
    {
        return ("high", true);
    }
    if text.contains("live trading") || text.contains("wallet") || text.contains("deploy") {
        return ("medium", true);
    }
    ("low", false)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SubconsciousProfile {
    Strict,
    Balanced,
    Dev,
}

impl SubconsciousProfile {
    fn as_str(self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::Balanced => "balanced",
            Self::Dev => "dev",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "strict" => Some(Self::Strict),
            "balanced" | "standard" => Some(Self::Balanced),
            "dev" => Some(Self::Dev),
            _ => None,
        }
    }
}

fn subconscious_profile_env() -> SubconsciousProfile {
    std::env::var("HERMES_SUBCONSCIOUS_PROFILE")
        .ok()
        .and_then(|v| SubconsciousProfile::parse(&v))
        .unwrap_or(SubconsciousProfile::Balanced)
}

fn subconscious_guard_allows(
    profile: SubconsciousProfile,
    task: &SubconsciousTask,
) -> (bool, String) {
    let risk = task.risk.to_ascii_lowercase();
    match profile {
        SubconsciousProfile::Dev => (true, "dev profile allows execution".to_string()),
        SubconsciousProfile::Balanced => {
            if risk == "high" {
                (
                    false,
                    "balanced profile blocks high-risk subconscious runs".to_string(),
                )
            } else {
                (true, "balanced profile allows low/medium risk".to_string())
            }
        }
        SubconsciousProfile::Strict => {
            if task.requires_approval || risk != "low" {
                (
                    false,
                    "strict profile allows only low-risk non-approval tasks".to_string(),
                )
            } else {
                (true, "strict profile allows low-risk task".to_string())
            }
        }
    }
}

// ---------------------------------------------------------------------------
// /subconscious
// ---------------------------------------------------------------------------

pub(crate) fn handle_subconscious_command(
    host: &mut impl crate::app::SlashCommandHost,
    args: &[&str],
) -> Result<CommandResult, AgentError> {
    let action = args
        .first()
        .copied()
        .unwrap_or("status")
        .to_ascii_lowercase();
    match action.as_str() {
        "status" | "list" => {
            let state = load_subconscious_state();
            let profile = subconscious_profile_env();
            let mut out = String::new();
            out.push_str("Subconscious queue\n");
            out.push_str("-----------------\n");
            let _ = writeln!(out, "profile: {}", profile.as_str());
            if state.tasks.is_empty() {
                out.push_str("No queued subconscious tasks.\n");
            } else {
                for task in state.tasks.iter().rev().take(24) {
                    let _ = writeln!(
                        out,
                        "- {} [{}] score={:.2} risk={} approval={} source={} :: {}",
                        task.id,
                        task.status,
                        task.score,
                        task.risk,
                        task.requires_approval,
                        task.source,
                        truncate_chars(&task.prompt, 100)
                    );
                }
            }
            out.push_str(
                "\nUsage: /subconscious add <prompt> | approve <id> | reject <id> | run [n] [--dry-run] [profile=<strict|balanced|dev>] | profile [status|list|strict|balanced|dev|clear] | clear",
            );
            emit_command_output(host, out.trim_end());
        }
        "add" => {
            let prompt = args.get(1..).unwrap_or(&[]).join(" ").trim().to_string();
            if prompt.is_empty() {
                emit_command_output(host, "Usage: /subconscious add <prompt>");
                return Ok(CommandResult::Handled);
            }
            let (risk, requires_approval) = risk_for_prompt(&prompt);
            let score = score_subconscious_task(&prompt);
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
            let task = SubconsciousTask {
                id: id.clone(),
                source: "manual".to_string(),
                prompt,
                score,
                risk: risk.to_string(),
                requires_approval,
                status: if requires_approval {
                    "pending-approval".to_string()
                } else {
                    "pending".to_string()
                },
                job_id: None,
                created_at: chrono::Utc::now().to_rfc3339(),
                updated_at: chrono::Utc::now().to_rfc3339(),
            };
            state.tasks.push(task.clone());
            save_subconscious_state(&state)?;
            emit_command_output(
                host,
                format!(
                    "Queued subconscious task {}\nstatus={} score={:.2} risk={}\n{}",
                    task.id,
                    task.status,
                    task.score,
                    task.risk,
                    if task.requires_approval {
                        "Requires approval: /subconscious approve <id>"
                    } else {
                        "Ready to run: /subconscious run"
                    }
                ),
            );
        }
        "approve" | "reject" => {
            let Some(task_id) = args.get(1).copied() else {
                emit_command_output(host, format!("Usage: /subconscious {} <id>", action));
                return Ok(CommandResult::Handled);
            };
            let mut state = load_subconscious_state();
            let mut found = false;
            for task in &mut state.tasks {
                if task.id.eq_ignore_ascii_case(task_id) {
                    found = true;
                    task.status = if action == "approve" {
                        "pending".to_string()
                    } else {
                        "rejected".to_string()
                    };
                    task.updated_at = chrono::Utc::now().to_rfc3339();
                    break;
                }
            }
            if !found {
                emit_command_output(host, format!("Task not found: {}", task_id));
                return Ok(CommandResult::Handled);
            }
            save_subconscious_state(&state)?;
            emit_command_output(host, format!("Subconscious task {} {}", task_id, action));
        }
        "run" => {
            let mut limit = 1usize;
            let mut dry_run = false;
            let mut profile_override: Option<SubconsciousProfile> = None;
            for token in args.get(1..).unwrap_or(&[]) {
                let token_l = token.trim().to_ascii_lowercase();
                if token_l == "--dry-run" || token_l == "dry-run" || token_l == "preview" {
                    dry_run = true;
                    continue;
                }
                if let Ok(parsed) = token_l.parse::<usize>() {
                    limit = parsed.clamp(1, 8);
                    continue;
                }
                if let Some(raw) = token_l.strip_prefix("profile=") {
                    profile_override = SubconsciousProfile::parse(raw);
                    continue;
                }
                if profile_override.is_none() {
                    profile_override = SubconsciousProfile::parse(&token_l);
                }
            }
            let profile = profile_override.unwrap_or_else(subconscious_profile_env);
            let mut state = load_subconscious_state();
            let mut reviewed = 0usize;
            let mut dispatched = 0usize;
            let mut blocked = 0usize;
            let mut notes = Vec::new();
            for task in &mut state.tasks {
                if reviewed >= limit {
                    break;
                }
                if task.status != "pending" {
                    continue;
                }
                reviewed += 1;
                let (allowed, guard_note) = subconscious_guard_allows(profile, task);
                if !allowed {
                    blocked += 1;
                    notes.push(format!("{} blocked ({})", task.id, guard_note));
                    continue;
                }
                if dry_run {
                    notes.push(format!("{} would dispatch ({})", task.id, guard_note));
                    continue;
                }
                let job = background::queue_background_job(&task.prompt)?;
                task.status = "dispatched".to_string();
                task.job_id = Some(job.id.clone());
                task.updated_at = chrono::Utc::now().to_rfc3339();
                dispatched += 1;
                notes.push(format!("{} dispatched id={}", task.id, job.id));
            }
            if !dry_run {
                save_subconscious_state(&state)?;
            }
            emit_command_output(
                host,
                format!(
                    "{} subconscious run profile={}\nreviewed={} dispatched={} blocked={}\n{}\nUse `/background status` and `/subconscious status` for tracking.",
                    if dry_run { "Dry-run" } else { "Executed" },
                    profile.as_str(),
                    reviewed,
                    dispatched,
                    blocked,
                    if notes.is_empty() {
                        "No pending tasks matched selection.".to_string()
                    } else {
                        notes.join("\n")
                    }
                ),
            );
        }
        "profile" => {
            let token = args
                .get(1)
                .copied()
                .unwrap_or("status")
                .to_ascii_lowercase();
            match token.as_str() {
                "status" | "show" => emit_command_output(
                    host,
                    format!(
                        "Subconscious profile: {}\nUse `/subconscious profile list` or `/subconscious profile strict|balanced|dev`.",
                        subconscious_profile_env().as_str()
                    ),
                ),
                "list" => emit_command_output(
                    host,
                    "Subconscious profiles:\n- strict: only low-risk non-approval tasks auto-dispatch\n- balanced: low/medium dispatch, high-risk blocked\n- dev: permit all pending tasks\nSet with `/subconscious profile <name>`.",
                ),
                "clear" => {
                    env_vars::remove_var("HERMES_SUBCONSCIOUS_PROFILE");
                    emit_command_output(
                        host,
                        "Cleared subconscious profile override (default=balanced).",
                    );
                }
                other => {
                    let Some(next) = SubconsciousProfile::parse(other) else {
                        emit_command_output(
                            host,
                            "Usage: /subconscious profile [status|list|strict|balanced|dev|clear]",
                        );
                        return Ok(CommandResult::Handled);
                    };
                    env_vars::set_var("HERMES_SUBCONSCIOUS_PROFILE", next.as_str());
                    emit_command_output(
                        host,
                        format!("Subconscious profile set to {}.", next.as_str()),
                    );
                }
            }
        }
        "clear" => {
            let path = subconscious_state_path();
            if path.exists() {
                std::fs::remove_file(&path).map_err(|e| {
                    AgentError::Io(format!("Failed to remove {}: {}", path.display(), e))
                })?;
            }
            emit_command_output(host, "Cleared subconscious queue.");
        }
        _ => emit_command_output(
            host,
            "Usage: /subconscious [status|add <prompt>|approve <id>|reject <id>|run [n] [--dry-run] [profile=<strict|balanced|dev>]|profile [status|list|strict|balanced|dev|clear]|clear]",
        ),
    }
    Ok(CommandResult::Handled)
}
