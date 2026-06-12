//! Tool-loop guardrails — parity with Python `agent.tool_guardrails`.

use serde_json::Value;

/// Decision from [`ToolGuardrailController::before_call`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardrailDecision {
    Allow,
    Block(String),
    Halt(String),
}

/// Per-turn tool policy controller.
#[derive(Debug, Default)]
pub struct ToolGuardrailController {
    observations: Vec<String>,
    halt_reason: Option<String>,
    repeated_errors: std::collections::HashMap<String, u32>,
}

impl ToolGuardrailController {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn before_call(&mut self, tool_name: &str, _args: &Value) -> GuardrailDecision {
        if self.halt_reason.is_some() {
            return GuardrailDecision::Halt(
                self.halt_reason
                    .clone()
                    .unwrap_or_else(|| "guardrail halt".into()),
            );
        }
        if tool_name.is_empty() {
            return GuardrailDecision::Block("empty tool name".into());
        }
        GuardrailDecision::Allow
    }

    pub fn after_call(&mut self, tool_name: &str, is_error: bool, preview: &str) {
        if is_error {
            let count = self
                .repeated_errors
                .entry(tool_name.to_string())
                .or_insert(0);
            *count += 1;
            self.observations.push(format!(
                "Tool '{}' failed ({}x this turn): {}",
                tool_name,
                count,
                preview.chars().take(180).collect::<String>()
            ));
            if *count >= 5 {
                self.halt_reason = Some(format!(
                    "Repeated failures calling '{}' ({} errors this turn)",
                    tool_name, count
                ));
            }
        }
    }

    pub fn halt_decision(&self) -> Option<&str> {
        self.halt_reason.as_deref()
    }

    pub fn observations(&self) -> &[String] {
        &self.observations
    }

    pub fn append_observation(&mut self, text: impl Into<String>) {
        self.observations.push(text.into());
    }
}
