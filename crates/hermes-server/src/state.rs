use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

use hermes_config::GatewayConfig;
use serde::{Deserialize, Serialize};

use crate::{
    core::session::SessionState,
    rpc::interaction::PendingInteractions,
};

/// Handoff state for session transfer to messaging platforms.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffState {
    pub state: String,
    pub platform: String,
    pub started_at: f64,
    pub completed_at: Option<f64>,
    pub error: Option<String>,
}

/// Global application state shared across all handlers.
#[derive(Clone)]
pub struct AppState {
    /// Runtime gateway configuration.
    pub config: Arc<RwLock<GatewayConfig>>,
    
    /// Path to HERMES_HOME directory.
    pub hermes_home: std::path::PathBuf,
    
    /// Session token for authentication.
    pub session_token: String,
    
    /// Active sessions (sid -> SessionState).
    pub sessions: Arc<RwLock<HashMap<String, SessionState>>>,
    
    /// Current active profile name.
    pub active_profile: Arc<RwLock<String>>,
    
    /// Pending interaction requests (approval, clarify, sudo, secret).
    pub pending_interactions: PendingInteractions,

    /// Broadcast channel for server-sent events.
    pub event_broadcast: broadcast::Sender<serde_json::Value>,

    /// Global tool registry (shared across all sessions).
    pub tool_registry: Arc<hermes_agent::ToolRegistry>,

    /// Path to cron data directory.
    pub cron_data_dir: std::path::PathBuf,

    /// Handoff states for session transfers to messaging platforms.
    pub handoff_states: Arc<RwLock<HashMap<String, HandoffState>>>,
}

impl AppState {
    pub fn new(config: GatewayConfig, hermes_home: std::path::PathBuf) -> Self {
        // Priority: HERMES_DASHBOARD_SESSION_TOKEN env var > auto-generated
        let session_token = std::env::var("HERMES_DASHBOARD_SESSION_TOKEN")
            .unwrap_or_else(|_| generate_session_token());
        let (event_broadcast, _) = broadcast::channel(256);
        let cron_data_dir = hermes_home.join("cron");
        std::fs::create_dir_all(&cron_data_dir).ok();
        Self {
            config: Arc::new(RwLock::new(config)),
            hermes_home,
            session_token,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            active_profile: Arc::new(RwLock::new("default".to_string())),
            pending_interactions: crate::rpc::interaction::new_pending_interactions(),
            event_broadcast,
            tool_registry: Arc::new(hermes_agent::ToolRegistry::new()),
            cron_data_dir,
            handoff_states: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Returns the current session token (for loopback auth).
    pub fn session_token(&self) -> &str {
        &self.session_token
    }
    
    /// Returns the current active profile name.
    pub async fn active_profile_name(&self) -> String {
        self.active_profile.read().await.clone()
    }
    
    /// Returns the path to the config.yaml file for the active profile.
    pub fn config_path(&self) -> std::path::PathBuf {
        self.profile_home(None).join("config.yaml")
    }
    
    /// Returns the path to the .env file for the active profile.
    pub fn env_path(&self) -> std::path::PathBuf {
        self.profile_home(None).join(".env")
    }
    
    /// Returns the home directory for the given profile.
    pub fn profile_home(&self, profile: Option<&str>) -> std::path::PathBuf {
        match profile {
            Some(name) if name != "default" => {
                self.hermes_home.join("profiles").join(name)
            }
            _ => self.hermes_home.clone(),
        }
    }
    
    /// Returns a SessionPersistence instance for the given profile.
    pub fn profile_persistence(
        &self,
        profile: Option<&str>,
    ) -> Result<hermes_agent::session_persistence::SessionPersistence, hermes_core::AgentError> {
        let home = self.profile_home(profile);
        if !home.exists() {
            std::fs::create_dir_all(&home)
                .map_err(|e| hermes_core::AgentError::Io(format!("create profile dir: {}", e)))?;
        }
        Ok(hermes_agent::session_persistence::SessionPersistence::new(&home))
    }
    
    /// Returns a SessionPersistence instance for the active profile.
    pub fn session_persistence(&self) -> Result<hermes_agent::session_persistence::SessionPersistence, hermes_core::AgentError> {
        // Use default profile path for backward compatibility
        // Profile-aware code should use profile_persistence() with explicit profile name
        Ok(hermes_agent::session_persistence::SessionPersistence::new(&self.hermes_home))
    }
    
    /// Invalidate all cached AgentLoop instances so they are rebuilt
    /// with the latest config on the next prompt.submit.
    pub async fn invalidate_agent_caches(&self) {
        let sessions = self.sessions.read().await;
        for (_, session) in sessions.iter() {
            if let Ok(mut guard) = session.agent.lock() {
                *guard = None;
            }
        }
        tracing::info!("Invalidated agent caches for all sessions");
    }

    /// Subscribe to the event broadcast channel.
    pub fn subscribe_events(&self) -> broadcast::Receiver<serde_json::Value> {
        self.event_broadcast.subscribe()
    }

    /// Publish an event to all subscribers.
    pub fn publish_event(&self, event: serde_json::Value) {
        let _ = self.event_broadcast.send(event);
    }
}

/// Generate a random session token for authentication.
fn generate_session_token() -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                             abcdefghijklmnopqrstuvwxyz\
                             0123456789";
    let mut rng = rand::thread_rng();
    (0..32)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}
