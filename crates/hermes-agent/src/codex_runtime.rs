//! Codex app-server runtime — parity with `agent/codex_runtime.py`.
//!
//! When [`crate::smart_model_routing::ApiMode::CodexAppServer`] is active,
//! [`crate::conversation_loop::AgentLoop::run_with_message_prelude`] bypasses
//! the Hermes tool loop and delegates the turn here (Python `run_conversation`
//! early return at ~752).

use hermes_core::Message;

use crate::agent_loop::{AgentLoop, LoopExit};
use crate::context::ContextManager;

impl AgentLoop {
    /// True when the active runtime is the codex app-server path.
    pub(crate) fn api_mode_is_codex_app_server(&self) -> bool {
        use crate::smart_model_routing::ApiMode;
        matches!(
            self.primary_runtime_snapshot().api_mode,
            ApiMode::CodexAppServer
        )
    }

    /// Drive one user turn through the codex app-server path (Python `run_codex_app_server_turn`).
    ///
    /// The subprocess JSON-RPC client (`agent/transports/codex_app_server_session.py`) is not
    /// ported yet; failures match the Python crash path shape (assistant error text, `partial`).
    pub(crate) async fn run_codex_app_server_turn(
        &self,
        user_message: &str,
        mut messages: Vec<Message>,
        _should_review_memory: bool,
        session_started_hooks_fired: bool,
    ) -> hermes_core::AgentResult {
        let transport_err = std::env::var("HERMES_CODEX_APP_SERVER_CMD")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map(|_| {
                "HERMES_CODEX_APP_SERVER_CMD is set but the Rust CodexAppServerSession adapter is not implemented yet.".to_string()
            })
            .unwrap_or_else(|| {
                "Codex app-server subprocess transport is not wired in Rust yet. \
                 Use `/codex-runtime auto` or keep `codex_responses` until the session adapter is ported."
                    .to_string()
            });

        tracing::warn!(
            user_message_len = user_message.len(),
            error = %transport_err,
            "codex app-server turn failed"
        );

        let assistant_text = format!(
            "Codex app-server turn failed: {transport_err} \
             Fall back to default runtime with `/codex-runtime auto`."
        );
        messages.push(Message::assistant(&assistant_text));

        let mut ctx = ContextManager::for_model(self.active_model().as_str());
        for msg in &messages {
            ctx.add_message(msg.clone());
        }

        self.turn_end_plugin_hooks(
            ctx.get_messages(),
            false,
            false,
            0,
            session_started_hooks_fired,
        );

        self.seal_loop_result(
            &ctx,
            None,
            LoopExit {
                turn_exit_reason: "codex_app_server_transport_unavailable",
                api_calls: 0,
                failed: false,
                partial: true,
                finished_naturally: false,
                interrupted: false,
            },
            0,
            &[],
            None,
            0.0,
            session_started_hooks_fired,
        )
    }
}
