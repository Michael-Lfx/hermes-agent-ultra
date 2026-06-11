mod artifacts;
mod catalog;
mod fanout;
mod policy;
mod prompts;
mod scoring;

use serde::Serialize;

pub(crate) const QUORUM_HINT_PREFIX: &str = "[QUORUM_MODE] ";
pub(super) const QUORUM_MAX_VOTER_OUTPUT_CHARS: usize = 120_000;
pub(crate) const QUORUM_DEFAULT_VOTER_PASSES: usize = 6;
pub(super) const QUORUM_AGENT_CONTRACT_DEFAULT_PATH: &str =
    "/Users/sheawinkler/Documents/Projects/hermes-agent-ultra/docs/QUORUM_AGENTS.md";

#[derive(Debug, Clone, Serialize)]
pub(super) struct QuorumVoterOutcome {
    model: String,
    status: String,
    duration_ms: u64,
    total_turns: u32,
    tool_errors: usize,
    output: String,
    error: Option<String>,
}
