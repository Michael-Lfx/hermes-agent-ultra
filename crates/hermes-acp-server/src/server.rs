//! ACP Pipe Server -- accept loop + multi-connection management.
//!
//! # Shutdown
//!
//! Calling `shutdown()` from outside a tokio context will panic because it
//! uses `tokio::spawn` for the poke-listener. Always call from within an
//! async runtime (e.g. a slash command handler running on the main tokio
//! task).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::Notify;
use tracing::{debug, error, info, warn};

use crate::connection::{AcpConnection, AgentInfo, ConnectionMetaCb};
use crate::executor::PromptExecutor;
use crate::platform;
use crate::session::MetaUpdate;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// ACP Pipe Server configuration.
pub struct AcpServerConfig {
    /// IPC endpoint path.
    pub pipe_path: String,
    /// Maximum concurrent connections (default: 5).
    pub max_connections: usize,
    /// Prompt execution timeout in seconds (default: 300).
    pub prompt_timeout_secs: u64,
    /// Agent brand information.
    pub agent_info: AgentInfo,
    /// Prompt executor.
    pub executor: Arc<dyn PromptExecutor>,
}

impl std::fmt::Debug for AcpServerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AcpServerConfig")
            .field("pipe_path", &self.pipe_path)
            .field("max_connections", &self.max_connections)
            .field("prompt_timeout_secs", &self.prompt_timeout_secs)
            .field("agent_info", &self.agent_info)
            .finish_non_exhaustive()
    }
}

// ---------------------------------------------------------------------------
// Connection info
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    pub id: String,
    pub client_name: Option<String>,
    pub client_title: Option<String>,
    pub session_id: Option<String>,
    pub is_cherry: bool,
}

// ---------------------------------------------------------------------------
// Server error
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum AcpServerError {
    Io(std::io::Error),
    Platform(crate::platform::IpcError),
}

impl std::fmt::Display for AcpServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AcpServerError::Io(e) => write!(f, "I/O error: {}", e),
            AcpServerError::Platform(e) => write!(f, "IPC error: {}", e),
        }
    }
}

impl std::error::Error for AcpServerError {}

impl From<std::io::Error> for AcpServerError {
    fn from(e: std::io::Error) -> Self { AcpServerError::Io(e) }
}
impl From<crate::platform::IpcError> for AcpServerError {
    fn from(e: crate::platform::IpcError) -> Self { AcpServerError::Platform(e) }
}

// ---------------------------------------------------------------------------
// AcpPipeServer
// ---------------------------------------------------------------------------

/// Multi-client ACP Pipe Server.
pub struct AcpPipeServer {
    config: AcpServerConfig,
    shutdown: Arc<AtomicBool>,
    shutdown_notify: Arc<Notify>,
    // SAFETY: This Mutex is std::sync (blocking). All lock sites are very
    // short (single HashMap op) and never held across .await points.
    // Do NOT introduce an .await while holding this lock.
    connections: Arc<Mutex<HashMap<String, ConnectionInfo>>>,
}

impl AcpPipeServer {
    pub fn new(config: AcpServerConfig) -> Result<Self, AcpServerError> {
        Ok(Self {
            config,
            shutdown: Arc::new(AtomicBool::new(false)),
            shutdown_notify: Arc::new(Notify::new()),
            connections: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Start the accept loop. Blocks until shutdown.
    pub async fn run(&self) -> Result<(), AcpServerError> {
        let listener = platform::create_listener(
            &self.config.pipe_path,
            self.config.max_connections,
        )?;

        info!(
            endpoint = %listener.endpoint(),
            max = self.config.max_connections,
            "ACP server listening"
        );

        loop {
            let accept_result = tokio::select! {
                r = listener.accept() => r,
                _ = self.shutdown_notify.notified() => {
                    info!("ACP server shutdown via notify");
                    break;
                }
            };

            if self.shutdown.load(Ordering::Acquire) {
                info!("ACP server shutting down");
                break;
            }

            match accept_result {
                Ok(stream) => {
                    let conn_count = self.connections.lock().unwrap().len();
                    if conn_count >= self.config.max_connections {
                        warn!(current = conn_count, max = self.config.max_connections, "rejecting connection");
                        continue;
                    }

                    let conn_id = uuid::Uuid::new_v4().to_string();

                    let conns = self.connections.clone();
                    let conn_id_cb = conn_id.clone();
                    let meta_cb: ConnectionMetaCb = Arc::new(move |id: String, update: MetaUpdate| {
                        if let Some(info) = conns.lock().unwrap().get_mut(&id) {
                            info.client_name = update.client_name;
                            info.client_title = update.client_title;
                            info.session_id = update.session_id;
                            info.is_cherry = info.client_name.as_deref() == Some("ai-cherry");
                        }
                    });

                    let conn = AcpConnection::new(
                        conn_id.clone(),
                        self.config.agent_info.clone(),
                        self.config.executor.clone(),
                    ).with_meta_cb(meta_cb)
                        .with_timeout(self.config.prompt_timeout_secs);

                    let info = ConnectionInfo {
                        id: conn_id.clone(),
                        client_name: None,
                        client_title: None,
                        session_id: None,
                        is_cherry: false,
                    };
                    self.connections.lock().unwrap().insert(conn_id.clone(), info);

                    let conns = self.connections.clone();

                    tokio::spawn(async move {
                        conn.run(stream).await;
                        conns.lock().unwrap().remove(&conn_id_cb);
                        debug!(conn_id = %conn_id_cb, "connection removed from map");
                    });
                }
                Err(e) => {
                    if self.shutdown.load(Ordering::Acquire) {
                        break;
                    }
                    error!(error = %e, "accept error");
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            }
        }

        self.connections.lock().unwrap().clear();
        #[cfg(unix)]
        if self.config.pipe_path.starts_with('/') {
            let _ = std::fs::remove_file(&self.config.pipe_path);
        }

        info!("ACP server stopped");
        Ok(())
    }

    /// Request graceful shutdown.
    ///
    /// **Panics** if called outside a tokio runtime (uses `tokio::spawn`).
    pub fn shutdown(&self) {
        if self.shutdown.swap(true, Ordering::Release) {
            return;
        }
        info!("ACP server shutdown requested");

        self.shutdown_notify.notify_waiters();
        let pipe_path = self.config.pipe_path.clone();
        tokio::spawn(async move {
            platform::poke_listener(&pipe_path).await;
        });
    }

    pub fn is_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::Acquire)
    }

    pub fn max_connections(&self) -> usize {
        self.config.max_connections
    }

    pub fn connection_count(&self) -> usize {
        self.connections.lock().unwrap().len()
    }

    pub fn has_cherry_client(&self) -> bool {
        self.connections.lock().unwrap().values().any(|c| c.is_cherry)
    }

    pub fn connections(&self) -> Vec<ConnectionInfo> {
        self.connections.lock().unwrap().values().cloned().collect()
    }

    pub fn endpoint(&self) -> &str {
        &self.config.pipe_path
    }
}
