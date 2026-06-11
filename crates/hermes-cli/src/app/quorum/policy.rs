use crate::alpha_runtime::{QuorumPolicy, load_quorum_policy};
use hermes_core::Message;

use super::super::App;
use super::{QUORUM_DEFAULT_VOTER_PASSES, QUORUM_HINT_PREFIX};

impl App {
    pub(super) fn quorum_voter_retry_limit() -> usize {
        if let Ok(raw) = std::env::var("HERMES_QUORUM_VOTER_MAX_RETRIES") {
            if Self::is_unbounded_token(&raw) {
                return 16;
            }
            if let Some(parsed) = raw.trim().parse::<usize>().ok().filter(|v| *v > 0) {
                return parsed.max(2);
            }
        }
        Self::auth_refresh_retry_limit().max(6)
    }
    pub(super) fn quorum_force_refresh_each_voter() -> bool {
        Self::bool_env("HERMES_QUORUM_FORCE_REFRESH_EACH_VOTER").unwrap_or(false)
    }

    pub(super) fn quorum_toolless_provider_fallback_enabled() -> bool {
        !matches!(
            Self::bool_env("HERMES_QUORUM_TOOLLESS_PROVIDER_FALLBACK"),
            Some(false)
        )
    }

    pub(super) fn quorum_voter_tools_enabled() -> bool {
        !matches!(Self::bool_env("HERMES_QUORUM_VOTER_TOOLS"), Some(false))
    }

    pub(super) fn quorum_synthesis_tools_enabled() -> bool {
        !matches!(Self::bool_env("HERMES_QUORUM_SYNTHESIS_TOOLS"), Some(false))
    }
    pub(crate) fn compose_quorum_messages(
        control_sections: Vec<String>,
        base_messages: Vec<Message>,
        trailing_user_context: Option<String>,
    ) -> Vec<Message> {
        let control_context = control_sections
            .into_iter()
            .map(|section| section.trim().to_string())
            .filter(|section| !section.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");
        let mut merged_system_sections: Vec<String> = Vec::new();
        let mut non_system_messages: Vec<Message> = Vec::new();

        for message in base_messages {
            if message.role == hermes_core::MessageRole::System {
                if let Some(content) = message.content.as_deref().map(str::trim) {
                    if !content.is_empty() {
                        merged_system_sections.push(content.to_string());
                    }
                }
            } else {
                non_system_messages.push(message);
            }
        }

        let mut messages = Vec::new();
        if !merged_system_sections.is_empty() {
            messages.push(hermes_core::Message::system(
                merged_system_sections.join("\n\n"),
            ));
        }
        if !control_context.is_empty() {
            messages.push(hermes_core::Message::user(format!(
                "[QUORUM_CONTROL]\n{}",
                control_context
            )));
        }
        messages.extend(non_system_messages);
        if let Some(context) = trailing_user_context
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
        {
            messages.push(hermes_core::Message::user(context));
        }
        messages
    }

    pub(in crate::app) fn quorum_mode_armed_for_turn(&self) -> Option<QuorumPolicy> {
        let policy = match load_quorum_policy() {
            Ok(policy) => policy,
            Err(err) => {
                Self::emit_lifecycle_event(
                    &self.stream.stream_handle_shared,
                    format!("quorum policy load failed: {}", err),
                );
                return None;
            }
        };
        if !policy.enabled {
            if self.runtime.quorum_armed_once {
                Self::emit_lifecycle_event(
                    &self.stream.stream_handle_shared,
                    "quorum run requested but policy is disabled; run `/quorum on` first",
                );
            }
            return None;
        }
        let has_hint = self.session.messages.iter().any(|message| {
            message.role == hermes_core::MessageRole::System
                && message
                    .content
                    .as_deref()
                    .unwrap_or_default()
                    .starts_with(QUORUM_HINT_PREFIX)
        });
        let has_user_turn = self
            .session
            .messages
            .iter()
            .any(|m| m.role == hermes_core::MessageRole::User);
        if !has_user_turn {
            if self.runtime.quorum_armed_once || has_hint {
                Self::emit_lifecycle_event(
                    &self.stream.stream_handle_shared,
                    "quorum armed but no user turn present yet; waiting for next user prompt",
                );
            }
            return None;
        }
        if !(self.runtime.quorum_armed_once || has_hint) {
            let auto_arm = std::env::var("HERMES_QUORUM_AUTO_ARM")
                .ok()
                .map(|raw| {
                    matches!(
                        raw.trim().to_ascii_lowercase().as_str(),
                        "1" | "true" | "yes" | "on" | "auto"
                    )
                })
                .unwrap_or(false);
            if auto_arm {
                Self::emit_lifecycle_event(
                    &self.stream.stream_handle_shared,
                    "quorum auto-arm enabled via HERMES_QUORUM_AUTO_ARM=1",
                );
                return Some(policy);
            }
            return None;
        }
        Some(policy)
    }

    pub(in crate::app) fn clear_quorum_system_hints_inplace(&mut self) {
        self.session.messages.retain(|message| {
            if message.role != hermes_core::MessageRole::System {
                return true;
            }
            !message
                .content
                .as_deref()
                .unwrap_or_default()
                .starts_with(QUORUM_HINT_PREFIX)
        });
    }

    pub(crate) fn collect_quorum_models(policy: &QuorumPolicy, current_model: &str) -> Vec<String> {
        let mut models: Vec<String> = Vec::new();
        let push_unique = |target: &mut Vec<String>, raw: &str| {
            let candidate = raw.trim();
            if candidate.is_empty() {
                return;
            }
            if target.iter().any(|existing| existing == candidate) {
                return;
            }
            target.push(candidate.to_string());
        };
        for model in &policy.models {
            push_unique(&mut models, model);
        }
        if models.is_empty() {
            push_unique(&mut models, current_model);
        }
        let max_voters = policy.voters.clamp(2, 8);
        if models.len() < max_voters {
            push_unique(&mut models, current_model);
        }
        if models.len() > max_voters {
            models.truncate(max_voters);
        }
        models
    }

    pub(crate) fn quorum_voter_passes() -> usize {
        if let Ok(raw) = std::env::var("HERMES_QUORUM_VOTER_PASSES") {
            if Self::is_unbounded_token(&raw) {
                return 16;
            }
            if let Some(parsed) = raw.trim().parse::<usize>().ok().filter(|v| *v > 0) {
                return parsed.clamp(1, 16);
            }
        }
        QUORUM_DEFAULT_VOTER_PASSES
    }
}
