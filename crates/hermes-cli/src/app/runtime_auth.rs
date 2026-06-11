use hermes_core::AgentError;

use crate::auth::{NousDeviceCodeOptions, login_nous_device_code, save_nous_auth_state};

use super::App;
use super::auth_refresh::{AuthRefreshJob, AuthRefreshOutcome, run_auth_refresh};
use super::provider::provider_api_key_from_env;

impl App {
    pub(super) fn should_force_preflight_auth_refresh(provider: &str) -> bool {
        if let Some(explicit) = Self::bool_env("HERMES_FORCE_RUNTIME_AUTH_REFRESH") {
            return explicit;
        }
        matches!(
            provider,
            "nous" | "qwen-oauth" | "google-gemini-cli" | "gemini-cli" | "gemini-oauth"
        )
    }

    pub(super) fn auto_nous_reauth_enabled() -> bool {
        !matches!(
            std::env::var("HERMES_AUTO_NOUS_REAUTH")
                .ok()
                .as_deref()
                .map(|v| v.trim().to_ascii_lowercase()),
            Some(v) if matches!(v.as_str(), "0" | "false" | "off" | "no")
        )
    }

    pub(super) async fn attempt_interactive_nous_login(&mut self, reason: &str) -> bool {
        if !Self::auto_nous_reauth_enabled() {
            return false;
        }
        Self::emit_lifecycle_event(
            &self.stream.stream_handle_shared,
            format!("Nous OAuth re-auth required ({reason}); launching portal login flow"),
        );
        match login_nous_device_code(NousDeviceCodeOptions::default()).await {
            Ok(state) => match save_nous_auth_state(&state) {
                Ok(path) => {
                    Self::emit_lifecycle_event(
                        &self.stream.stream_handle_shared,
                        format!("Nous OAuth state refreshed: {}", path.display()),
                    );
                    true
                }
                Err(err) => {
                    Self::emit_lifecycle_event(
                        &self.stream.stream_handle_shared,
                        format!("Nous OAuth state save failed: {}", err),
                    );
                    false
                }
            },
            Err(err) => {
                Self::emit_lifecycle_event(
                    &self.stream.stream_handle_shared,
                    format!("Nous OAuth interactive login failed: {}", err),
                );
                false
            }
        }
    }

    pub(super) async fn refresh_runtime_auth(
        &mut self,
        force_refresh: bool,
        nous_login_reason: &str,
    ) -> AuthRefreshOutcome {
        let provider = self.current_runtime_provider();
        let mut outcome = self
            .auth_lane
            .refresh(provider.clone(), force_refresh)
            .await;
        if outcome.nous_login_required
            && self.attempt_interactive_nous_login(nous_login_reason).await
        {
            outcome = run_auth_refresh(AuthRefreshJob {
                provider,
                force_refresh: true,
            })
            .await;
        }
        outcome
    }

    pub(super) async fn refresh_runtime_provider_credentials_if_needed(
        &mut self,
        force_refresh: bool,
    ) {
        let outcome = self
            .refresh_runtime_auth(force_refresh, "credential missing or invalid")
            .await;
        self.apply_auth_refresh_outcome(outcome);
    }

    fn apply_auth_refresh_outcome(&mut self, outcome: AuthRefreshOutcome) {
        let mut rotated = false;
        for (key, value) in outcome.env_updates {
            rotated |= Self::set_env_if_changed(&key, &value);
        }
        if rotated {
            let model = self.model.current_model.clone();
            self.model.switch_active(
                &model,
                &mut self.core,
                &self.session,
                &self.state_root,
                &self.stream,
            );
        }
        for msg in outcome.lifecycle_messages {
            Self::emit_lifecycle_event(&self.stream.stream_handle_shared, msg);
        }
    }

    /// Refresh and verify runtime credentials for the active provider.
    pub async fn verify_runtime_auth(&mut self, force_refresh: bool) -> Result<String, AgentError> {
        let provider = self.current_runtime_provider();
        let before_present = provider_api_key_from_env(&provider).is_some();
        self.refresh_runtime_provider_credentials_if_needed(force_refresh)
            .await;
        let after = provider_api_key_from_env(&provider);
        let after_present = after.is_some();
        let status = if let Some(key) = after {
            format!(
                "present (masked={} chars)",
                key.chars().count().max(1).saturating_sub(8).max(1)
            )
        } else {
            "missing".to_string()
        };
        let refresh_mode = if force_refresh { "forced" } else { "passive" };
        let changed = if before_present == after_present {
            "unchanged"
        } else {
            "updated"
        };
        Ok(format!(
            "Auth verify\nprovider: {}\nmode: {}\ncredential: {}\nstate: {}\nmodel: {}",
            provider, refresh_mode, status, changed, self.model.current_model
        ))
    }

    pub(super) async fn force_auth_refresh_after_error(&mut self) -> bool {
        let outcome = self
            .refresh_runtime_auth(true, "runtime auth refresh failed")
            .await;
        let refreshed = outcome.credential_rotated;
        let notice = if outcome.credential_rotated {
            Some(format!(
                "{} auth auto-refresh succeeded; retrying request.",
                self.current_runtime_provider()
            ))
        } else if outcome.nous_login_required {
            Some("Nous auth auto-refresh failed: login required".to_string())
        } else if let Some(msg) = outcome.lifecycle_messages.last() {
            Some(msg.clone())
        } else {
            None
        };

        self.apply_auth_refresh_outcome(outcome);

        if let Some(text) = notice {
            Self::emit_lifecycle_event(&self.stream.stream_handle_shared, &text);
            if self.stream.stream_handle.is_some() {
                self.push_ui_assistant(text.clone());
            } else {
                println!("{}", text);
            }
        }
        refreshed
    }
}
