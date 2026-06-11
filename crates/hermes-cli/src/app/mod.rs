//! Application state management for the interactive CLI.
//!
//! The `App` struct owns the configuration, agent loop, tool registry,
//! and conversation message history. It coordinates input handling,
//! slash commands, and session management.

use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use hermes_agent::sub_agent_orchestrator::SubAgentOrchestrator;
use hermes_agent::{
    AgentCallbacks, AgentLoop, InterruptController, RunConversationParams, SessionPersistence,
    split_messages_for_run_conversation,
};
use hermes_config::{GatewayConfig, hermes_home as hermes_home_dir, load_config, state_dir};
use hermes_core::AgentError;
use hermes_core::ToolSchema;
use hermes_cron::cron_scheduler_for_data_dir;
use hermes_skills::{FileSkillStore, SkillManager};
use hermes_tools::ToolRegistry;
use hermes_tools::tools::messaging::MessagingSessionContext;

use hermes_acp_server::server::AcpPipeServer;

use crate::alpha_runtime::{
    ObjectiveContract, canonical_objective_behavior_mode, load_objective_contract,
    objective_lifecycle_is_active,
};
use crate::auth::{
    DEFAULT_NOUS_AGENT_KEY_MIN_TTL_SECONDS, NOUS_ACCESS_TOKEN_REFRESH_SKEW_SECONDS,
    NousDeviceCodeOptions, NousRuntimeCredentials, QWEN_ACCESS_TOKEN_REFRESH_SKEW_SECONDS,
    login_nous_device_code, resolve_gemini_oauth_runtime_credentials,
    resolve_nous_runtime_credentials, resolve_qwen_runtime_credentials, save_nous_auth_state,
};
use crate::cli::Cli;
use crate::commands::recover_queued_background_jobs;
use crate::model_switch::provider_model_ids;
use crate::runtime_tool_wiring::{wire_cron_scheduler_backend, wire_stdio_clarify_backend};
use crate::terminal_backend::build_terminal_backend;
use crate::tui::StreamHandle;

mod pet;
mod provider;
mod quorum;
mod session_snapshot;
mod snapshot_policy;

#[cfg(test)]
mod tests;

pub use pet::{PetDock, PetSettings};
pub use provider::{
    async_tool_dispatch_for, bridge_tool_registry, build_agent_config, build_provider,
    provider_api_key_from_env,
};

use pet::{load_pet_settings, persist_pet_settings};
use provider::{
    apply_cli_runtime_overrides, default_mouse_enabled, default_rtk_raw_mode,
    normalize_runtime_provider_name, resolve_provider_and_model, resolve_startup_model,
    sync_runtime_model_env,
};
use snapshot_policy::SnapshotPersistGate;

const RUNTIME_REFORMULATION_PROMPT_PREVIEW_CHARS: usize = 1_600;

/// Top-level application state for an interactive Hermes session.
pub struct App {
    /// Resolved Hermes state root (respects `-C/--config-dir`).
    pub state_root: PathBuf,

    /// Loaded gateway configuration.
    pub config: Arc<GatewayConfig>,

    /// The agent loop engine.
    pub agent: Arc<AgentLoop>,

    /// The tool registry (shared with the agent).
    pub tool_registry: Arc<ToolRegistry>,

    /// Active tool schemas exposed to the model for this runtime.
    pub tool_schemas: Vec<ToolSchema>,

    /// Conversation messages for the current session.
    pub messages: Vec<hermes_core::Message>,

    /// UI-only transcript messages (slash commands, local notices), anchored
    /// to a conversation index so they do not pollute model context.
    pub ui_messages: Vec<UiTranscriptMessage>,

    /// Unique identifier for the current session.
    pub session_id: String,

    /// Whether the application loop is still running.
    pub running: bool,

    /// Currently active model identifier (e.g. "openai:gpt-4o").
    pub current_model: String,

    /// Currently active personality name.
    pub current_personality: Option<String>,

    /// History of user inputs for recall.
    pub input_history: Vec<String>,

    /// Index into input_history for up/down arrow navigation.
    pub history_index: usize,

    /// Interrupt controller for stopping agent execution.
    pub interrupt_controller: InterruptController,

    /// Optional TUI streaming sink for incremental chunks.
    pub stream_handle: Option<StreamHandle>,
    /// Shared streaming sink used by agent callbacks for progress events.
    stream_handle_shared: Arc<StdMutex<Option<StreamHandle>>>,
    /// Whether TUI mouse events are enabled.
    pub mouse_enabled: bool,
    /// Pending skin/theme slug to apply in the TUI loop.
    pub pending_theme: Option<String>,
    /// Optional image path hint injected into the next user prompt.
    pub pending_image_hint: Option<String>,
    /// Optional durable objective for the current interactive session.
    pub session_objective: Option<String>,
    /// User text staged back into the composer by commands such as `/undo`.
    pending_input_prefill: Option<String>,
    /// One-shot quorum arm state set by `/quorum run`.
    pub quorum_armed_once: bool,
    /// Animated companion pet settings.
    pub pet_settings: PetSettings,
    /// Background ACP Pipe Server (started via /acp_server).
    pub acp_server: Option<Arc<AcpPipeServer>>,
    /// Accumulated ACP server lifecycle events (connect, prompt, disconnect).
    /// Printed when the user interacts with /acp_server commands.
    pub acp_event_buffer: Option<Arc<std::sync::Mutex<Vec<String>>>>,
    /// Coalesces autosave writes between agent turns.
    pub(super) snapshot_gate: SnapshotPersistGate,
}

impl std::fmt::Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("App")
            .field("state_root", &self.state_root)
            .field("session_id", &self.session_id)
            .field("running", &self.running)
            .field("current_model", &self.current_model)
            .field("current_personality", &self.current_personality)
            .field("history_index", &self.history_index)
            .field("mouse_enabled", &self.mouse_enabled)
            .field("pending_theme", &self.pending_theme)
            .field("pending_image_hint", &self.pending_image_hint)
            .field("session_objective", &self.session_objective)
            .field("pending_input_prefill", &self.pending_input_prefill)
            .field("quorum_armed_once", &self.quorum_armed_once)
            .field("pet_settings", &self.pet_settings)
            .finish_non_exhaustive()
    }
}

impl Clone for App {
    fn clone(&self) -> Self {
        Self {
            state_root: self.state_root.clone(),
            config: self.config.clone(),
            agent: self.agent.clone(),
            tool_registry: self.tool_registry.clone(),
            tool_schemas: self.tool_schemas.clone(),
            messages: self.messages.clone(),
            ui_messages: self.ui_messages.clone(),
            session_id: self.session_id.clone(),
            running: self.running,
            current_model: self.current_model.clone(),
            current_personality: self.current_personality.clone(),
            input_history: self.input_history.clone(),
            history_index: self.history_index,
            interrupt_controller: self.interrupt_controller.clone(),
            stream_handle: self.stream_handle.clone(),
            stream_handle_shared: self.stream_handle_shared.clone(),
            mouse_enabled: self.mouse_enabled,
            pending_theme: self.pending_theme.clone(),
            pending_image_hint: self.pending_image_hint.clone(),
            session_objective: self.session_objective.clone(),
            pending_input_prefill: self.pending_input_prefill.clone(),
            quorum_armed_once: self.quorum_armed_once,
            pet_settings: self.pet_settings.clone(),
            acp_server: self.acp_server.clone(),
            acp_event_buffer: self.acp_event_buffer.clone(),
            snapshot_gate: self.snapshot_gate.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// SessionInfo (for serialization)
// ---------------------------------------------------------------------------

/// Serializable snapshot of a session (for save/restore).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub model: String,
    pub personality: Option<String>,
    pub message_count: usize,
    pub created_at: String,
}

/// A TUI-local transcript message anchored to a conversation position.
#[derive(Debug, Clone)]
pub struct UiTranscriptMessage {
    /// Conversation message count at insertion time.
    pub insert_at: usize,
    /// Rendered message payload.
    pub message: hermes_core::Message,
}

// ---------------------------------------------------------------------------
// App implementation
// ---------------------------------------------------------------------------

impl App {
    const SESSION_OBJECTIVE_PREFIX: &'static str = "[SESSION_OBJECTIVE] ";
    const RUNTIME_REFORMULATION_PREFIX: &'static str = "[HERMES_RUNTIME_REFORMULATION] ";

    fn push_stream_extra_event(
        shared: &Arc<StdMutex<Option<StreamHandle>>>,
        payload: serde_json::Value,
    ) {
        if let Ok(guard) = shared.lock() {
            if let Some(handle) = guard.clone() {
                handle.send_chunk(hermes_core::StreamChunk {
                    delta: Some(hermes_core::StreamDelta {
                        content: None,
                        tool_calls: None,
                        extra: Some(payload),
                    }),
                    finish_reason: None,
                    usage: None,
                });
            }
        }
    }

    fn preview_for_status(raw: &str, max_chars: usize) -> String {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return String::new();
        }
        let collapsed = trimmed.split_whitespace().collect::<Vec<_>>().join(" ");
        if collapsed.chars().count() <= max_chars {
            collapsed
        } else {
            let mut out: String = collapsed
                .chars()
                .take(max_chars.saturating_sub(1))
                .collect();
            out.push('…');
            out
        }
    }

    fn set_env_if_changed(key: &str, value: &str) -> bool {
        let next = value.trim();
        if next.is_empty() {
            return false;
        }
        let current = std::env::var(key).ok().unwrap_or_default();
        if current == next {
            return false;
        }
        crate::env_vars::set_var(key, next);
        true
    }

    fn bool_env(key: &str) -> Option<bool> {
        let raw = std::env::var(key).ok()?;
        let normalized = raw.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        }
    }

    fn is_unbounded_token(raw: &str) -> bool {
        matches!(
            raw.trim().to_ascii_lowercase().as_str(),
            "off" | "unlimited" | "infinite" | "max"
        )
    }

    fn auth_refresh_retry_limit() -> usize {
        std::env::var("HERMES_AUTH_REFRESH_MAX_RETRIES")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(3)
    }

    fn transient_retry_limit() -> usize {
        std::env::var("HERMES_TRANSIENT_MAX_RETRIES")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(2)
    }

    fn is_transient_retryable_error(err: &AgentError) -> bool {
        let message = match err {
            AgentError::LlmApi(msg)
            | AgentError::Config(msg)
            | AgentError::ToolExecution(msg)
            | AgentError::Gateway(msg)
            | AgentError::AuthFailed(msg)
            | AgentError::Io(msg) => msg.to_ascii_lowercase(),
            _ => return false,
        };
        message.contains("timed out")
            || message.contains("timeout")
            || message.contains("connection reset")
            || message.contains("connection refused")
            || message.contains("temporarily unavailable")
            || message.contains("try again")
            || message.contains("rate limit")
            || message.contains("429")
            || message.contains("502")
            || message.contains("503")
            || message.contains("504")
            || message.contains("provider rejected")
    }

    fn objective_execution_enforcer_enabled() -> bool {
        !matches!(
            std::env::var("HERMES_OBJECTIVE_EXECUTION_ENFORCER")
                .ok()
                .as_deref()
                .map(|v| v.trim().to_ascii_lowercase()),
            Some(v) if matches!(v.as_str(), "0" | "false" | "off" | "no")
        )
    }

    fn objective_continuation_retry_limit() -> usize {
        std::env::var("HERMES_OBJECTIVE_CONTINUATION_MAX_RETRIES")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(1)
    }

    fn load_active_objective_contract() -> Option<ObjectiveContract> {
        load_objective_contract()
            .ok()
            .flatten()
            .filter(|contract| objective_lifecycle_is_active(&contract.lifecycle_status))
    }

    fn looks_like_status_only_output(text: &str) -> bool {
        let lowered = text.trim().to_ascii_lowercase();
        if lowered.is_empty() {
            return true;
        }

        let has_future_language = [
            "i will",
            "i'll",
            "next i",
            "going to",
            "plan:",
            "i can",
            "we should",
            "i would",
            "i'll proceed",
            "i will proceed",
            "proceeding with",
        ]
        .iter()
        .any(|needle| lowered.contains(needle));
        let has_execution_evidence = [
            "path=",
            "file=",
            "exit code",
            "result:",
            "tested",
            "verified",
            "implemented",
            "changed",
            "patched",
            "command:",
            "run_id",
            "metric",
        ]
        .iter()
        .any(|needle| lowered.contains(needle));

        let has_weakness_markers = [
            "let me know",
            "if you'd like",
            "i can do that next",
            "awaiting",
            "need your confirmation",
        ]
        .iter()
        .any(|needle| lowered.contains(needle));

        (has_future_language && !has_execution_evidence) || has_weakness_markers
    }

    fn should_force_objective_continuation(
        &self,
        result: &hermes_core::AgentResult,
        baseline_len: usize,
    ) -> Option<String> {
        if !Self::objective_execution_enforcer_enabled() {
            return None;
        }
        let contract = Self::load_active_objective_contract()?;
        let behavior_mode = canonical_objective_behavior_mode(&contract.behavior_mode);
        if !matches!(behavior_mode.as_str(), "autonomous" | "mission") {
            return None;
        }

        let new_messages = if result.messages.len() > baseline_len {
            &result.messages[baseline_len..]
        } else {
            &result.messages[..]
        };

        let had_tool_activity = new_messages.iter().any(|message| {
            message.role == hermes_core::MessageRole::Tool
                || (message.role == hermes_core::MessageRole::Assistant
                    && message
                        .tool_calls
                        .as_ref()
                        .map(|calls| !calls.is_empty())
                        .unwrap_or(false))
        });
        if had_tool_activity {
            return None;
        }

        let output = Self::extract_last_assistant_output(new_messages);
        if output.trim().is_empty() {
            return Some(
                "assistant returned empty output while objective remained active".to_string(),
            );
        }
        if Self::looks_like_status_only_output(&output) {
            return Some(
                "assistant output was status/plan-heavy without concrete executed action"
                    .to_string(),
            );
        }
        None
    }

    fn objective_continuation_system_prompt(reason: &str) -> String {
        format!(
            "[OBJECTIVE_CONTINUATION_ENFORCER]\n\
             reason={}\n\
             Continue objective execution immediately.\n\
             Requirements for this pass:\n\
             1) execute at least one concrete action (tool or code operation),\n\
             2) include verifiable evidence from that action,\n\
             3) report objective delta in measurable terms,\n\
             4) end with the next highest-value action and continue momentum.\n\
             Do not return a plan-only or defer-only response.",
            reason
        )
    }

    fn should_force_preflight_auth_refresh(provider: &str) -> bool {
        if let Some(explicit) = Self::bool_env("HERMES_FORCE_RUNTIME_AUTH_REFRESH") {
            return explicit;
        }
        matches!(
            provider,
            "nous" | "qwen-oauth" | "google-gemini-cli" | "gemini-cli" | "gemini-oauth"
        )
    }

    fn nous_refresh_contention_error(err: &AgentError) -> bool {
        let text = err.to_string().to_ascii_lowercase();
        text.contains("slow_down")
            || text.contains("too many requests")
            || text.contains("refresh already in progress")
            || text.contains("429")
    }

    fn apply_nous_runtime_credentials(creds: &NousRuntimeCredentials) -> bool {
        let mut changed = false;
        changed |= Self::set_env_if_changed("NOUS_API_KEY", &creds.api_key);
        if !creds.base_url.trim().is_empty() {
            changed |= Self::set_env_if_changed("NOUS_INFERENCE_BASE_URL", &creds.base_url);
        }
        changed
    }

    fn contextlattice_ui_status_enabled() -> bool {
        !matches!(
            std::env::var("HERMES_CONTEXTLATTICE_UI_STATUS")
                .ok()
                .as_deref()
                .map(|v| v.trim().to_ascii_lowercase()),
            Some(v) if matches!(v.as_str(), "0" | "false" | "off" | "no")
        )
    }

    fn contextlattice_orchestrator_url() -> String {
        std::env::var("CONTEXTLATTICE_ORCHESTRATOR_URL")
            .ok()
            .or_else(|| std::env::var("MEMMCP_ORCHESTRATOR_URL").ok())
            .map(|v| v.trim().trim_end_matches('/').to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "http://127.0.0.1:8075".to_string())
    }

    fn contextlattice_ping_timeout_secs() -> u64 {
        std::env::var("HERMES_CONTEXTLATTICE_PING_TIMEOUT_SECONDS")
            .ok()
            .and_then(|raw| raw.trim().parse::<u64>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(12)
            .clamp(1, 120)
    }

    async fn emit_contextlattice_connectivity_status(&self) {
        if !Self::contextlattice_ui_status_enabled() {
            return;
        }
        let base = Self::contextlattice_orchestrator_url();
        let url = format!("{}/status", base);
        let topic = std::env::var("CONTEXTLATTICE_TOPIC_PATH")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "runbooks/hermes".to_string());
        Self::emit_lifecycle_event(
            &self.stream_handle_shared,
            format!("contextlattice preflight ping {} (topic={})", base, topic),
        );
        let client = match reqwest::Client::builder()
            .timeout(Duration::from_secs(Self::contextlattice_ping_timeout_secs()))
            .build()
        {
            Ok(c) => c,
            Err(err) => {
                Self::emit_lifecycle_event(
                    &self.stream_handle_shared,
                    format!("contextlattice client init failed: {}", err),
                );
                return;
            }
        };
        match client.get(&url).send().await {
            Ok(resp) => {
                let status_code = resp.status();
                if status_code.is_success() {
                    let parsed = resp.json::<serde_json::Value>().await.ok();
                    let service = parsed
                        .as_ref()
                        .and_then(|v| v.get("service").and_then(|s| s.as_str()))
                        .unwrap_or("unknown");
                    let ok_flag = parsed
                        .as_ref()
                        .and_then(|v| v.get("ok").and_then(|s| s.as_bool()))
                        .unwrap_or(true);
                    let detail = if ok_flag { "connected" } else { "degraded" };
                    Self::emit_lifecycle_event(
                        &self.stream_handle_shared,
                        format!(
                            "contextlattice {} (service={} status={} endpoint={})",
                            detail, service, status_code, base
                        ),
                    );
                    Self::emit_phase_event(
                        &self.stream_handle_shared,
                        "context",
                        if ok_flag {
                            "contextlattice connected"
                        } else {
                            "contextlattice degraded"
                        },
                        12,
                    );
                } else {
                    Self::emit_lifecycle_event(
                        &self.stream_handle_shared,
                        format!(
                            "contextlattice status endpoint returned {} ({})",
                            status_code, url
                        ),
                    );
                }
            }
            Err(err) => {
                Self::emit_lifecycle_event(
                    &self.stream_handle_shared,
                    format!("contextlattice preflight failed: {} ({})", err, url),
                );
            }
        }
    }

    fn auto_nous_reauth_enabled() -> bool {
        !matches!(
            std::env::var("HERMES_AUTO_NOUS_REAUTH")
                .ok()
                .as_deref()
                .map(|v| v.trim().to_ascii_lowercase()),
            Some(v) if matches!(v.as_str(), "0" | "false" | "off" | "no")
        )
    }

    fn auth_error_requires_nous_login(err: &AgentError) -> bool {
        let text = err.to_string().to_ascii_lowercase();
        text.contains("not logged into nous portal")
            || text.contains("re-run `hermes auth nous`")
            || text.contains("stored nous auth state is invalid")
            || text.contains("missing refresh token")
            || text.contains("invalid nous refresh response")
    }

    async fn attempt_interactive_nous_login(&mut self, reason: &str) -> bool {
        if !Self::auto_nous_reauth_enabled() {
            return false;
        }
        Self::emit_lifecycle_event(
            &self.stream_handle_shared,
            format!("Nous OAuth re-auth required ({reason}); launching portal login flow"),
        );
        match login_nous_device_code(NousDeviceCodeOptions::default()).await {
            Ok(state) => match save_nous_auth_state(&state) {
                Ok(path) => {
                    Self::emit_lifecycle_event(
                        &self.stream_handle_shared,
                        format!("Nous OAuth state refreshed: {}", path.display()),
                    );
                    true
                }
                Err(err) => {
                    Self::emit_lifecycle_event(
                        &self.stream_handle_shared,
                        format!("Nous OAuth state save failed: {}", err),
                    );
                    false
                }
            },
            Err(err) => {
                Self::emit_lifecycle_event(
                    &self.stream_handle_shared,
                    format!("Nous OAuth interactive login failed: {}", err),
                );
                false
            }
        }
    }

    async fn refresh_runtime_provider_credentials_if_needed(&mut self, force_refresh: bool) {
        let (provider_name, _) = resolve_provider_and_model(&self.config, &self.current_model);
        let provider = normalize_runtime_provider_name(provider_name.as_str());
        let mut rotated = false;
        let mut note: Option<String> = None;

        match provider.as_str() {
            "nous" => match resolve_nous_runtime_credentials(
                force_refresh,
                true,
                NOUS_ACCESS_TOKEN_REFRESH_SKEW_SECONDS,
                DEFAULT_NOUS_AGENT_KEY_MIN_TTL_SECONDS,
            )
            .await
            {
                Ok(creds) => {
                    rotated |= Self::apply_nous_runtime_credentials(&creds);
                    if rotated {
                        note = Some("refreshed Nous runtime credential".to_string());
                    }
                }
                Err(e) => {
                    if force_refresh && Self::nous_refresh_contention_error(&e) {
                        match resolve_nous_runtime_credentials(
                            false,
                            true,
                            NOUS_ACCESS_TOKEN_REFRESH_SKEW_SECONDS,
                            DEFAULT_NOUS_AGENT_KEY_MIN_TTL_SECONDS,
                        )
                        .await
                        {
                            Ok(creds) => {
                                rotated |= Self::apply_nous_runtime_credentials(&creds);
                                note = Some(
                                    "Nous refresh busy; reused cached runtime credential"
                                        .to_string(),
                                );
                            }
                            Err(cache_err) => {
                                Self::emit_lifecycle_event(
                                    &self.stream_handle_shared,
                                    format!(
                                        "warning: Nous cached credential hydration failed after refresh contention ({cache_err})"
                                    ),
                                );
                            }
                        }
                    }
                    if Self::auth_error_requires_nous_login(&e)
                        && self
                            .attempt_interactive_nous_login("credential missing or invalid")
                            .await
                    {
                        match resolve_nous_runtime_credentials(
                            true,
                            true,
                            NOUS_ACCESS_TOKEN_REFRESH_SKEW_SECONDS,
                            DEFAULT_NOUS_AGENT_KEY_MIN_TTL_SECONDS,
                        )
                        .await
                        {
                            Ok(creds) => {
                                rotated |= Self::apply_nous_runtime_credentials(&creds);
                                if rotated {
                                    note = Some("refreshed Nous runtime credential".to_string());
                                }
                            }
                            Err(err) => {
                                Self::emit_lifecycle_event(
                                    &self.stream_handle_shared,
                                    format!("warning: Nous credential refresh skipped ({err})"),
                                );
                            }
                        }
                    } else {
                        if !rotated && note.is_none() {
                            Self::emit_lifecycle_event(
                                &self.stream_handle_shared,
                                format!("warning: Nous credential refresh skipped ({e})"),
                            );
                        }
                    }
                }
            },
            "qwen-oauth" => match resolve_qwen_runtime_credentials(
                force_refresh,
                true,
                QWEN_ACCESS_TOKEN_REFRESH_SKEW_SECONDS,
            )
            .await
            {
                Ok(creds) => {
                    rotated |=
                        Self::set_env_if_changed("HERMES_QWEN_OAUTH_API_KEY", &creds.api_key);
                    rotated |= Self::set_env_if_changed("DASHSCOPE_API_KEY", &creds.api_key);
                    if !creds.base_url.trim().is_empty() {
                        rotated |=
                            Self::set_env_if_changed("HERMES_QWEN_BASE_URL", &creds.base_url);
                    }
                    if rotated {
                        note = Some("refreshed Qwen OAuth runtime credential".to_string());
                    }
                }
                Err(e) => {
                    Self::emit_lifecycle_event(
                        &self.stream_handle_shared,
                        format!("warning: Qwen OAuth refresh skipped ({e})"),
                    );
                }
            },
            "google-gemini-cli" | "gemini-cli" | "gemini-oauth" => {
                match resolve_gemini_oauth_runtime_credentials(force_refresh).await {
                    Ok(creds) => {
                        rotated |=
                            Self::set_env_if_changed("HERMES_GEMINI_OAUTH_API_KEY", &creds.api_key);
                        rotated |= Self::set_env_if_changed("GOOGLE_API_KEY", &creds.api_key);
                        rotated |= Self::set_env_if_changed("GEMINI_API_KEY", &creds.api_key);
                        if rotated {
                            note = Some("refreshed Gemini OAuth runtime credential".to_string());
                        }
                    }
                    Err(e) => {
                        Self::emit_lifecycle_event(
                            &self.stream_handle_shared,
                            format!("warning: Gemini OAuth refresh skipped ({e})"),
                        );
                    }
                }
            }
            _ => {}
        }

        if rotated {
            self.switch_model(&self.current_model.clone());
        }
        if let Some(msg) = note {
            Self::emit_lifecycle_event(&self.stream_handle_shared, msg);
        }
    }

    fn stream_callbacks(shared: Arc<StdMutex<Option<StreamHandle>>>) -> AgentCallbacks {
        let thinking_shared = shared.clone();
        let tool_start_shared = shared.clone();
        let tool_done_shared = shared.clone();
        let status_shared = shared;
        AgentCallbacks {
            on_thinking: Some(Box::new(move |thinking: &str| {
                let preview = App::preview_for_status(thinking, 220);
                if preview.is_empty() {
                    return;
                }
                App::push_stream_extra_event(
                    &thinking_shared,
                    serde_json::json!({
                        "ui_event": "thinking",
                        "text": preview,
                    }),
                );
            })),
            on_tool_start: Some(Box::new(move |tool: &str, args: &Value| {
                let arg_preview = App::preview_for_status(&args.to_string(), 140);
                App::push_stream_extra_event(
                    &tool_start_shared,
                    serde_json::json!({
                        "ui_event": "tool_start",
                        "tool": tool,
                        "args_preview": arg_preview,
                    }),
                );
            })),
            on_tool_complete: Some(Box::new(move |tool: &str, content: &str| {
                let preview = App::preview_for_status(content, 160);
                let failed = content.trim_start().starts_with("Error")
                    || content.contains("Tool execution failed")
                    || content.contains("timed out after");
                let mut payload = serde_json::json!({
                    "ui_event": "tool_complete",
                    "tool": tool,
                    "result_preview": preview,
                    "failed": failed,
                });
                if failed && !preview.is_empty() {
                    payload["error"] = serde_json::Value::String(preview.clone());
                }
                App::push_stream_extra_event(&tool_done_shared, payload);
            })),
            status_callback: Some(Arc::new(move |event_type: &str, message: &str| {
                let preview = App::preview_for_status(message, 200);
                if preview.is_empty() {
                    return;
                }
                App::push_stream_extra_event(
                    &status_shared,
                    serde_json::json!({
                        "ui_event": "status",
                        "event_type": event_type,
                        "message": preview,
                    }),
                );
            })),
            ..AgentCallbacks::default()
        }
    }

    fn emit_lifecycle_event(
        shared: &Arc<StdMutex<Option<StreamHandle>>>,
        message: impl AsRef<str>,
    ) {
        let preview = App::preview_for_status(message.as_ref(), 220);
        if preview.is_empty() {
            return;
        }
        if App::oneshot_lifecycle_stdout_enabled(shared) {
            println!("[lifecycle] {}", preview);
        }
        App::push_stream_extra_event(
            shared,
            serde_json::json!({
                "ui_event": "lifecycle",
                "message": preview,
            }),
        );
    }

    fn emit_phase_event(
        shared: &Arc<StdMutex<Option<StreamHandle>>>,
        phase: &str,
        label: &str,
        progress_pct: u8,
    ) {
        let phase = phase.trim();
        let label = App::preview_for_status(label, 220);
        if phase.is_empty() || label.is_empty() {
            return;
        }
        if App::oneshot_lifecycle_stdout_enabled(shared) {
            println!("[phase {:>3}%] {}: {}", progress_pct.min(100), phase, label);
        }
        App::push_stream_extra_event(
            shared,
            serde_json::json!({
                "ui_event": "phase",
                "phase": phase,
                "label": label,
                "progress_pct": progress_pct.min(100),
            }),
        );
    }

    fn oneshot_lifecycle_stdout_enabled(shared: &Arc<StdMutex<Option<StreamHandle>>>) -> bool {
        let stream_attached = shared
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(|_| ()))
            .is_some();
        if stream_attached {
            return false;
        }
        matches!(
            std::env::var("HERMES_ONESHOT_LIFECYCLE_STDOUT")
                .ok()
                .as_deref()
                .map(|v| v.trim().to_ascii_lowercase()),
            Some(v) if matches!(v.as_str(), "1" | "true" | "yes" | "on")
        )
    }

    fn objective_context_autopin_enabled() -> bool {
        !matches!(
            std::env::var("HERMES_OBJECTIVE_CONTEXT_AUTOPIN")
                .ok()
                .as_deref()
                .map(|v| v.trim().to_ascii_lowercase()),
            Some(v) if matches!(v.as_str(), "0" | "false" | "off" | "no")
        )
    }

    fn sanitize_topic_path_segment(raw: &str) -> String {
        let mut out = String::with_capacity(raw.len());
        for ch in raw.chars() {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '/') {
                out.push(ch);
            } else {
                out.push('-');
            }
        }
        out.trim_matches('-').to_string()
    }

    fn maybe_autopin_contextlattice_topic_from_objective(&self) {
        if !Self::objective_context_autopin_enabled() {
            return;
        }
        let Ok(Some(contract)) = load_objective_contract() else {
            return;
        };
        let objective_id = Self::sanitize_topic_path_segment(contract.id.trim());
        if objective_id.is_empty() {
            return;
        }
        let target_topic = format!("runbooks/objective/{}", objective_id);
        let current_topic = std::env::var("CONTEXTLATTICE_TOPIC_PATH")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        let should_override = match current_topic.as_deref() {
            None => true,
            Some("runbooks/hermes") => true,
            Some(existing)
                if existing.eq_ignore_ascii_case(target_topic.as_str())
                    || !existing
                        .to_ascii_lowercase()
                        .starts_with("runbooks/objective/") =>
            {
                false
            }
            Some(_) => true,
        };
        if should_override {
            crate::env_vars::set_var("CONTEXTLATTICE_TOPIC_PATH", &target_topic);
            Self::emit_lifecycle_event(
                &self.stream_handle_shared,
                format!(
                    "ContextLattice objective autopin set topic_path={} (objective_id={})",
                    target_topic, contract.id
                ),
            );
            Self::emit_phase_event(
                &self.stream_handle_shared,
                "context",
                "objective context autopin",
                8,
            );
        }
    }

    fn runtime_prompt_reformulation_enabled() -> bool {
        !matches!(
            std::env::var("HERMES_RUNTIME_PROMPT_REFORMULATION")
                .ok()
                .as_deref()
                .map(|v| v.trim().to_ascii_lowercase()),
            Some(v) if matches!(v.as_str(), "0" | "false" | "off" | "no")
        )
    }

    fn runtime_contradiction_self_check_enabled() -> bool {
        !matches!(
            std::env::var("HERMES_RUNTIME_CONTRADICTION_SELF_CHECK")
                .ok()
                .as_deref()
                .map(|v| v.trim().to_ascii_lowercase()),
            Some(v) if matches!(v.as_str(), "0" | "false" | "off" | "no")
        )
    }

    fn runtime_reformulation_prompt_preview_chars() -> usize {
        std::env::var("HERMES_RUNTIME_REFORMULATION_PROMPT_PREVIEW_CHARS")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(RUNTIME_REFORMULATION_PROMPT_PREVIEW_CHARS)
    }

    fn current_tool_profile_mode() -> String {
        std::env::var("HERMES_REPO_REVIEW_TOOL_PROFILE_MODE")
            .ok()
            .map(|v| v.trim().to_ascii_lowercase())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "balanced".to_string())
    }

    fn build_runtime_reformulation_message(&self, latest_user_prompt: &str) -> Option<String> {
        if !Self::runtime_prompt_reformulation_enabled() {
            return None;
        }
        let prompt = latest_user_prompt.trim();
        if prompt.is_empty() {
            return None;
        }
        let tool_profile_mode = Self::current_tool_profile_mode();
        let contradiction_check = Self::runtime_contradiction_self_check_enabled();
        let context_topic = std::env::var("CONTEXTLATTICE_TOPIC_PATH")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "runbooks/hermes".to_string());

        let objective_contract = Self::load_active_objective_contract();
        let objective_line = objective_contract
            .as_ref()
            .map(|contract| {
                format!(
                    "objective(active): {} | behavior={} | text={}",
                    contract.id,
                    canonical_objective_behavior_mode(&contract.behavior_mode),
                    Self::preview_for_status(&contract.objective_text, 220)
                )
            })
            .unwrap_or_else(|| "objective(active): none".to_string());
        let objective_directives = objective_contract
            .as_ref()
            .map(|contract| {
                contract
                    .behavior_directives
                    .iter()
                    .take(6)
                    .map(|line| format!("- {}", line.trim()))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "- (none)".to_string());
        let objective_success = objective_contract
            .as_ref()
            .map(|contract| {
                contract
                    .success_criteria
                    .iter()
                    .take(5)
                    .map(|line| format!("- {}", line.trim()))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "- (none)".to_string());

        let contradiction_line = if contradiction_check {
            "before final response: self-audit contradictions across tool outputs, runtime facts, and claims; unresolved items must be marked UNPROVEN/CONTRADICTORY."
        } else {
            "before final response: consistency self-audit optional (disabled by runtime toggle)."
        };

        let mut out = String::new();
        out.push_str(Self::RUNTIME_REFORMULATION_PREFIX);
        out.push_str(
            "\nRuntime execution reformulation (internal):\n\
             1) apply anti-scheming evidence-first discipline\n\
             2) pull ContextLattice context first when relevant\n\
             3) route tool usage intentionally and avoid repetitive low-signal loops\n\
             4) match requested output shape exactly (count/format), with no template placeholders or duplicate list items\n\
             5) for open-ended missions, execute at least one concrete action before returning status text\n\
             6) maintain iterative objective momentum: gather evidence, test, refine, then continue with next high-value action\n",
        );
        out.push_str(&format!(
            "tool-profile(mode): {}\ncontextlattice(topic): {}\n{}\n",
            tool_profile_mode, context_topic, objective_line
        ));
        out.push_str("objective behavior directives:\n");
        out.push_str(&objective_directives);
        out.push('\n');
        out.push_str("objective success criteria:\n");
        out.push_str(&objective_success);
        out.push('\n');
        out.push_str(
            "objective loop protocol:\n\
             - baseline: state current objective KPI and latest known value\n\
             - execute: perform concrete highest-leverage action now\n\
             - verify: present measurable delta or explicit blocked evidence\n\
             - continue: state next action with no soft deferral\n",
        );
        out.push_str(contradiction_line);
        out.push_str("\nuser-request(routing-preview):\n");
        let preview_cap = Self::runtime_reformulation_prompt_preview_chars();
        let prompt_preview = Self::preview_for_status(prompt, preview_cap);
        out.push_str(&prompt_preview);
        if prompt.chars().count() > preview_cap {
            out.push_str(
                "\n[preview truncated; the full user request remains available as the next user message]",
            );
        } else {
            out.push_str("\n[full user request remains available as the next user message]");
        }
        Some(out)
    }

    fn build_inference_messages(&self) -> (Vec<hermes_core::Message>, bool) {
        let mut messages = self.messages.clone();
        let Some(last_user_idx) = messages
            .iter()
            .rposition(|m| m.role == hermes_core::MessageRole::User)
        else {
            return (messages, false);
        };
        let user_prompt = messages[last_user_idx]
            .content
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_string();
        let Some(reformulation) = self.build_runtime_reformulation_message(&user_prompt) else {
            return (messages, false);
        };
        messages.insert(last_user_idx, hermes_core::Message::system(reformulation));
        (messages, true)
    }

    fn apply_explore_first_runtime_defaults() {
        if std::env::var("HERMES_SKILL_GUARD_MODE")
            .ok()
            .map(|v| v.trim().is_empty())
            .unwrap_or(true)
        {
            crate::env_vars::set_var("HERMES_SKILL_GUARD_MODE", "off");
        }
        if std::env::var("HERMES_GUARD_MODE")
            .ok()
            .map(|v| v.trim().is_empty())
            .unwrap_or(true)
        {
            crate::env_vars::set_var("HERMES_GUARD_MODE", "off");
        }
        if std::env::var("HERMES_TOOL_POLICY_PRESET")
            .ok()
            .map(|v| v.trim().is_empty())
            .unwrap_or(true)
        {
            crate::env_vars::set_var("HERMES_TOOL_POLICY_PRESET", "dev");
        }
        if std::env::var("HERMES_TOOL_POLICY_MODE")
            .ok()
            .map(|v| v.trim().is_empty())
            .unwrap_or(true)
        {
            crate::env_vars::set_var("HERMES_TOOL_POLICY_MODE", "audit");
        }
        if std::env::var("HERMES_REPO_REVIEW_BUDGET_PROFILE")
            .ok()
            .map(|v| v.trim().is_empty())
            .unwrap_or(true)
        {
            crate::env_vars::set_var("HERMES_REPO_REVIEW_BUDGET_PROFILE", "off");
        }
        if std::env::var("HERMES_MAX_ITERATIONS")
            .ok()
            .map(|v| v.trim().is_empty())
            .unwrap_or(true)
        {
            crate::env_vars::set_var("HERMES_MAX_ITERATIONS", "250");
        }
        if std::env::var("HERMES_TOOL_CALL_MAX_CONCURRENCY")
            .ok()
            .map(|v| v.trim().is_empty())
            .unwrap_or(true)
        {
            crate::env_vars::set_var("HERMES_TOOL_CALL_MAX_CONCURRENCY", "12");
        }
        if std::env::var("HERMES_MAX_DELEGATE_DEPTH")
            .ok()
            .map(|v| v.trim().is_empty())
            .unwrap_or(true)
        {
            crate::env_vars::set_var("HERMES_MAX_DELEGATE_DEPTH", "4");
        }
    }

    /// Create a new `App` from the parsed CLI arguments.
    ///
    /// This loads (or creates) the gateway configuration, builds a tool
    /// registry with the configured tools, constructs an LLM provider,
    /// and initializes the agent loop.
    pub async fn new(cli: Cli) -> Result<Self, AgentError> {
        let state_root = state_dir(cli.config_dir.as_deref().map(std::path::Path::new));
        let config = load_config(cli.config_dir.as_deref())
            .map_err(|e| AgentError::Config(e.to_string()))?;

        let mut config = config;
        apply_cli_runtime_overrides(&mut config, &cli);
        Self::apply_explore_first_runtime_defaults();

        if config.sessions.auto_prune {
            let resolved_home = config
                .home_dir
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::var("HERMES_HOME")
                        .ok()
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .map(PathBuf::from)
                })
                .unwrap_or_else(hermes_home_dir);
            let sp = SessionPersistence::new(&resolved_home);
            let maintenance = sp.maybe_auto_prune_and_vacuum(
                config.sessions.retention_days,
                config.sessions.min_interval_hours,
                config.sessions.vacuum_after_prune,
            );
            if let Some(err) = maintenance.error {
                tracing::debug!("sessions db auto-maintenance skipped: {}", err);
            } else if !maintenance.skipped && maintenance.pruned > 0 {
                tracing::info!(
                    "sessions db auto-maintenance pruned {} session(s){}",
                    maintenance.pruned,
                    if maintenance.vacuumed {
                        " + vacuum"
                    } else {
                        ""
                    }
                );
            }
        }

        let configured_model = config.model.clone().unwrap_or_else(|| "gpt-4o".to_string());
        let current_model = resolve_startup_model(&config, &configured_model);
        let current_personality = config.personality.clone();

        sync_runtime_model_env(&config, &current_model);

        let tool_registry = Arc::new(ToolRegistry::new());
        if default_rtk_raw_mode() {
            tool_registry.set_raw_mode(true);
        }
        let stream_handle_shared: Arc<StdMutex<Option<StreamHandle>>> =
            Arc::new(StdMutex::new(None));
        let terminal_backend = build_terminal_backend(&config);
        let skill_store = Arc::new(FileSkillStore::new(FileSkillStore::default_dir()));
        let skill_provider: Arc<dyn hermes_core::SkillProvider> =
            Arc::new(SkillManager::new(skill_store));
        hermes_tools::register_builtin_tools(&tool_registry, terminal_backend, skill_provider);
        let live_count =
            crate::live_messaging::enable_live_messaging_tool(&config, &tool_registry).await;
        if live_count > 0 {
            tracing::info!(
                adapters = live_count,
                "send_message live delivery enabled via configured gateway adapters"
            );
        }
        wire_stdio_clarify_backend(&tool_registry);
        let cron_data_dir = state_root.join("cron");
        std::fs::create_dir_all(&cron_data_dir)
            .map_err(|e| AgentError::Io(format!("cron dir {}: {}", cron_data_dir.display(), e)))?;
        let cron_scheduler = Arc::new(cron_scheduler_for_data_dir(cron_data_dir));
        cron_scheduler
            .load_persisted_jobs()
            .await
            .map_err(|e| AgentError::Config(format!("cron load: {e}")))?;
        cron_scheduler.start().await;
        wire_cron_scheduler_backend(
            &tool_registry,
            cron_scheduler,
            MessagingSessionContext::new(),
        );
        let agent_tool_registry = Arc::new(bridge_tool_registry(&tool_registry));
        let tool_schemas =
            crate::platform_toolsets::resolve_platform_tool_schemas(&config, "cli", &tool_registry);

        let agent_config = build_agent_config(&config, &current_model);
        let provider = build_provider(&config, &current_model);

        let agent_inner = hermes_agent::attach_agent_runtime(
            AgentLoop::new(agent_config, agent_tool_registry, provider)
                .with_async_tool_dispatch(async_tool_dispatch_for(tool_registry.clone())),
        )
        .with_callbacks(Self::stream_callbacks(stream_handle_shared.clone()));
        let orchestrator = Arc::new(SubAgentOrchestrator::from_parent(
            &agent_inner,
            state_root.clone(),
        ));
        let agent = Arc::new(agent_inner.with_sub_agent_orchestrator(orchestrator));

        let recovered_background_jobs = recover_queued_background_jobs(8);
        if recovered_background_jobs > 0 {
            tracing::info!(
                "Recovered {} queued background job(s) from durable status queue",
                recovered_background_jobs
            );
        }

        let app = Self {
            state_root,
            config: Arc::new(config),
            agent,
            tool_registry,
            tool_schemas,
            messages: Vec::new(),
            ui_messages: Vec::new(),
            session_id: Uuid::new_v4().to_string(),
            running: true,
            current_model,
            current_personality,
            input_history: Vec::new(),
            history_index: 0,
            interrupt_controller: InterruptController::new(),
            stream_handle: None,
            stream_handle_shared,
            mouse_enabled: default_mouse_enabled(),
            pending_theme: None,
            pending_image_hint: None,
            session_objective: None,
            pending_input_prefill: None,
            quorum_armed_once: false,
            pet_settings: load_pet_settings(),
            acp_server: None,
            acp_event_buffer: None,
            snapshot_gate: SnapshotPersistGate::new(),
        };
        app.ensure_session_stub_snapshot();
        Ok(app)
    }

    /// Attach a streaming handle (used by TUI mode).
    pub fn set_stream_handle(&mut self, handle: Option<StreamHandle>) {
        if let Ok(mut guard) = self.stream_handle_shared.lock() {
            *guard = handle.clone();
        }
        self.stream_handle = handle;
    }

    /// Enable/disable TUI mouse handling.
    pub fn set_mouse_enabled(&mut self, enabled: bool) {
        self.mouse_enabled = enabled;
    }

    /// Current TUI mouse handling state.
    pub fn mouse_enabled(&self) -> bool {
        self.mouse_enabled
    }

    /// Queue a TUI skin/theme change request to be applied in the UI loop.
    pub fn request_theme_change(&mut self, skin: &str) {
        let value = skin.trim();
        if value.is_empty() {
            return;
        }
        self.pending_theme = Some(value.to_string());
    }

    /// Queue an image hint for the next user prompt.
    pub fn set_pending_image_hint(&mut self, path: String) {
        let trimmed = path.trim();
        self.pending_image_hint = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        };
    }

    /// Read queued image hint without consuming it.
    pub fn pending_image_hint(&self) -> Option<&str> {
        self.pending_image_hint.as_deref()
    }

    /// Clear queued image hint.
    pub fn clear_pending_image_hint(&mut self) {
        self.pending_image_hint = None;
    }

    /// Prepare outbound user text, consuming any queued image hint.
    pub fn prepare_user_message(&mut self, raw: &str) -> String {
        let base = raw.trim();
        if let Some(path) = self
            .pending_image_hint
            .take()
            .filter(|value| !value.trim().is_empty())
        {
            format!("[IMAGE_HINT] path={}\n{}", path, base)
        } else {
            base.to_string()
        }
    }

    /// Drain any queued skin/theme change request.
    pub fn take_pending_theme_change(&mut self) -> Option<String> {
        self.pending_theme.take()
    }

    /// Drain composer prefill staged by `/undo` or `/rewind`.
    pub fn take_pending_input_prefill(&mut self) -> Option<String> {
        self.pending_input_prefill.take()
    }

    /// Retrieve current companion pet settings.
    pub fn pet_settings(&self) -> &PetSettings {
        &self.pet_settings
    }

    /// Update and persist companion pet settings.
    pub fn set_pet_settings(&mut self, settings: PetSettings) -> Result<(), AgentError> {
        let normalized = settings.normalized();
        persist_pet_settings(&normalized)?;
        self.pet_settings = normalized;
        Ok(())
    }

    /// Run the interactive REPL loop.
    ///
    /// This is the main entry point for interactive mode. It delegates
    /// to the TUI subsystem for rendering and event handling.
    pub async fn run_interactive(&mut self) -> Result<(), AgentError> {
        // The actual TUI loop is in crate::tui::run()
        // This method exists so non-TUI callers can drive the loop manually.
        if self.running {
            loop {
                if !self.running {
                    break;
                }
                // In a real implementation, the TUI event loop would drive this.
                // Here we just mark that we're ready.
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
        Ok(())
    }

    /// Handle a line of user input.
    ///
    /// If the input starts with `/` it is treated as a slash command.
    /// Otherwise it is sent as a user message to the agent.
    pub async fn handle_input(&mut self, input: &str) -> Result<(), AgentError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Ok(());
        }

        // Store in input history
        self.input_history.push(trimmed.to_string());
        self.history_index = self.input_history.len();

        if trimmed.starts_with('/') {
            if self.stream_handle.is_some() {
                self.push_ui_user(trimmed);
            }
            // Parse the slash command and its arguments
            let parts: Vec<&str> = trimmed.splitn(2, ' ').collect();
            let cmd = parts[0];
            let args: Vec<&str> = parts
                .get(1)
                .map(|s| s.split_whitespace().collect())
                .unwrap_or_default();

            let result = crate::commands::handle_slash_command(self, cmd, &args).await?;
            if result == crate::commands::CommandResult::Quit {
                self.running = false;
            }
        } else {
            // Regular user message
            let user_message = self.prepare_user_message(trimmed);
            self.messages.push(hermes_core::Message::user(user_message));
            self.run_agent().await?;
        }

        Ok(())
    }

    /// Handle a slash command string (without the leading `/`).
    pub async fn handle_command(&mut self, cmd: &str) -> Result<(), AgentError> {
        let trimmed = cmd.trim();
        if trimmed.is_empty() {
            return Ok(());
        }

        let parts: Vec<&str> = trimmed.splitn(2, ' ').collect();
        let slash_cmd = if parts[0].starts_with('/') {
            parts[0]
        } else {
            // Prepend / if not present
            return self.handle_input(&format!("/{}", trimmed)).await;
        };

        if self.stream_handle.is_some() {
            self.push_ui_user(trimmed);
        }

        let args: Vec<&str> = parts
            .get(1)
            .map(|s| s.split_whitespace().collect())
            .unwrap_or_default();

        let result = crate::commands::handle_slash_command(self, slash_cmd, &args).await?;
        if result == crate::commands::CommandResult::Quit {
            self.running = false;
        }
        Ok(())
    }

    /// Sync runtime session id to the agent and notify memory providers.
    pub fn notify_memory_session_switch(
        &self,
        new_session_id: &str,
        parent_session_id: &str,
        reset: bool,
        reason: &str,
    ) {
        self.agent.set_runtime_session_id(new_session_id);
        self.agent
            .memory_on_session_switch(new_session_id, parent_session_id, reset, reason);
    }

    /// Run agent-loop context compression on the current CLI transcript.
    pub async fn compress_conversation_context(
        &mut self,
    ) -> Result<(usize, usize, bool), AgentError> {
        let pre_len = self.messages.len();
        if pre_len <= 2 {
            return Ok((pre_len, pre_len, false));
        }
        let model = self.current_model.clone();
        let session_id = self.session_id.clone();
        let (compressed_messages, did_compress) = self
            .agent
            .compress_messages(self.messages.clone(), &session_id, &model)
            .await;
        let post_len = compressed_messages.len();
        self.messages = compressed_messages;
        self.ui_messages
            .retain(|msg| msg.insert_at <= self.messages.len());
        if let Some(new_sid) = self.agent.runtime_session_id() {
            let new_sid = new_sid.trim();
            if !new_sid.is_empty() && new_sid != self.session_id {
                self.session_id = new_sid.to_string();
            }
        }
        Ok((pre_len, post_len, did_compress))
    }

    /// Reset the current session — Python parity: same as [`Self::new_session`]
    /// (`/reset` is an alias of `/new`; rotates session id + memory switch).
    pub fn reset_session(&mut self) {
        self.new_session();
    }

    /// Set or clear a durable session objective.
    ///
    /// The objective is represented as a synthetic system message so it is
    /// applied consistently on every turn without requiring user re-entry.
    pub fn set_session_objective(&mut self, objective: Option<String>) {
        self.messages.retain(|m| {
            if m.role != hermes_core::MessageRole::System {
                return true;
            }
            !m.content
                .as_deref()
                .unwrap_or_default()
                .starts_with(Self::SESSION_OBJECTIVE_PREFIX)
        });

        self.session_objective = objective
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        if let Some(obj) = &self.session_objective {
            let system =
                hermes_core::Message::system(format!("{}{}", Self::SESSION_OBJECTIVE_PREFIX, obj));
            self.messages.insert(0, system);
        }
        self.prune_ui_after_current_messages();
    }

    /// Retry the last user message by re-sending it to the agent.
    ///
    /// Finds the last user message in history, removes all messages after it
    /// (including the assistant response), and re-runs the agent.
    pub async fn retry_last(&mut self) -> Result<(), AgentError> {
        // Find the last user message
        let last_user_idx = self
            .messages
            .iter()
            .rposition(|m| m.role == hermes_core::MessageRole::User);

        if let Some(idx) = last_user_idx {
            let last_user_msg = self.messages[idx].clone();
            // Truncate messages to just before the last user message
            self.messages.truncate(idx);
            // Re-add the user message
            self.messages.push(last_user_msg);
            // Re-run the agent
            self.run_agent().await?;
            self.prune_ui_after_current_messages();
        }

        Ok(())
    }

    /// Undo one or more user turns, returning the text staged for editing.
    pub fn undo_last(&mut self) -> Option<String> {
        self.undo_last_n(1)
    }

    pub fn undo_last_n(&mut self, user_turns: usize) -> Option<String> {
        let user_indices: Vec<usize> = self
            .messages
            .iter()
            .enumerate()
            .filter_map(|(idx, msg)| (msg.role == hermes_core::MessageRole::User).then_some(idx))
            .collect();
        if user_indices.is_empty() {
            return None;
        }
        let count = user_turns.max(1);
        let target_pos = user_indices.len().saturating_sub(count);
        let target_idx = user_indices[target_pos];
        let prefill = self.messages[target_idx]
            .content
            .as_deref()
            .unwrap_or_default()
            .to_string();

        match SessionPersistence::new(&self.state_root)
            .rewind_active_user_turns(&self.session_id, count)
        {
            Ok(Some(outcome)) => tracing::debug!(
                "Soft-rewound session {} at message {} (inactive={}, active={})",
                self.session_id,
                outcome.target_message_id,
                outcome.inactive_count,
                outcome.active_message_count
            ),
            Ok(None) => tracing::debug!(
                "No persisted session row available for undo in session {}",
                self.session_id
            ),
            Err(err) => tracing::debug!("Failed to soft-rewind persisted session: {}", err),
        }

        self.messages.truncate(target_idx);
        self.prune_ui_after_current_messages();
        if prefill.trim().is_empty() {
            self.pending_input_prefill = None;
        } else {
            self.pending_input_prefill = Some(prefill.clone());
        }
        Some(prefill)
    }

    /// Switch the active model, rebuilding the provider and agent loop.
    pub fn switch_model(&mut self, provider_model: &str) {
        self.current_model = provider_model.to_string();
        sync_runtime_model_env(&self.config, &self.current_model);

        let provider = build_provider(&self.config, &self.current_model);
        let agent_config = build_agent_config(&self.config, &self.current_model);
        let agent_tool_registry = Arc::new(bridge_tool_registry(&self.tool_registry));

        let agent_inner = hermes_agent::attach_agent_runtime(AgentLoop::new(
            agent_config,
            agent_tool_registry,
            provider,
        ))
        .with_async_tool_dispatch(async_tool_dispatch_for(self.tool_registry.clone()))
        .with_callbacks(Self::stream_callbacks(self.stream_handle_shared.clone()));
        let orchestrator = Arc::new(SubAgentOrchestrator::from_parent(
            &agent_inner,
            self.state_root.clone(),
        ));
        self.agent = Arc::new(agent_inner.with_sub_agent_orchestrator(orchestrator));

        match SessionPersistence::new(&self.state_root)
            .update_session_model(&self.session_id, &self.current_model)
        {
            Ok(true) => tracing::debug!(
                "Persisted model switch for session {} to {}",
                self.session_id,
                self.current_model
            ),
            Ok(false) => {}
            Err(err) => tracing::debug!("Failed to persist model switch to session DB: {}", err),
        }

        tracing::info!("Switched model to: {}", provider_model);
    }

    /// Switch the active personality.
    pub fn switch_personality(&mut self, name: &str) {
        self.current_personality = Some(name.to_string());
        tracing::info!("Switched personality to: {}", name);
    }

    /// Return the normalized runtime provider for the active model.
    pub fn current_runtime_provider(&self) -> String {
        let (provider_name, _) = resolve_provider_and_model(&self.config, &self.current_model);
        normalize_runtime_provider_name(provider_name.as_str())
    }

    /// Refresh and verify runtime credentials for the active provider.
    ///
    /// This is the command-surface lifecycle helper used by `/auth`.
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
            provider, refresh_mode, status, changed, self.current_model
        ))
    }

    pub(crate) async fn run_messages_with_current_agent(
        &self,
        messages: Vec<hermes_core::Message>,
        stream_enabled: bool,
    ) -> Result<hermes_core::AgentResult, AgentError> {
        self.run_messages_with_current_agent_tools(messages, stream_enabled, true)
            .await
    }

    async fn run_messages_with_current_agent_tools(
        &self,
        messages: Vec<hermes_core::Message>,
        stream_enabled: bool,
        include_tools: bool,
    ) -> Result<hermes_core::AgentResult, AgentError> {
        let tool_schemas = include_tools.then(|| self.tool_schemas.clone());
        let task_id = Some(self.session_id.clone());
        let (history, user_message) = split_messages_for_run_conversation(&messages)
            .ok_or_else(|| AgentError::Config("no user message in turn".into()))?;
        if stream_enabled && self.config.streaming.enabled {
            let stream_handle = self.stream_handle.clone();
            let stream_cb: Option<Box<dyn Fn(hermes_core::StreamChunk) + Send + Sync>> =
                stream_handle.map(|h| {
                    Box::new(move |chunk: hermes_core::StreamChunk| {
                        h.send_chunk(chunk);
                    }) as Box<dyn Fn(hermes_core::StreamChunk) + Send + Sync>
                });
            let conv = self
                .agent
                .run_conversation(RunConversationParams {
                    user_message,
                    conversation_history: history,
                    task_id,
                    stream_callback: stream_cb,
                    persist_user_message: None,
                    tools: tool_schemas,
                    persist_session: false,
                })
                .await?;
            Ok(conv.into_loop_result())
        } else {
            let conv = self
                .agent
                .run_conversation(RunConversationParams {
                    user_message,
                    conversation_history: history,
                    task_id,
                    stream_callback: None,
                    persist_user_message: None,
                    tools: tool_schemas,
                    persist_session: false,
                })
                .await?;
            Ok(conv.into_loop_result())
        }
    }

    /// Run the agent on the current message history.
    ///
    /// Sends all messages to the agent loop and appends the result.
    /// Checks the interrupt controller before running and clears it after.
    async fn run_agent(&mut self) -> Result<(), AgentError> {
        let run_started_at = Instant::now();
        self.maybe_autopin_contextlattice_topic_from_objective();
        Self::emit_phase_event(
            &self.stream_handle_shared,
            "preflight",
            "runtime preflight + credential hydration",
            5,
        );
        self.emit_contextlattice_connectivity_status().await;
        let provider = self.current_runtime_provider();
        let force_refresh = Self::should_force_preflight_auth_refresh(provider.as_str());
        self.refresh_runtime_provider_credentials_if_needed(force_refresh)
            .await;
        if force_refresh {
            Self::emit_lifecycle_event(
                &self.stream_handle_shared,
                format!("preflight auth refresh forced for provider {}", provider),
            );
        }
        if let Some(policy) = self.quorum_mode_armed_for_turn() {
            self.quorum_armed_once = false;
            self.clear_quorum_system_hints_inplace();
            self.interrupt_controller.clear_interrupt();
            match self.run_quorum_fanout_turn(run_started_at, policy).await {
                Ok(true) => return Ok(()),
                Ok(false) => {}
                Err(err) => return Err(err),
            }
        }
        Self::emit_phase_event(
            &self.stream_handle_shared,
            "dispatch",
            "dispatching model request",
            15,
        );
        self.interrupt_controller.clear_interrupt();
        let mut remediation_attempted = false;
        let mut auth_refresh_attempts = 0usize;
        let auth_refresh_retry_limit = Self::auth_refresh_retry_limit();
        let mut transient_retry_attempts = 0usize;
        let transient_retry_limit = Self::transient_retry_limit();
        let mut objective_continuation_attempts = 0usize;
        let objective_continuation_limit = Self::objective_continuation_retry_limit();
        loop {
            Self::emit_lifecycle_event(
                &self.stream_handle_shared,
                format!(
                    "dispatching request to {} (messages={})",
                    self.current_model,
                    self.messages.len()
                ),
            );
            Self::emit_phase_event(
                &self.stream_handle_shared,
                "inference",
                "model inference + tool execution",
                35,
            );
            let baseline_len = self.messages.len();
            let (messages, reformulated) = self.build_inference_messages();
            if reformulated {
                Self::emit_lifecycle_event(
                    &self.stream_handle_shared,
                    "runtime prompt reformulation injected (anti-scheming + context + tool routing + contradiction self-check)",
                );
            }
            let result = self.run_messages_with_current_agent(messages, true).await;

            match result {
                Ok(result) => {
                    let total_turns = result.total_turns;
                    let interrupted = result.interrupted;
                    let finished_naturally = result.finished_naturally;
                    if objective_continuation_attempts < objective_continuation_limit {
                        if let Some(reason) =
                            self.should_force_objective_continuation(&result, baseline_len)
                        {
                            self.messages = result.messages;
                            self.messages.push(hermes_core::Message::system(
                                Self::objective_continuation_system_prompt(&reason),
                            ));
                            self.prune_ui_after_current_messages();
                            objective_continuation_attempts += 1;
                            Self::emit_lifecycle_event(
                                &self.stream_handle_shared,
                                format!(
                                    "objective continuation enforcer triggered ({}/{}): {}",
                                    objective_continuation_attempts,
                                    objective_continuation_limit,
                                    reason
                                ),
                            );
                            Self::emit_phase_event(
                                &self.stream_handle_shared,
                                "objective",
                                "auto-continuing objective loop for concrete execution",
                                50,
                            );
                            continue;
                        }
                    }
                    if let Err(err) = self.apply_agent_result_and_persist(result) {
                        tracing::warn!("session autosave skipped: {}", err);
                    }
                    Self::emit_lifecycle_event(
                        &self.stream_handle_shared,
                        format!(
                            "run finished in {:.2}s (total_turns={})",
                            run_started_at.elapsed().as_secs_f64(),
                            total_turns
                        ),
                    );
                    Self::emit_phase_event(
                        &self.stream_handle_shared,
                        "finalize",
                        "transcript finalization + persistence",
                        100,
                    );
                    if let Some(handle) = &self.stream_handle {
                        handle.send_done();
                    }
                    if interrupted {
                        tracing::info!("Agent loop returned interrupted=true (graceful stop)");
                        if self.stream_handle.is_some() {
                            self.push_ui_assistant("[Agent execution interrupted]");
                        } else {
                            println!("[Agent execution interrupted]");
                        }
                    } else if !finished_naturally {
                        tracing::warn!(
                            "Agent stopped after {} turns (did not finish naturally)",
                            total_turns
                        );
                    }
                    break;
                }
                Err(AgentError::Interrupted { message }) => {
                    self.interrupt_controller.clear_interrupt();
                    Self::emit_lifecycle_event(
                        &self.stream_handle_shared,
                        format!(
                            "run interrupted after {:.2}s",
                            run_started_at.elapsed().as_secs_f64()
                        ),
                    );
                    if let Some(handle) = &self.stream_handle {
                        handle.send_done();
                    }
                    if let Some(redirect) = message {
                        tracing::info!("Agent interrupted with redirect: {}", redirect);
                    } else {
                        tracing::info!("Agent interrupted by user");
                    }
                    if self.stream_handle.is_some() {
                        self.push_ui_assistant("[Agent execution interrupted]");
                    } else {
                        println!("[Agent execution interrupted]");
                    }
                    break;
                }
                Err(e) => {
                    Self::emit_lifecycle_event(
                        &self.stream_handle_shared,
                        format!(
                            "run error after {:.2}s: {}",
                            run_started_at.elapsed().as_secs_f64(),
                            e
                        ),
                    );
                    Self::emit_phase_event(
                        &self.stream_handle_shared,
                        "recovery",
                        "error handling + remediation",
                        60,
                    );
                    if let Some(handle) = &self.stream_handle {
                        handle.send_done();
                    }
                    if Self::is_provider_auth_or_session_error(&e) {
                        if auth_refresh_attempts < auth_refresh_retry_limit {
                            if self.force_auth_refresh_after_error().await {
                                auth_refresh_attempts += 1;
                                Self::emit_lifecycle_event(
                                    &self.stream_handle_shared,
                                    format!(
                                        "auth refresh retry {}/{}",
                                        auth_refresh_attempts, auth_refresh_retry_limit
                                    ),
                                );
                                continue;
                            }
                        } else {
                            Self::emit_lifecycle_event(
                                &self.stream_handle_shared,
                                format!(
                                    "auth refresh retries exhausted ({})",
                                    auth_refresh_retry_limit
                                ),
                            );
                        }
                    }
                    if Self::is_transient_retryable_error(&e)
                        && transient_retry_attempts < transient_retry_limit
                    {
                        transient_retry_attempts += 1;
                        let backoff_ms = (transient_retry_attempts as u64)
                            .saturating_mul(1_000)
                            .max(800);
                        Self::emit_lifecycle_event(
                            &self.stream_handle_shared,
                            format!(
                                "transient runtime error retry {}/{} after {}ms: {}",
                                transient_retry_attempts, transient_retry_limit, backoff_ms, e
                            ),
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                        continue;
                    }
                    if !remediation_attempted {
                        if let Some((next_model, notice)) =
                            self.model_auto_remediation_target(&e).await
                        {
                            tracing::warn!(
                                "Model auto-remediation triggered: {} -> {}",
                                self.current_model,
                                next_model
                            );
                            if self.stream_handle.is_some() {
                                self.push_ui_assistant(notice.clone());
                            } else {
                                println!("{notice}");
                            }
                            Self::emit_lifecycle_event(
                                &self.stream_handle_shared,
                                format!("auto-remediation switching model to {}", next_model),
                            );
                            self.switch_model(&next_model);
                            remediation_attempted = true;
                            continue;
                        }
                    }
                    return Err(e);
                }
            }
        }

        Ok(())
    }

    /// Append a UI-only message anchored to the current conversation size.
    pub fn push_ui_message(&mut self, message: hermes_core::Message) {
        self.ui_messages.push(UiTranscriptMessage {
            insert_at: self.messages.len(),
            message,
        });
    }

    /// Append a UI-only user transcript line.
    pub fn push_ui_user(&mut self, text: impl Into<String>) {
        self.push_ui_message(hermes_core::Message::user(text.into()));
    }

    /// Append a UI-only assistant transcript line.
    pub fn push_ui_assistant(&mut self, text: impl Into<String>) {
        self.push_ui_message(hermes_core::Message::assistant(text.into()));
    }

    /// Build the merged transcript for TUI rendering.
    ///
    /// This includes durable conversation history and UI-only events in
    /// chronological order, while preserving model-facing context purity.
    pub fn transcript_messages(&self) -> Vec<hermes_core::Message> {
        let mut merged = Vec::with_capacity(self.messages.len() + self.ui_messages.len());
        for idx in 0..=self.messages.len() {
            for ui in self.ui_messages.iter().filter(|m| m.insert_at == idx) {
                merged.push(ui.message.clone());
            }
            if idx < self.messages.len() {
                merged.push(self.messages[idx].clone());
            }
        }
        merged
    }

    fn prune_ui_after_current_messages(&mut self) {
        let cap = self.messages.len();
        self.ui_messages.retain(|m| m.insert_at <= cap);
    }

    fn model_auto_remediation_enabled() -> bool {
        !matches!(
            std::env::var("HERMES_MODEL_AUTO_REMEDIATE")
                .ok()
                .as_deref()
                .map(|v| v.trim().to_ascii_lowercase()),
            Some(v) if matches!(v.as_str(), "0" | "false" | "off" | "no")
        )
    }

    fn is_model_not_found_error(err: &AgentError) -> bool {
        let message = match err {
            AgentError::LlmApi(msg)
            | AgentError::Config(msg)
            | AgentError::ToolExecution(msg)
            | AgentError::Gateway(msg) => msg.to_ascii_lowercase(),
            _ => return false,
        };
        let model_not_found = message.contains("model not found")
            || message.contains("requested model does not exist")
            || message.contains("404 not found")
            || message.contains("openrouter catalog");
        model_not_found && message.contains("model")
    }

    fn is_provider_auth_or_session_error(err: &AgentError) -> bool {
        let message = match err {
            AgentError::LlmApi(msg)
            | AgentError::Config(msg)
            | AgentError::ToolExecution(msg)
            | AgentError::Gateway(msg)
            | AgentError::AuthFailed(msg) => msg.to_ascii_lowercase(),
            _ => return false,
        };
        message.contains("401")
            || message.contains("403")
            || message.contains("unauthorized")
            || message.contains("invalid token")
            || message.contains("token_expired")
            || message.contains("expired_token")
            || message.contains("token expired")
            || message.contains("invalid_token")
            || message.contains("expired")
            || message.contains("authentication")
            || message.contains("session expired")
    }

    fn is_provider_tool_payload_error(err: &AgentError) -> bool {
        let message = match err {
            AgentError::LlmApi(msg)
            | AgentError::Config(msg)
            | AgentError::ToolExecution(msg)
            | AgentError::Gateway(msg)
            | AgentError::AuthFailed(msg) => msg.to_ascii_lowercase(),
            _ => return false,
        };
        let mentions_tool_payload =
            message.contains("tool") || message.contains("function") || message.contains("schema");
        let provider_payload_rejected = message.contains("provider returned error")
            && mentions_tool_payload
            && (message.contains("request is not valid")
                || message.contains("valid payload")
                || message.contains("check the model name")
                || message.contains("invalid"));
        let openai_shape_rejected = (message.contains("no choices in response")
            || message.contains("empty choices array"))
            && mentions_tool_payload
            && (message.contains("request is not valid")
                || message.contains("valid payload")
                || message.contains("provider returned error")
                || message.contains("invalid"));
        let explicit_tool_schema_rejected =
            message.contains("tool") && (message.contains("invalid") || message.contains("schema"));
        let strict_function_shape =
            message.contains("invalid input") && message.contains("function");
        provider_payload_rejected
            || openai_shape_rejected
            || explicit_tool_schema_rejected
            || strict_function_shape
            || (message.contains("422") && message.contains("valid payload"))
    }

    async fn force_auth_refresh_after_error(&mut self) -> bool {
        let (provider_name, _) = resolve_provider_and_model(&self.config, &self.current_model);
        let provider = normalize_runtime_provider_name(provider_name.as_str());
        let (notice, refreshed) = match provider.as_str() {
            "nous" => match resolve_nous_runtime_credentials(
                true,
                true,
                NOUS_ACCESS_TOKEN_REFRESH_SKEW_SECONDS,
                DEFAULT_NOUS_AGENT_KEY_MIN_TTL_SECONDS,
            )
            .await
            {
                Ok(creds) => {
                    let changed = Self::apply_nous_runtime_credentials(&creds);
                    if changed {
                        self.switch_model(&self.current_model.clone());
                    }
                    (
                        Some("Nous auth auto-refresh succeeded; retrying request.".to_string()),
                        true,
                    )
                }
                Err(err) => {
                    if Self::nous_refresh_contention_error(&err) {
                        match resolve_nous_runtime_credentials(
                            false,
                            true,
                            NOUS_ACCESS_TOKEN_REFRESH_SKEW_SECONDS,
                            DEFAULT_NOUS_AGENT_KEY_MIN_TTL_SECONDS,
                        )
                        .await
                        {
                            Ok(creds) => {
                                let changed = Self::apply_nous_runtime_credentials(&creds);
                                if changed {
                                    self.switch_model(&self.current_model.clone());
                                }
                                (
                                    Some(
                                        "Nous refresh busy; reused cached runtime credential and retrying request."
                                            .to_string(),
                                    ),
                                    true,
                                )
                            }
                            Err(cache_err) => (
                                Some(format!(
                                    "Nous cached credential hydration failed after refresh contention: {}",
                                    cache_err
                                )),
                                false,
                            ),
                        }
                    } else if Self::auth_error_requires_nous_login(&err)
                        && self
                            .attempt_interactive_nous_login("runtime auth refresh failed")
                            .await
                    {
                        match resolve_nous_runtime_credentials(
                            true,
                            true,
                            NOUS_ACCESS_TOKEN_REFRESH_SKEW_SECONDS,
                            DEFAULT_NOUS_AGENT_KEY_MIN_TTL_SECONDS,
                        )
                        .await
                        {
                            Ok(creds) => {
                                let changed = Self::apply_nous_runtime_credentials(&creds);
                                if changed {
                                    self.switch_model(&self.current_model.clone());
                                }
                                (
                                    Some(
                                        "Nous auth re-login succeeded; retrying request."
                                            .to_string(),
                                    ),
                                    true,
                                )
                            }
                            Err(retry_err) => (
                                Some(format!("Nous auth auto-refresh failed: {}", retry_err)),
                                false,
                            ),
                        }
                    } else {
                        (
                            Some(format!("Nous auth auto-refresh failed: {}", err)),
                            false,
                        )
                    }
                }
            },
            "qwen-oauth" => {
                match resolve_qwen_runtime_credentials(
                    true,
                    true,
                    QWEN_ACCESS_TOKEN_REFRESH_SKEW_SECONDS,
                )
                .await
                {
                    Ok(creds) => {
                        let mut changed = false;
                        changed |=
                            Self::set_env_if_changed("HERMES_QWEN_OAUTH_API_KEY", &creds.api_key);
                        changed |= Self::set_env_if_changed("DASHSCOPE_API_KEY", &creds.api_key);
                        if !creds.base_url.trim().is_empty() {
                            changed |=
                                Self::set_env_if_changed("HERMES_QWEN_BASE_URL", &creds.base_url);
                        }
                        if changed {
                            self.switch_model(&self.current_model.clone());
                        }
                        (
                            Some(
                                "Qwen OAuth auto-refresh succeeded; retrying request.".to_string(),
                            ),
                            true,
                        )
                    }
                    Err(err) => (
                        Some(format!("Qwen OAuth auto-refresh failed: {}", err)),
                        false,
                    ),
                }
            }
            "google-gemini-cli" | "gemini-cli" | "gemini-oauth" => {
                match resolve_gemini_oauth_runtime_credentials(true).await {
                    Ok(creds) => {
                        let mut changed = false;
                        changed |=
                            Self::set_env_if_changed("HERMES_GEMINI_OAUTH_API_KEY", &creds.api_key);
                        changed |= Self::set_env_if_changed("GOOGLE_API_KEY", &creds.api_key);
                        changed |= Self::set_env_if_changed("GEMINI_API_KEY", &creds.api_key);
                        if changed {
                            self.switch_model(&self.current_model.clone());
                        }
                        (
                            Some(
                                "Gemini OAuth auto-refresh succeeded; retrying request."
                                    .to_string(),
                            ),
                            true,
                        )
                    }
                    Err(err) => (
                        Some(format!("Gemini OAuth auto-refresh failed: {}", err)),
                        false,
                    ),
                }
            }
            _ => (None, false),
        };

        if let Some(text) = notice {
            Self::emit_lifecycle_event(&self.stream_handle_shared, &text);
            if self.stream_handle.is_some() {
                self.push_ui_assistant(text);
            } else {
                println!("{}", text);
            }
        }
        refreshed
    }

    async fn model_auto_remediation_target(&self, err: &AgentError) -> Option<(String, String)> {
        if !Self::model_auto_remediation_enabled() || !Self::is_model_not_found_error(err) {
            return None;
        }

        let (provider, current_model_id) = self
            .current_model
            .split_once(':')
            .unwrap_or(("openai", self.current_model.as_str()));
        let provider = provider.trim().to_ascii_lowercase();
        if provider.is_empty() {
            return None;
        }

        let catalog = provider_model_ids(&provider).await;
        if catalog.is_empty() {
            return None;
        }

        let selected = Self::resolve_quorum_catalog_candidate(current_model_id, &catalog)
            .or_else(|| catalog.first().cloned())?;

        let next_model = format!("{}:{}", provider, selected.trim());
        if next_model.eq_ignore_ascii_case(&self.current_model) {
            return None;
        }
        let close = Self::rank_catalog_candidates(current_model_id, &catalog, 3);
        let notice = format!(
            "Model catalog remediation: `{}` failed with not-found; switching to `{}` and retrying once. close matches: {}",
            self.current_model,
            next_model,
            if close.is_empty() {
                "(none)".to_string()
            } else {
                close.join(", ")
            }
        );
        Some((next_model, notice))
    }

    /// Navigate backward in input history.
    pub fn history_prev(&mut self) -> Option<&str> {
        if self.history_index > 0 {
            self.history_index -= 1;
            self.input_history
                .get(self.history_index)
                .map(|s| s.as_str())
        } else {
            None
        }
    }

    /// Navigate forward in input history.
    pub fn history_next(&mut self) -> Option<&str> {
        if self.history_index < self.input_history.len() {
            self.history_index += 1;
            if self.history_index < self.input_history.len() {
                self.input_history
                    .get(self.history_index)
                    .map(|s| s.as_str())
            } else {
                None
            }
        } else {
            None
        }
    }
}
