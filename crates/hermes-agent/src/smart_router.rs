//! SmartRouter facade — groups per-session route-learning state and the frozen
//! primary runtime that were previously scattered as two separate `AgentLoop` fields.
//!
//! All heavy routing logic remains in `route_learning.rs`; this module just
//! provides a cohesive holder so `AgentLoop` has `router: SmartRouter` instead of
//! two unrelated fields.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

use crate::replay::RouteLearningStats;
use crate::smart_model_routing::PrimaryRuntime;

/// Holds the two pieces of mutable routing state for a session.
///
/// Fields are `pub(crate)` so `route_learning.rs` functions that take
/// `agent: &AgentLoop` can reach them as `agent.router.route_learning` /
/// `agent.router.stored_primary_runtime` without adding method indirection.
pub(crate) struct SmartRouter {
    /// Online latency/success stats per (provider, model) key.
    pub(crate) route_learning: Arc<Mutex<HashMap<String, RouteLearningStats>>>,
    /// Frozen primary runtime at session start (Python `_primary_runtime`).
    pub(crate) stored_primary_runtime: PrimaryRuntime,
}

impl SmartRouter {
    pub(crate) fn new(
        route_learning: Arc<Mutex<HashMap<String, RouteLearningStats>>>,
        stored_primary_runtime: PrimaryRuntime,
    ) -> Self {
        Self {
            route_learning,
            stored_primary_runtime,
        }
    }
}
