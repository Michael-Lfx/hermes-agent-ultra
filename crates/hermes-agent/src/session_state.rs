//! Session-scoped usage counters and context-engine transition (Python `reset_session_state`).

use std::collections::HashMap;

use hermes_core::Message;
use serde_json::Value;
use tracing::debug;

use crate::agent_loop::{AgentConfig, AgentLoop};
use crate::compression::ContextCompressor;

/// Cumulative token/cost counters for the active session (Python `AIAgent.session_*`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SessionUsageMetrics {
    pub total_tokens: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub reasoning_tokens: u64,
    pub api_calls: u32,
    pub estimated_cost_usd: f64,
    pub cost_status: String,
    pub cost_source: String,
}

impl SessionUsageMetrics {
    pub fn reset(&mut self) {
        self.total_tokens = 0;
        self.input_tokens = 0;
        self.output_tokens = 0;
        self.prompt_tokens = 0;
        self.completion_tokens = 0;
        self.cache_read_tokens = 0;
        self.cache_write_tokens = 0;
        self.reasoning_tokens = 0;
        self.api_calls = 0;
        self.estimated_cost_usd = 0.0;
        self.cost_status = "unknown".to_string();
        self.cost_source = "none".to_string();
    }
}

/// Optional context-engine session hooks (Python `hasattr` checks on `context_compressor`).
pub trait ContextEngineHost {
    fn context_length(&self) -> Option<u64> {
        None
    }

    fn on_session_end(&mut self, _old_session_id: &str, _previous_messages: &[Message]) {}

    fn on_session_reset(&mut self) {}

    fn on_session_start(&mut self, _session_id: &str, _context: &HashMap<String, Value>) {}

    fn carry_over_new_session_context(&mut self, _old_session_id: &str, _new_session_id: &str) {}
}

impl ContextEngineHost for ContextCompressor {
    fn context_length(&self) -> Option<u64> {
        Some(self.config_context_length())
    }

    fn on_session_reset(&mut self) {
        self.reset_session_state();
    }
}

/// Notify the active context engine about a host session transition.
pub(crate) fn transition_context_engine_session(
    engine: &mut dyn ContextEngineHost,
    config: &AgentConfig,
    old_session_id: Option<&str>,
    new_session_id: Option<&str>,
    previous_messages: Option<&[Message]>,
    carry_over_context: bool,
    reset_engine: bool,
    extra_context: HashMap<String, Value>,
) {
    if let (Some(old_sid), Some(msgs)) = (old_session_id, previous_messages) {
        if !old_sid.is_empty() {
            engine.on_session_end(old_sid, msgs);
        }
    }

    if reset_engine {
        engine.on_session_reset();
    }

    let should_start = old_session_id.is_some()
        || previous_messages.is_some()
        || carry_over_context
        || !extra_context.is_empty();

    let target_session_id = new_session_id
        .or(config.session_id.as_deref())
        .unwrap_or("")
        .trim()
        .to_string();

    if should_start && !target_session_id.is_empty() {
        let platform = config
            .platform
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| {
                std::env::var("HERMES_SESSION_SOURCE")
                    .ok()
                    .filter(|s| !s.is_empty())
            })
            .unwrap_or_else(|| "cli".to_string());

        let mut start_context: HashMap<String, Value> = HashMap::from([
            (
                "carry_over_context".to_string(),
                Value::Bool(carry_over_context),
            ),
            (
                "platform".to_string(),
                Value::String(platform),
            ),
            (
                "model".to_string(),
                Value::String(config.model.clone()),
            ),
        ]);
        if let Some(old) = old_session_id.filter(|s| !s.is_empty()) {
            start_context.insert("old_session_id".to_string(), Value::String(old.to_string()));
        }
        if let Some(ctx_len) = engine.context_length() {
            start_context.insert("context_length".to_string(), Value::from(ctx_len));
        }
        if let Some(ref conv_id) = config.gateway_session_key {
            if !conv_id.is_empty() {
                start_context.insert("conversation_id".to_string(), Value::String(conv_id.clone()));
            }
        }
        start_context.extend(extra_context);
        start_context.retain(|_, v| {
            !v.is_null()
                && !(v.is_string() && v.as_str().is_some_and(|s| s.is_empty()))
        });
        engine.on_session_start(&target_session_id, &start_context);
    }

    if carry_over_context {
        if let (Some(old), Some(new)) = (old_session_id, Some(target_session_id.as_str())) {
            if !old.is_empty() && !new.is_empty() {
                engine.carry_over_new_session_context(old, new);
            }
        }
    }
}

impl AgentLoop {
    /// Reset session-scoped token counters and context-engine state (Python `reset_session_state`).
    pub fn reset_session_state(
        &self,
        previous_messages: Option<&[Message]>,
        old_session_id: Option<&str>,
        carry_over_context: bool,
    ) {
        if let Ok(mut metrics) = self.session_usage.lock() {
            metrics.reset();
        }
        if let Ok(mut counters) = self.evolution_counters.lock() {
            counters.user_turn_count = 0;
        }

        let config = self.config();
        let new_session_id = config.session_id.clone();
        if let Ok(mut compressor) = self.context_compressor.try_lock() {
            transition_context_engine_session(
                &mut *compressor,
                &config,
                old_session_id,
                new_session_id.as_deref(),
                previous_messages,
                carry_over_context,
                true,
                HashMap::new(),
            );
        } else {
            debug!("context engine transition skipped: compressor lock busy");
        }
    }

    pub fn session_usage_metrics(&self) -> SessionUsageMetrics {
        self.session_usage
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct RecordingEngine {
        events: Vec<String>,
        context_length: u64,
    }

    impl ContextEngineHost for RecordingEngine {
        fn context_length(&self) -> Option<u64> {
            Some(self.context_length)
        }

        fn on_session_end(&mut self, _: &str, _: &[Message]) {
            self.events.push("on_session_end".into());
        }

        fn on_session_reset(&mut self) {
            self.events.push("on_session_reset".into());
        }

        fn on_session_start(&mut self, _: &str, _: &HashMap<String, Value>) {
            self.events.push("on_session_start".into());
        }

        fn carry_over_new_session_context(&mut self, _: &str, _: &str) {
            self.events.push("carry_over".into());
        }
    }

    #[test]
    fn transition_runs_full_lifecycle_in_order() {
        let mut engine = RecordingEngine {
            events: Vec::new(),
            context_length: 200_000,
        };
        let config = AgentConfig::default();
        transition_context_engine_session(
            &mut engine,
            &config,
            Some("old-sid"),
            Some("new-sid"),
            Some(&[Message::user("hi")]),
            true,
            true,
            HashMap::new(),
        );
        assert_eq!(
            engine.events,
            vec![
                "on_session_end",
                "on_session_reset",
                "on_session_start",
                "carry_over"
            ]
        );
    }

    #[test]
    fn transition_default_call_only_resets() {
        let mut engine = RecordingEngine {
            events: Vec::new(),
            context_length: 100_000,
        };
        let config = AgentConfig::default();
        transition_context_engine_session(
            &mut engine,
            &config,
            None,
            None,
            None,
            false,
            true,
            HashMap::new(),
        );
        assert_eq!(engine.events, vec!["on_session_reset"]);
    }

    #[test]
    fn transition_passes_conversation_id_from_gateway_session_key() {
        struct CaptureEngine {
            captured: Option<HashMap<String, Value>>,
        }
        impl ContextEngineHost for CaptureEngine {
            fn context_length(&self) -> Option<u64> {
                Some(200_000)
            }
            fn on_session_start(&mut self, _: &str, ctx: &HashMap<String, Value>) {
                self.captured = Some(ctx.clone());
            }
        }
        let mut engine = CaptureEngine { captured: None };
        let config = AgentConfig {
            gateway_session_key: Some("agent:main:telegram:dm:42".into()),
            platform: Some("telegram".into()),
            ..AgentConfig::default()
        };
        transition_context_engine_session(
            &mut engine,
            &config,
            Some("old-sid"),
            Some("new-sid"),
            Some(&[Message::user("hi")]),
            false,
            true,
            HashMap::new(),
        );
        let ctx = engine.captured.expect("on_session_start");
        assert_eq!(
            ctx.get("conversation_id").and_then(|v| v.as_str()),
            Some("agent:main:telegram:dm:42")
        );
        assert_eq!(
            ctx.get("old_session_id").and_then(|v| v.as_str()),
            Some("old-sid")
        );
        assert_eq!(ctx.get("platform").and_then(|v| v.as_str()), Some("telegram"));
    }

}
