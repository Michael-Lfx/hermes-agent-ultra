//! ContextCompressionOrchestrator — module boundary for compression concerns.
//!
//! The three orchestration methods (`context_compression_should_run`,
//! `compress_context`, `auto_compress_if_over_threshold`) remain on `AgentLoop`
//! because they depend on many fields (session_persistence, tool_registry,
//! config, callbacks).  This module exists as a clear namespace boundary and
//! will receive those methods when `AgentLoop` field grouping is complete.
//!
//! The `ContextCompressionOrchestrator` struct is a thin wrapper around
//! `Arc<tokio::sync::Mutex<ContextCompressor>>` that gives the type a
//! distinct identity in the `AgentLoop` struct.

use std::sync::Arc;

use tokio::sync::Mutex;

use crate::compression::ContextCompressor;

/// Wrapper around the context compressor that will eventually own
/// the should_run / compress / auto_compress orchestration methods.
pub(crate) struct ContextCompressionOrchestrator {
    pub(crate) inner: Arc<Mutex<ContextCompressor>>,
}

impl ContextCompressionOrchestrator {
    pub(crate) fn new(inner: Arc<Mutex<ContextCompressor>>) -> Self {
        Self { inner }
    }

    /// Access the inner compressor's threshold without additional arguments.
    pub(crate) async fn threshold_tokens(&self) -> u64 {
        self.inner.lock().await.threshold_tokens()
    }
}
