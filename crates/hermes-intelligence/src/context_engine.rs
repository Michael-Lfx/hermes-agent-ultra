//! Context engine — pluggable context compression for long conversations.
//!
//! Provides a trait-based interface for compressing conversation context when
//! it approaches the model's context window limit.

use async_trait::async_trait;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

pub use crate::model_metadata::IMAGE_TOKEN_ESTIMATE;
use crate::model_metadata::estimate_tokens_rough;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum ContextError {
    #[error("compression failed: {0}")]
    CompressionFailed(String),

    #[error("context too small to compress ({0} messages)")]
    TooSmall(usize),

    #[error("token estimation error: {0}")]
    TokenEstimation(String),
}

// ---------------------------------------------------------------------------
// ContextEngine trait
// ---------------------------------------------------------------------------

/// Pluggable context compression strategy.
#[async_trait]
pub trait ContextEngine: Send + Sync {
    /// Compress messages to fit within `target_tokens`.
    ///
    /// Returns the compressed messages.  The implementation decides
    /// how to condense — summarization, truncation, importance-based
    /// filtering, or a combination.
    async fn compress(
        &self,
        messages: &[Value],
        target_tokens: u64,
    ) -> Result<Vec<Value>, ContextError>;

    /// Estimate the total token count for a list of messages.
    fn estimate_tokens(&self, messages: &[Value]) -> u64 {
        messages
            .iter()
            .map(|m| {
                let content = m.get("content").and_then(|c| c.as_str()).unwrap_or("");
                estimate_tokens_rough(content)
            })
            .sum()
    }

    /// Name for logging/diagnostics.
    fn name(&self) -> &str;
}

// ---------------------------------------------------------------------------
// DefaultContextEngine — summarization-based compression
// ---------------------------------------------------------------------------

/// Default context engine that compresses by removing older messages
/// and replacing them with a structured summary marker.
pub struct DefaultContextEngine {
    /// Fraction of messages to keep (from the end).
    pub keep_ratio: f64,
    /// Whether to attempt LLM summary generation via optional HTTP endpoint.
    ///
    /// If enabled, the engine reads `HERMES_CONTEXT_SUMMARY_URL` and sends
    /// removed messages to that endpoint. On any failure it falls back to
    /// deterministic heuristic summarization.
    pub use_llm_summary: bool,
    /// Consecutive compaction count where post-compaction tokens still
    /// exceed the target window. When this reaches 2, auto-compaction
    /// is paused to prevent a stuck compaction loop that repeatedly
    /// collapses context without relief and destroys the cache prefix.
    pub consecutive_compacts: AtomicU32,
    /// Whether auto-compaction is currently paused due to stuck detection.
    /// Resets to false when a subsequent compression succeeds.
    pub compact_stuck: AtomicBool,
    /// Token estimation ratio. Default 0.25 = ~4 chars/token.
    /// CJK-heavy text may benefit from 0.5–1.0.
    /// Ported from Reasonix compact.go tokPerChar.
    pub tokens_per_char: f64,
}

impl DefaultContextEngine {
    pub fn new() -> Self {
        Self {
            keep_ratio: 0.33,
            use_llm_summary: false,
            consecutive_compacts: AtomicU32::new(0),
            compact_stuck: AtomicBool::new(false),
            tokens_per_char: 0.25,
>>>>>>> 5a68f1ecc (parity(cache): Phase 2 — absorb Reasonix epsilon improvements)
// ---------------------------------------------------------------------------
// ImportanceBasedEngine — token budget with message scoring
// ---------------------------------------------------------------------------

/// Context engine that assigns importance scores to messages and
/// drops the least important ones to fit within the token budget.
pub struct ImportanceBasedEngine {
    /// System messages always have this score.
    pub system_importance: f64,
    /// Recent user messages get boosted importance.
    pub recency_weight: f64,
    /// Tool results get this base importance.
    pub tool_result_importance: f64,
}

impl ImportanceBasedEngine {
    pub fn new() -> Self {
        Self {
            system_importance: 1.0,
            recency_weight: 0.3,
            tool_result_importance: 0.5,
        }
    }

    fn score_message(&self, msg: &Value, index: usize, total: usize) -> f64 {
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
        let recency = (index as f64) / (total as f64).max(1.0);

        match role {
            "system" => self.system_importance,
            "tool" => self.tool_result_importance + recency * self.recency_weight,
            "assistant" => 0.6 + recency * self.recency_weight,
            "user" => 0.7 + recency * self.recency_weight,
            _ => 0.3 + recency * self.recency_weight,
        }
    }
}

impl Default for ImportanceBasedEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ContextEngine for ImportanceBasedEngine {
    async fn compress(
        &self,
        messages: &[Value],
        target_tokens: u64,
    ) -> Result<Vec<Value>, ContextError> {
        if messages.len() <= 2 {
            return Err(ContextError::TooSmall(messages.len()));
        }

        let total = messages.len();

        // Score each message
        let mut scored: Vec<(usize, f64, u64)> = messages
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let score = self.score_message(m, i, total);
                let tokens =
                    estimate_tokens_rough(m.get("content").and_then(|c| c.as_str()).unwrap_or(""));
                (i, score, tokens)
            })
            .collect();

        // Sort by importance (descending), keep adding until budget filled
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut budget = target_tokens;
        let mut keep_indices: Vec<usize> = Vec::new();

        for (idx, _score, tokens) in &scored {
            if *tokens <= budget {
                keep_indices.push(*idx);
                budget -= tokens;
            }
        }

        // Restore original order
        keep_indices.sort();

        if keep_indices.is_empty() {
            return Err(ContextError::CompressionFailed(
                "Could not fit any messages within target tokens".into(),
            ));
        }

        let dropped = total - keep_indices.len();
        let mut result = Vec::with_capacity(keep_indices.len() + 1);

        if dropped > 0 {
            result.push(serde_json::json!({
                "role": "system",
                "content": format!(
                    "[Context compressed: {} of {} messages retained (dropped {} low-priority messages).]",
                    keep_indices.len(), total, dropped,
                ),
            }));
        }

        for &idx in &keep_indices {
            result.push(messages[idx].clone());
        }

        Ok(result)
    }

    fn name(&self) -> &str {
        "importance"
    }
}

// ---------------------------------------------------------------------------
// Token counting helpers
// ---------------------------------------------------------------------------

fn is_image_content_block(block: &Value) -> bool {
    matches!(
        block.get("type").and_then(|t| t.as_str()),
        Some("image") | Some("image_url") | Some("input_image")
    ) || block.get("image_url").is_some()
}

/// Count tokens for content that may be a string or an array of content blocks.
pub fn count_content_tokens(content: &Value) -> u64 {
    if let Some(s) = content.as_str() {
        return estimate_tokens_rough(s);
    }
    if let Some(arr) = content.as_array() {
        return arr
            .iter()
            .map(|block| {
                if is_image_content_block(block) {
                    return IMAGE_TOKEN_ESTIMATE;
                }
                if let Some(s) = block.as_str() {
                    return estimate_tokens_rough(s);
                }
                if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                    estimate_tokens_rough(text)
                } else {
                    estimate_tokens_rough(&block.to_string())
                }
            })
            .sum();
    }
    0
}

/// Estimate total tokens for a full message including role overhead.
pub fn estimate_message_tokens(msg: &Value) -> u64 {
    let role_overhead: u64 = 4; // ~4 tokens for role metadata
    let content_tokens = msg.get("content").map(count_content_tokens).unwrap_or(0);
    let tool_calls_tokens = msg
        .get("tool_calls")
        .and_then(|tc| tc.as_array())
        .map(|calls| {
            calls
                .iter()
                .map(|c| estimate_tokens_rough(&c.to_string()))
                .sum::<u64>()
        })
        .unwrap_or(0);

    role_overhead + content_tokens + tool_calls_tokens
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_messages(count: usize) -> Vec<Value> {
        (0..count)
            .map(|i| {
                json!({
                    "role": if i % 2 == 0 { "user" } else { "assistant" },
                    "content": format!("Message {} with some content to make it longer for token estimation purposes and testing", i),
                })
            })
            .collect()
    }

    #[tokio::test]
    async fn test_default_engine_compress() {
        let engine = DefaultContextEngine::new();
        // foldEconomics requires >= 400 tokens in the foldable region.
        // 40 messages × ~20 tokens each = ~800 tokens → passes threshold.
        let messages = make_messages(40);
        let result = engine.compress(&messages, 200).await.unwrap();
        assert!(result.len() < 40);
        assert!(
            result[0]
                .get("content")
                .unwrap()
                .as_str()
                .unwrap()
                .contains("compressed")
        );
    }

    #[tokio::test]
    async fn test_default_engine_no_compress_needed() {
        let engine = DefaultContextEngine::new();
        let messages = make_messages(3);
        let result = engine.compress(&messages, 10_000).await.unwrap();
        assert_eq!(result.len(), 3); // No compression needed
    }

    #[tokio::test]
    async fn test_default_engine_too_small() {
        let engine = DefaultContextEngine::new();
        let messages = make_messages(2);
        assert!(engine.compress(&messages, 10).await.is_err());
    }

    #[tokio::test]
    async fn test_importance_engine() {
        let engine = ImportanceBasedEngine::new();
        let messages = make_messages(20);
        let result = engine.compress(&messages, 200).await.unwrap();
        assert!(result.len() < 20);
    }

    #[test]
    fn test_count_content_tokens() {
        assert!(count_content_tokens(&json!("hello world")) > 0);
        assert!(
            count_content_tokens(&json!([
                {"type": "text", "text": "hello"},
                {"type": "text", "text": "world"},
            ])) > 0
        );
    }

    #[test]
    fn image_blocks_charge_fixed_budget_without_counting_base64_payload() {
        let huge_data = format!("data:image/png;base64,{}", "a".repeat(100_000));
        let content = json!([
            {"type": "text", "text": "look at this"},
            {"type": "image_url", "image_url": {"url": huge_data}},
            {"type": "input_image", "image_url": "https://example.com/a.png"},
            {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "b".repeat(100_000)}}
        ]);
        let text_only = estimate_tokens_rough("look at this");
        assert_eq!(
            count_content_tokens(&content),
            text_only + IMAGE_TOKEN_ESTIMATE * 3
        );
    }

    #[test]
    fn mixed_content_blocks_count_text_and_bare_strings() {
        let content = json!([
            {"type": "text", "text": "hello"},
            "world",
            {"type": "tool_result", "value": "ok"}
        ]);
        let estimate = count_content_tokens(&content);
        assert!(estimate >= estimate_tokens_rough("hello") + estimate_tokens_rough("world"));
    }

    #[test]
    fn test_estimate_message_tokens() {
        let msg = json!({"role": "user", "content": "hello world"});
        assert!(estimate_message_tokens(&msg) > 0);
    }
}
