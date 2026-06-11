//! Provider credential fetch for the auth actor lane (no App / UI coupling).

use hermes_core::AgentError;

use crate::auth::{
    DEFAULT_NOUS_AGENT_KEY_MIN_TTL_SECONDS, NOUS_ACCESS_TOKEN_REFRESH_SKEW_SECONDS,
    QWEN_ACCESS_TOKEN_REFRESH_SKEW_SECONDS, resolve_gemini_oauth_runtime_credentials,
    resolve_nous_runtime_credentials, resolve_qwen_runtime_credentials,
};

#[derive(Debug, Clone, Default)]
pub struct AuthRefreshOutcome {
    pub env_updates: Vec<(String, String)>,
    pub credential_rotated: bool,
    pub lifecycle_messages: Vec<String>,
    pub nous_login_required: bool,
}

pub struct AuthRefreshJob {
    pub provider: String,
    pub force_refresh: bool,
}

pub(crate) fn nous_refresh_contention_error(err: &AgentError) -> bool {
    let text = err.to_string().to_ascii_lowercase();
    text.contains("slow_down")
        || text.contains("too many requests")
        || text.contains("refresh already in progress")
        || text.contains("429")
}

pub(crate) fn auth_error_requires_nous_login(err: &AgentError) -> bool {
    let text = err.to_string().to_ascii_lowercase();
    text.contains("not logged into nous portal")
        || text.contains("re-run `hermes auth nous`")
        || text.contains("stored nous auth state is invalid")
        || text.contains("missing refresh token")
        || text.contains("invalid nous refresh response")
}

fn push_env(outcome: &mut AuthRefreshOutcome, key: &str, value: &str) {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return;
    }
    outcome
        .env_updates
        .push((key.to_string(), trimmed.to_string()));
    outcome.credential_rotated = true;
}

pub async fn run_auth_refresh(job: AuthRefreshJob) -> AuthRefreshOutcome {
    let mut outcome = AuthRefreshOutcome::default();
    match job.provider.as_str() {
        "nous" => refresh_nous(&mut outcome, job.force_refresh).await,
        "qwen-oauth" => refresh_qwen(&mut outcome, job.force_refresh).await,
        "google-gemini-cli" | "gemini-cli" | "gemini-oauth" => {
            refresh_gemini(&mut outcome, job.force_refresh).await;
        }
        _ => {}
    }
    outcome
}

async fn refresh_nous(outcome: &mut AuthRefreshOutcome, force_refresh: bool) {
    match resolve_nous_runtime_credentials(
        force_refresh,
        true,
        NOUS_ACCESS_TOKEN_REFRESH_SKEW_SECONDS,
        DEFAULT_NOUS_AGENT_KEY_MIN_TTL_SECONDS,
    )
    .await
    {
        Ok(creds) => {
            push_env(outcome, "NOUS_API_KEY", &creds.api_key);
            if !creds.base_url.trim().is_empty() {
                push_env(outcome, "NOUS_INFERENCE_BASE_URL", &creds.base_url);
            }
            if outcome.credential_rotated {
                outcome
                    .lifecycle_messages
                    .push("refreshed Nous runtime credential".to_string());
            }
        }
        Err(e) => {
            if force_refresh && nous_refresh_contention_error(&e) {
                match resolve_nous_runtime_credentials(
                    false,
                    true,
                    NOUS_ACCESS_TOKEN_REFRESH_SKEW_SECONDS,
                    DEFAULT_NOUS_AGENT_KEY_MIN_TTL_SECONDS,
                )
                .await
                {
                    Ok(creds) => {
                        push_env(outcome, "NOUS_API_KEY", &creds.api_key);
                        if !creds.base_url.trim().is_empty() {
                            push_env(outcome, "NOUS_INFERENCE_BASE_URL", &creds.base_url);
                        }
                        outcome.lifecycle_messages.push(
                            "Nous refresh busy; reused cached runtime credential".to_string(),
                        );
                    }
                    Err(cache_err) => outcome.lifecycle_messages.push(format!(
                        "warning: Nous cached credential hydration failed after refresh contention ({cache_err})"
                    )),
                }
            }
            if auth_error_requires_nous_login(&e) {
                outcome.nous_login_required = true;
            } else if !outcome.credential_rotated && outcome.lifecycle_messages.is_empty() {
                outcome
                    .lifecycle_messages
                    .push(format!("warning: Nous credential refresh skipped ({e})"));
            }
        }
    }
}

async fn refresh_qwen(outcome: &mut AuthRefreshOutcome, force_refresh: bool) {
    match resolve_qwen_runtime_credentials(
        force_refresh,
        true,
        QWEN_ACCESS_TOKEN_REFRESH_SKEW_SECONDS,
    )
    .await
    {
        Ok(creds) => {
            push_env(outcome, "HERMES_QWEN_OAUTH_API_KEY", &creds.api_key);
            push_env(outcome, "DASHSCOPE_API_KEY", &creds.api_key);
            if !creds.base_url.trim().is_empty() {
                push_env(outcome, "HERMES_QWEN_BASE_URL", &creds.base_url);
            }
            if outcome.credential_rotated {
                outcome
                    .lifecycle_messages
                    .push("refreshed Qwen OAuth runtime credential".to_string());
            }
        }
        Err(e) => outcome
            .lifecycle_messages
            .push(format!("warning: Qwen OAuth refresh skipped ({e})")),
    }
}

async fn refresh_gemini(outcome: &mut AuthRefreshOutcome, force_refresh: bool) {
    match resolve_gemini_oauth_runtime_credentials(force_refresh).await {
        Ok(creds) => {
            push_env(outcome, "HERMES_GEMINI_OAUTH_API_KEY", &creds.api_key);
            push_env(outcome, "GOOGLE_API_KEY", &creds.api_key);
            push_env(outcome, "GEMINI_API_KEY", &creds.api_key);
            if outcome.credential_rotated {
                outcome
                    .lifecycle_messages
                    .push("refreshed Gemini OAuth runtime credential".to_string());
            }
        }
        Err(e) => outcome
            .lifecycle_messages
            .push(format!("warning: Gemini OAuth refresh skipped ({e})")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_error_requires_nous_login_detects_missing_refresh_token() {
        let err = AgentError::AuthFailed("missing refresh token".into());
        assert!(auth_error_requires_nous_login(&err));
    }
}
