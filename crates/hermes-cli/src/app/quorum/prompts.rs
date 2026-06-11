use std::path::PathBuf;

use crate::alpha_runtime::QuorumPolicy;
use hermes_core::Message;

use super::super::App;
use super::{
    QUORUM_AGENT_CONTRACT_DEFAULT_PATH, QUORUM_MAX_VOTER_OUTPUT_CHARS, QuorumVoterOutcome,
};

impl App {
    pub(super) fn quorum_output_char_cap() -> Option<usize> {
        if let Ok(raw) = std::env::var("HERMES_QUORUM_MAX_VOTER_OUTPUT_CHARS") {
            if Self::is_unbounded_token(&raw) {
                return None;
            }
            if let Some(parsed) = raw.trim().parse::<usize>().ok().filter(|v| *v > 0) {
                return Some(parsed);
            }
        }
        Some(QUORUM_MAX_VOTER_OUTPUT_CHARS)
    }

    pub(super) fn load_quorum_agent_contract_text(&self) -> Option<(PathBuf, String)> {
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Ok(raw) = std::env::var("HERMES_QUORUM_AGENT_CONTRACT_PATH") {
            let path = PathBuf::from(raw.trim());
            if !path.as_os_str().is_empty() {
                candidates.push(path);
            }
        }
        candidates.push(self.state_root.join("quorum").join("AGENTS.md"));
        candidates.push(PathBuf::from(QUORUM_AGENT_CONTRACT_DEFAULT_PATH));
        for path in candidates {
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            let trimmed = content.trim();
            if trimmed.is_empty() {
                continue;
            }
            return Some((path, trimmed.to_string()));
        }
        None
    }

    pub(super) fn build_quorum_voter_prompt(
        pass_index: usize,
        total_passes: usize,
        model: &str,
    ) -> String {
        if pass_index == 0 {
            return format!(
                "[QUORUM_VOTER] model={}\n\
                 You are in deep-voter mode. Act like quality is existential.\n\
                 Hard requirements:\n\
                 1) exhaustive exploration before conclusion,\n\
                 2) contradiction/null-hypothesis attack,\n\
                 3) final synthesis with explicit confidence and risk caveats,\n\
                 4) no placeholder names, no fake files, no invented metrics.\n\
                 Verification requirements:\n\
                 - every file/module claim must include an absolute path and exists_now=true/false\n\
                 - if you cannot verify a claim, mark it UNPROVEN (never guess)\n\
                 - include evidence bullets from tools/data/reasoning traces\n\
                 - include at least one counter-argument before final answer.\n\
                 Language requirement: answer in English unless the user explicitly requests another language.\n\
                 This is pass {}/{}.",
                model,
                pass_index + 1,
                total_passes
            );
        }
        format!(
            "[QUORUM_VOTER_REVIEW] pass {}/{}\n\
             Critique and strengthen your prior answer.\n\
             - Assume the previous draft is partially wrong.\n\
             - Remove any unverified file names/modules/metrics.\n\
             - Fix weak claims, tighten evidence, and improve actionability.\n\
             - Keep the answer in English unless the user explicitly requested another language.\n\
             - Keep objective truth over optimism.",
            pass_index + 1,
            total_passes
        )
    }

    pub(in crate::app) fn extract_last_assistant_output(messages: &[Message]) -> String {
        for message in messages.iter().rev() {
            if message.role != hermes_core::MessageRole::Assistant {
                continue;
            }
            if let Some(content) = message.content.as_deref() {
                let trimmed = content.trim();
                if !trimmed.is_empty() {
                    return trimmed.to_string();
                }
            }
            if let Some(reasoning) = message.reasoning_content.as_deref() {
                let trimmed = reasoning.trim();
                if !trimmed.is_empty() {
                    return trimmed.to_string();
                }
            }
        }
        String::new()
    }

    pub(super) fn truncate_for_quorum(text: &str, max_chars: Option<usize>) -> String {
        let Some(max_chars) = max_chars else {
            return text.to_string();
        };
        if max_chars == 0 || text.chars().count() <= max_chars {
            return text.to_string();
        }
        let keep = max_chars.saturating_sub(1);
        let mut out = String::with_capacity(max_chars + 24);
        for ch in text.chars().take(keep) {
            out.push(ch);
        }
        out.push('…');
        out
    }

    pub(super) fn build_quorum_synthesis_prompt(
        policy: &QuorumPolicy,
        voter_outcomes: &[QuorumVoterOutcome],
    ) -> String {
        let required_success = Self::required_quorum_success(voter_outcomes.len());
        let mut prompt = String::new();
        prompt.push_str(
            "[QUORUM_SYNTHESIS] You must synthesize across independent model voters.\n\
             Rules:\n\
             1) Use only the voter outputs below as evidence.\n\
             2) Call out disagreements explicitly.\n\
             3) If a voter failed, mark it failed and continue.\n\
             4) Return: (a) strongest case, (b) strongest counter-case, (c) final synthesis with confidence.\n\
             5) Do not claim quorum executed unless voter outputs are present.\n\
             6) Reject placeholder names/fake files/fake metrics; keep only verified claims.\n\
             7) Any file claim in final synthesis must include absolute path + exists_now status or be marked UNPROVEN.\n",
        );
        prompt.push_str(
            "             8) Do not invent commands, tool calls, benchmark results, repository paths, execution evidence, or research citations.\n\
             9) Only cite a command/file/result if it appears verbatim in the voter output or the original user prompt; otherwise mark it UNPROVEN.\n\
             10) If voter evidence is thin or failed, say that directly instead of filling the gap.\n",
        );
        prompt.push_str(&format!(
            "Configured voters: {} | mode={} | enabled={} | required_success={}\n\n",
            policy.voters, policy.mode, policy.enabled, required_success
        ));
        for (idx, voter) in voter_outcomes.iter().enumerate() {
            prompt.push_str(&format!(
                "=== VOTER {} ===\nmodel: {}\nstatus: {}\nduration_ms: {}\nturns: {}\ntool_errors: {}\n",
                idx + 1,
                voter.model,
                voter.status,
                voter.duration_ms,
                voter.total_turns,
                voter.tool_errors
            ));
            if let Some(err) = &voter.error {
                prompt.push_str("error:\n");
                prompt.push_str(err);
                prompt.push('\n');
            }
            prompt.push_str("output:\n");
            prompt.push_str(&voter.output);
            prompt.push_str("\n\n");
        }
        prompt
    }
}
