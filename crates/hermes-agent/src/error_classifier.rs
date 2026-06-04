//! Structured API error classification (parity with `agent/error_classifier.py`).

use hermes_core::Message;

use crate::context::ContextManager;

/// Recovery-oriented error reasons used by the retry loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailoverReason {
    Auth,
    Billing,
    RateLimit,
    ThinkingSignature,
    ImageTooLarge,
    ProviderPolicyBlocked,
    LlamaCppGrammarPattern,
    OAuthLongContextBetaForbidden,
    InvalidEncryptedReplay,
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

const IMAGE_TOO_LARGE_PATTERNS: &[&str] = &[
    "image exceeds",
    "image too large",
    "image_too_large",
    "image size exceeds",
    "exceeds 5 mb maximum",
    "exceeds 5mb",
    "maximum: 6291456",
];

const LLAMA_CPP_GRAMMAR_PATTERNS: &[&str] = &[
    "error parsing grammar",
    "unable to generate parser",
    "json-schema-to-grammar",
    "parse: error parsing grammar",
];

const ENCRYPTED_REPLAY_PATTERNS: &[&str] = &[
    "encrypted content",
    "encrypted_content",
    "failed to decrypt",
    "invalid encrypted",
    "cannot replay encrypted",
    "reasoning.encrypted_content",
];

/// Classify an LLM API error string (priority-ordered).
pub fn classify_failover_reason(err: &str) -> FailoverReason {
    classify_failover_reason_with_provider(err, "")
}

pub fn classify_failover_reason_with_provider(err: &str, provider: &str) -> FailoverReason {
    let lower = err.to_ascii_lowercase();
    let provider_lc = provider.to_ascii_lowercase();

    if is_thinking_signature_error(&lower) {
        return FailoverReason::ThinkingSignature;
    }
    if is_invalid_encrypted_replay_error(&lower) {
        return FailoverReason::InvalidEncryptedReplay;
    }
    if is_image_too_large_error(&lower) {
        return FailoverReason::ImageTooLarge;
    }
    if is_llama_cpp_grammar_error(&lower) {
        return FailoverReason::LlamaCppGrammarPattern;
    }
    if provider_lc.contains("anthropic")
        && lower.contains("long context beta is not yet available for this subscription")
    {
        return FailoverReason::OAuthLongContextBetaForbidden;
    }
    if is_provider_policy_blocked(&lower) {
        return FailoverReason::ProviderPolicyBlocked;
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

pub fn is_image_too_large_error(lower: &str) -> bool {
    IMAGE_TOO_LARGE_PATTERNS.iter().any(|p| lower.contains(p))
}

pub fn is_llama_cpp_grammar_error(lower: &str) -> bool {
    lower.contains("400") && LLAMA_CPP_GRAMMAR_PATTERNS.iter().any(|p| lower.contains(p))
}

pub fn is_invalid_encrypted_replay_error(lower: &str) -> bool {
    ENCRYPTED_REPLAY_PATTERNS.iter().any(|p| lower.contains(p))
}

pub fn is_provider_policy_blocked(lower: &str) -> bool {
    lower.contains("guardrail restrictions")
        || lower.contains("matching your data policy")
        || lower.contains("no endpoints available matching")
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

/// Remove provider encrypted-replay blobs from assistant messages (Codex / Responses API).
pub fn strip_invalid_encrypted_replay_from_context(ctx: &mut ContextManager) -> bool {
    strip_invalid_encrypted_replay_from_messages(ctx.get_messages_mut())
}

pub fn strip_invalid_encrypted_replay_from_messages(messages: &mut [Message]) -> bool {
    let mut changed = false;
    for msg in messages.iter_mut() {
        if let Some(ref mut content) = msg.content {
            if content.contains("encrypted_content") || content.contains("gAAAA") {
                *content = "[Encrypted reasoning replay removed for retry.]".to_string();
                changed = true;
            }
        }
        if msg.reasoning_content.is_some() {
            msg.reasoning_content = None;
            changed = true;
        }
    }
    changed
}

/// Best-effort image shrink: strip/replace multimodal image payloads for retry.
pub fn shrink_oversized_images_in_context(ctx: &mut ContextManager) -> bool {
    let mut messages = ctx.get_messages().to_vec();
    let before = messages
        .iter()
        .filter(|m| {
            m.content
                .as_ref()
                .is_some_and(|c| c.contains("data:image"))
        })
        .count();
    crate::vision_message_prepare::strip_images_for_non_vision_model_in_place(&mut messages);
    let after = messages
        .iter()
        .filter(|m| {
            m.content
                .as_ref()
                .is_some_and(|c| c.contains("data:image"))
        })
        .count();
    if before == after {
        return false;
    }
    *ctx.get_messages_mut() = messages;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_too_large_not_context_overflow() {
        let err = "messages.0.content.1.image.source.base64: image exceeds 5 MB maximum";
        assert_eq!(
            classify_failover_reason(err),
            FailoverReason::ImageTooLarge
        );
    }

    #[test]
    fn llama_cpp_grammar_requires_400() {
        let err = "HTTP 400: parse: error parsing grammar: unknown escape";
        assert_eq!(
            classify_failover_reason(err),
            FailoverReason::LlamaCppGrammarPattern
        );
        let err500 = "error parsing grammar";
        assert_ne!(
            classify_failover_reason(err500),
            FailoverReason::LlamaCppGrammarPattern
        );
    }

    #[test]
    fn provider_policy_blocked() {
        let err = "No endpoints available matching your guardrail restrictions and data policy";
        assert_eq!(
            classify_failover_reason(err),
            FailoverReason::ProviderPolicyBlocked
        );
    }

    #[test]
    fn oauth_1m_beta_forbidden() {
        let err = "The long context beta is not yet available for this subscription.";
        assert_eq!(
            classify_failover_reason_with_provider(err, "anthropic"),
            FailoverReason::OAuthLongContextBetaForbidden
        );
    }

    #[test]
    fn encrypted_replay_detected() {
        assert_eq!(
            classify_failover_reason("failed to decrypt encrypted_content replay"),
            FailoverReason::InvalidEncryptedReplay
        );
    }
}
