//! Re-exports from [`crate::error_classifier`] (legacy module path).

pub use crate::error_classifier::{
    classify_failover_reason, classify_failover_reason_with_provider, is_thinking_signature_error,
    strip_thinking_blocks_from_context, strip_thinking_blocks_from_messages, FailoverReason,
};
