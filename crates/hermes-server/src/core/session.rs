use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;

use crate::ws::transport::ReplaceableTransport;

/// Session state for an active conversation.
pub struct SessionState {
    /// Unique session ID (short uuid, e.g. "abc123").
    pub session_id: String,
    
    /// Persistent session key (e.g. "20260610_150638_5d6ea6").
    pub session_key: String,
    
    /// Current transport for sending events to client.
    pub transport: ReplaceableTransport,
    
    /// Current working directory.
    pub cwd: PathBuf,
    
    /// Terminal columns.
    pub cols: u32,
    
    /// Whether the session is currently running an agent turn.
    pub running: AtomicBool,
    
    /// Whether the agent is being built (lazy initialization).
    pub agent_building: AtomicBool,
    
    /// Notification for agent ready.
    pub agent_ready: Notify,
    
    /// History version for optimistic concurrency control.
    pub history_version: AtomicU64,
    
    /// Whether to close on disconnect.
    pub close_on_disconnect: bool,
    
    /// Profile name (if multi-profile).
    pub profile: Option<String>,
    
    /// Lazy-built AgentLoop for this session.
    pub agent: Mutex<Option<Arc<hermes_agent::AgentLoop>>>,
    
    /// Steering queue for real-time user guidance during agent generation.
    pub steer_queue: Mutex<Vec<String>>,
}

impl SessionState {
    pub fn new(
        session_id: String,
        session_key: String,
        transport: ReplaceableTransport,
        cwd: PathBuf,
        cols: u32,
    ) -> Self {
        Self {
            session_id,
            session_key,
            transport,
            cwd,
            cols,
            running: AtomicBool::new(false),
            agent_building: AtomicBool::new(false),
            agent_ready: Notify::new(),
            history_version: AtomicU64::new(0),
            close_on_disconnect: false,
            profile: None,
            agent: Mutex::new(None),
            steer_queue: Mutex::new(Vec::new()),
        }
    }
    
    /// Get or build the AgentLoop for this session.
    pub fn get_or_build_agent(
        &self,
        config: &hermes_config::GatewayConfig,
        hermes_home: &std::path::Path,
        pending: &crate::rpc::interaction::PendingInteractions,
        tool_registry: Arc<hermes_agent::ToolRegistry>,
        tools_registry: Arc<hermes_tools::ToolRegistry>,
    ) -> Option<Arc<hermes_agent::AgentLoop>> {
        // Fast path: already built
        if let Ok(guard) = self.agent.lock() {
            if let Some(ref agent) = *guard {
                return Some(agent.clone());
            }
        }
        
        // Build agent with transport bound for Desktop events
        let agent = crate::core::agent_builder::build_agent(
            config,
            &self.session_id,
            &self.session_key,
            hermes_home,
            self.transport.clone(),
            pending,
            tool_registry,
            tools_registry,
        )?;
        
        let agent_arc = Arc::new(agent);
        if let Ok(mut guard) = self.agent.lock() {
            *guard = Some(agent_arc.clone());
        }
        
        Some(agent_arc)
    }
    
    /// Mark the session as running.
    pub fn set_running(&self, value: bool) {
        self.running.store(value, Ordering::SeqCst);
    }
    
    /// Check if the session is running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
    
    /// Increment history version.
    pub fn bump_history_version(&self) -> u64 {
        self.history_version.fetch_add(1, Ordering::SeqCst) + 1
    }
    
    /// Push a steer text to the queue.
    pub fn push_steer(&self, text: String) {
        if let Ok(mut guard) = self.steer_queue.lock() {
            guard.push(text);
        }
    }
    
    /// Drain all steer texts from the queue.
    pub fn drain_steer(&self) -> Vec<String> {
        self.steer_queue
            .lock()
            .map(|mut g| g.drain(..).collect())
            .unwrap_or_default()
    }
}
