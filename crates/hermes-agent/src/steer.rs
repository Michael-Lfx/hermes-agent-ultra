//! Mid-run `/steer` injection (Python `AIAgent.steer` / pre-API drain parity).
//!
//! Steer text is appended to the last tool result with a `User guidance:` marker so
//! message-role alternation and prompt-cache layout stay intact.

use std::sync::{Arc, Mutex};

use hermes_core::{Message, MessageRole};

/// Marker appended to tool output so the model attributes text to the user.
pub const STEER_GUIDANCE_MARKER: &str = "\n\nUser guidance: ";

/// Thread-safe pending steer slot (`_pending_steer` + lock in Python).
#[derive(Debug, Clone, Default)]
pub struct PendingSteer(Arc<Mutex<Option<String>>>);

impl PendingSteer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Accept non-empty steer text; concatenate multiple calls with newlines.
    pub fn steer(&self, text: &str) -> bool {
        let cleaned = text.trim();
        if cleaned.is_empty() {
            return false;
        }
        let mut guard = self.0.lock().expect("pending steer lock poisoned");
        if let Some(ref existing) = *guard {
            *guard = Some(format!("{existing}\n{cleaned}"));
        } else {
            *guard = Some(cleaned.to_string());
        }
        true
    }

    pub fn drain(&self) -> Option<String> {
        self.0.lock().ok()?.take()
    }

    pub fn clear(&self) {
        if let Ok(mut guard) = self.0.lock() {
            *guard = None;
        }
    }

    fn restash(&self, text: String) {
        let mut guard = self.0.lock().expect("pending steer lock poisoned");
        if let Some(ref existing) = *guard {
            *guard = Some(format!("{existing}\n{text}"));
        } else {
            *guard = Some(text);
        }
    }

    /// Pre-API-call drain (`conversation_loop.py` before `api_messages`).
    pub fn drain_pre_api_into_messages(&self, messages: &mut [Message]) {
        let Some(steer_text) = self.drain() else {
            return;
        };
        if inject_steer_into_last_tool(messages, &steer_text) {
            tracing::debug!("Pre-API-call steer drain: injected into last tool result");
        } else {
            self.restash(steer_text);
        }
    }

    /// Post-tool-batch drain (`apply_pending_steer_to_tool_results`).
    pub fn apply_to_tool_results(&self, messages: &mut [Message], num_tool_msgs: usize) {
        if num_tool_msgs == 0 || messages.is_empty() {
            return;
        }
        let Some(steer_text) = self.drain() else {
            return;
        };
        let min_idx = messages.len().saturating_sub(num_tool_msgs + 1);
        let target_idx = (min_idx..messages.len())
            .rev()
            .find(|&j| messages[j].role == MessageRole::Tool);
        if let Some(idx) = target_idx {
            append_steer_to_tool_message(&mut messages[idx], &steer_text);
        } else {
            self.restash(steer_text);
        }
    }
}

fn append_steer_to_tool_message(msg: &mut Message, steer_text: &str) {
    let marker = format!("{STEER_GUIDANCE_MARKER}{steer_text}");
    if let Some(ref mut content) = msg.content {
        content.push_str(&marker);
    } else {
        msg.content = Some(marker);
    }
}

fn inject_steer_into_last_tool(messages: &mut [Message], steer_text: &str) -> bool {
    if let Some(msg) = messages
        .iter_mut()
        .rev()
        .find(|m| m.role == MessageRole::Tool)
    {
        append_steer_to_tool_message(msg, steer_text);
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use hermes_core::Message;

    fn bare_pending() -> PendingSteer {
        PendingSteer::new()
    }

    #[test]
    fn steer_accepts_and_concatenates() {
        let p = bare_pending();
        assert!(p.steer("go ahead and check the logs"));
        assert!(p.steer("second note"));
        assert_eq!(
            p.drain().as_deref(),
            Some("go ahead and check the logs\nsecond note")
        );
        assert!(p.drain().is_none());
    }

    #[test]
    fn steer_rejects_empty() {
        let p = bare_pending();
        assert!(!p.steer(""));
        assert!(!p.steer("   \n\t  "));
        assert!(p.drain().is_none());
    }

    #[test]
    fn apply_appends_to_last_tool_result() {
        let p = bare_pending();
        p.steer("please also check auth.log");
        let mut messages = vec![
            Message::user("what's in /var/log?"),
            Message::assistant_with_tool_calls(None, vec![]),
            Message::tool_result("a", "ls output A"),
            Message::tool_result("b", "ls output B"),
        ];
        p.apply_to_tool_results(&mut messages, 2);
        assert_eq!(messages[2].content.as_deref(), Some("ls output A"));
        let last = messages[3].content.as_deref().unwrap_or("");
        assert!(last.contains("ls output B"));
        assert!(last.contains("User guidance:"));
        assert!(last.contains("please also check auth.log"));
        assert!(p.drain().is_none());
    }

    #[test]
    fn apply_no_op_when_num_tool_msgs_zero() {
        let p = bare_pending();
        p.steer("steer");
        let mut messages = vec![Message::user("hi")];
        p.apply_to_tool_results(&mut messages, 0);
        assert_eq!(p.drain().as_deref(), Some("steer"));
    }

    #[test]
    fn pre_api_drain_injects_into_last_tool_result() {
        let p = bare_pending();
        let mut messages = vec![
            Message::user("do something"),
            Message::assistant_with_tool_calls(
                Some("ok".into()),
                vec![hermes_core::ToolCall {
                    id: "tc1".into(),
                    function: hermes_core::FunctionCall {
                        name: "terminal".into(),
                        arguments: "{}".into(),
                    },
                    extra_content: None,
                }],
            ),
            Message::tool_result("tc1", "output here"),
        ];
        p.steer("focus on error handling");
        p.drain_pre_api_into_messages(&mut messages);
        let last = messages[2].content.as_deref().unwrap_or("");
        assert!(last.contains("User guidance:"));
        assert!(last.contains("focus on error handling"));
        assert!(p.drain().is_none());
    }

    #[test]
    fn pre_api_drain_restashes_when_no_tool_message() {
        let p = bare_pending();
        let mut messages = vec![Message::user("hello")];
        p.steer("early steer");
        p.drain_pre_api_into_messages(&mut messages);
        assert_eq!(messages[0].content.as_deref(), Some("hello"));
        assert_eq!(p.drain().as_deref(), Some("early steer"));
    }

    /// Mirrors CLI `promoted_steer_command_sets_and_clears_instruction` payload.
    #[test]
    fn cli_promoted_steer_instruction_accepted() {
        let p = bare_pending();
        assert!(p.steer("focus on repo map"));
        assert_eq!(p.drain().as_deref(), Some("focus on repo map"));
    }

    #[test]
    fn clear_drops_pending_steer() {
        let p = bare_pending();
        p.steer("will be dropped");
        p.clear();
        assert!(p.drain().is_none());
    }
}
