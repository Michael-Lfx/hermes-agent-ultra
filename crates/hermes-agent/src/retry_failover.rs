//! Structured API error classification for retry/failover (parity with `agent/error_classifier.py`).

use hermes_core::Message;

use crate::context::ContextManager;

/// Subset of Python `FailoverReason` used by the Rust retry loop today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailoverReason {
    Auth,
    Billing,
    RateLimit,
    ThinkingSignature,
    Unknown,
}

const BILLING_PATTERNS: &[&str] = &[
    "insufficient credits",
    "insufficient_quota",
    "insufficient balance",
    "credit balance",
    "credits exhausted",
    "credits have been exhausted",
    "no usable credits",
    "top up your credits",
    "payment required",
    "billing hard limit",
    "exceeded your current quota",
    "account is deactivated",
    "plan does not include",
    "out of funds",
    "run out of funds",
    "balance_depleted",
    "model_not_supported_on_free_tier",
    "not available on the free tier",
    "key limit exceeded",
    "spending limit",
];

const RATE_LIMIT_PATTERNS: &[&str] = &[
    "rate limit",
    "rate_limit",
    "too many requests",
    "429",
    "throttled",
    "resource_exhausted",
];

/// Classify an LLM API error string for structured recovery (priority-ordered).
pub fn classify_failover_reason(err: &str) -> FailoverReason {
    let lower = err.to_ascii_lowercase();
    if is_thinking_signature_error(&lower) {
        return FailoverReason::ThinkingSignature;
    }
    if lower.contains("402") || BILLING_PATTERNS.iter().any(|p| lower.contains(p)) {
        return FailoverReason::Billing;
    }
    if RATE_LIMIT_PATTERNS.iter().any(|p| lower.contains(p)) {
        return FailoverReason::RateLimit;
    }
    if lower.contains("401")
        || lower.contains("403")
        || lower.contains("unauthorized")
        || lower.contains("authentication")
    {
        return FailoverReason::Auth;
    }
    FailoverReason::Unknown
}

pub fn is_thinking_signature_error(lower: &str) -> bool {
    lower.contains("400")
        && lower.contains("signature")
        && lower.contains("thinking")
}

/// Strip thinking/reasoning blocks so the next retry sends no signed thinking content.
pub fn strip_thinking_blocks_from_context(ctx: &mut ContextManager) {
    strip_thinking_blocks_from_messages(ctx.get_messages_mut());
}

pub fn strip_thinking_blocks_from_messages(messages: &mut [Message]) {
    for msg in messages {
        msg.reasoning_content = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thinking_signature_detected() {
        let err = "HTTP 400: thinking block signature is invalid for message";
        assert_eq!(
            classify_failover_reason(err),
            FailoverReason::ThinkingSignature
        );
    }

    #[test]
    fn billing_402_detected() {
        assert_eq!(
            classify_failover_reason("HTTP 402 Payment Required"),
            FailoverReason::Billing
        );
    }

    #[test]
    fn billing_pattern_detected() {
        assert_eq!(
            classify_failover_reason("insufficient credits for this model"),
            FailoverReason::Billing
        );
    }

    #[test]
    fn rate_limit_detected() {
        assert_eq!(
            classify_failover_reason("429 rate limit exceeded"),
            FailoverReason::RateLimit
        );
    }
}
