//! Local user interest (POI) summarization configuration.

use serde::{Deserialize, Serialize};

/// Controls local topic-of-interest extraction and prompt injection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InterestConfig {
    /// Master switch for interest store, prefetch, and session-end ingestion.
    #[serde(default = "default_interest_enabled")]
    pub enabled: bool,

    /// Maximum topics retained after consolidation.
    #[serde(default = "default_interest_max_topics")]
    pub max_topics: u32,

    /// Topics injected into the frozen system prompt at session start.
    #[serde(default = "default_interest_snapshot_top_k")]
    pub snapshot_top_k: u32,

    /// Topics returned from per-turn prefetch.
    #[serde(default = "default_interest_prefetch_top_k")]
    pub prefetch_top_k: u32,

    /// Character budget for the frozen USER INTERESTS block.
    #[serde(default = "default_interest_char_budget_snapshot")]
    pub char_budget_snapshot: usize,

    /// Character budget for prefetch interest lines.
    #[serde(default = "default_interest_char_budget_prefetch")]
    pub char_budget_prefetch: usize,

    /// Extraction mode: `rules` (default), `llm`, or `hybrid`.
    #[serde(default = "default_interest_extract_mode")]
    pub extract_mode: String,

    /// Half-life in days for exponential weight decay.
    #[serde(default = "default_interest_decay_half_life_days")]
    pub decay_half_life_days: f64,

    /// Run LLM topic extraction at session end when mode is `llm` or `hybrid`.
    #[serde(default = "default_interest_llm_on_session_end")]
    pub llm_on_session_end: bool,
}

fn default_interest_enabled() -> bool {
    true
}

fn default_interest_max_topics() -> u32 {
    40
}

fn default_interest_snapshot_top_k() -> u32 {
    5
}

fn default_interest_prefetch_top_k() -> u32 {
    3
}

fn default_interest_char_budget_snapshot() -> usize {
    600
}

fn default_interest_char_budget_prefetch() -> usize {
    400
}

fn default_interest_extract_mode() -> String {
    "hybrid".to_string()
}

fn default_interest_decay_half_life_days() -> f64 {
    30.0
}

fn default_interest_llm_on_session_end() -> bool {
    true
}

impl Default for InterestConfig {
    fn default() -> Self {
        Self {
            enabled: default_interest_enabled(),
            max_topics: default_interest_max_topics(),
            snapshot_top_k: default_interest_snapshot_top_k(),
            prefetch_top_k: default_interest_prefetch_top_k(),
            char_budget_snapshot: default_interest_char_budget_snapshot(),
            char_budget_prefetch: default_interest_char_budget_prefetch(),
            extract_mode: default_interest_extract_mode(),
            decay_half_life_days: default_interest_decay_half_life_days(),
            llm_on_session_end: default_interest_llm_on_session_end(),
        }
    }
}

impl InterestConfig {
    pub fn uses_llm(&self) -> bool {
        matches!(
            self.extract_mode.trim().to_ascii_lowercase().as_str(),
            "llm" | "hybrid"
        )
    }

    pub fn uses_rules(&self) -> bool {
        matches!(
            self.extract_mode.trim().to_ascii_lowercase().as_str(),
            "rules" | "hybrid"
        )
    }
}
