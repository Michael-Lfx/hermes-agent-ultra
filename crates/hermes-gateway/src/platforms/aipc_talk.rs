//! AIPC Talk channel — WebSocket adapter for TTS-stream integration.
//!
//! Exposes a WebSocket server that receives JSON text queries from
//! `/home/leeyang/tts-stream` (or any WS client), routes them through
//! the full Gateway agent pipeline, and returns the plain response.
//!
//! Protocol (JSON over WebSocket text frames):
//!
//! Request:
//! ```json
//! {"request_id": "uuid", "text": "你好", "model": "?", "provider": "?"}
//! ```
//!
//! Response:
//! ```json
//! {"request_id": "uuid", "text": "agent reply", "status": "ok"}
//! ```

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use async_trait::async_trait;
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::{Notify, RwLock, mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tracing::{debug, error, info, warn};

use hermes_core::errors::GatewayError;
use hermes_core::traits::{ParseMode, PlatformAdapter};

use crate::gateway::IncomingMessage;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TalkConfig {
    #[serde(default = "default_talk_bind")]
    pub bind_addr: String,
}

fn default_talk_bind() -> String {
    "127.0.0.1:9100".to_string()
}

impl Default for TalkConfig {
    fn default() -> Self {
        Self {
            bind_addr: default_talk_bind(),
        }
    }
}

// ---------------------------------------------------------------------------
// JSON protocol types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct TalkRequest {
    pub request_id: String,
    pub text: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TalkResponse {
    pub request_id: String,
    pub text: String,
    pub status: String,
}

// ---------------------------------------------------------------------------
// TalkAdapter
// ---------------------------------------------------------------------------

pub struct TalkAdapter {
    config: TalkConfig,
    running: AtomicBool,
    pending: Arc<RwLock<HashMap<String, oneshot::Sender<String>>>>,
    inbound_tx: Arc<RwLock<Option<mpsc::Sender<IncomingMessage>>>>,
    shutdown_tx: RwLock<Option<oneshot::Sender<()>>>,
    shutdown_notify: Arc<Notify>,
}

impl TalkAdapter {
    pub fn new(config: TalkConfig) -> Self {
        Self {
            config,
            running: AtomicBool::new(false),
            pending: Arc::new(RwLock::new(HashMap::new())),
            inbound_tx: Arc::new(RwLock::new(None)),
            shutdown_tx: RwLock::new(None),
            shutdown_notify: Arc::new(Notify::new()),
        }
    }

    pub fn config(&self) -> &TalkConfig {
        &self.config
    }

    pub async fn set_inbound_sender(&self, tx: mpsc::Sender<IncomingMessage>) {
        *self.inbound_tx.write().await = Some(tx);
    }
}

#[async_trait]
impl PlatformAdapter for TalkAdapter {
    async fn start(&self) -> Result<(), GatewayError> {
        info!(
            "Talk adapter starting on {}",
            self.config.bind_addr
        );

        let listener = TcpListener::bind(&self.config.bind_addr)
            .await
            .map_err(|e| {
                GatewayError::ConnectionFailed(format!(
                    "Talk adapter failed to bind {}: {e}",
                    self.config.bind_addr
                ))
            })?;

        info!("Talk adapter WebSocket server listening on {}", self.config.bind_addr);

        let pending = self.pending.clone();
        let inbound_tx = self.inbound_tx.clone();
        let _shutdown = self.shutdown_notify.clone();

        let (shutdown_sender, mut shutdown_receiver) = oneshot::channel::<()>();
        *self.shutdown_tx.write().await = Some(shutdown_sender);

        self.running.store(true, std::sync::atomic::Ordering::SeqCst);

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    accept = listener.accept() => {
                        match accept {
                            Ok((stream, peer)) => {
                                debug!("Talk WS connection from {peer}");
                                let pending = pending.clone();
                                let inbound_tx = inbound_tx.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = handle_talk_connection(
                                        stream, peer, pending, inbound_tx,
                                    )
                                    .await
                                    {
                                        debug!("Talk WS error from {peer}: {e}");
                                    }
                                });
                            }
                            Err(e) => {
                                warn!("Talk adapter accept error: {e}");
                            }
                        }
                    }
                    _ = &mut shutdown_receiver => {
                        info!("Talk adapter shutting down");
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    async fn stop(&self) -> Result<(), GatewayError> {
        info!("Talk adapter stopping");
        if let Some(tx) = self.shutdown_tx.write().await.take() {
            let _ = tx.send(());
        }
        self.running.store(false, std::sync::atomic::Ordering::SeqCst);
        self.shutdown_notify.notify_one();
        Ok(())
    }

    async fn send_message(
        &self,
        chat_id: &str,
        text: &str,
        _parse_mode: Option<ParseMode>,
    ) -> Result<(), GatewayError> {
        let mut pending = self.pending.write().await;
        if let Some(tx) = pending.remove(chat_id) {
            let _ = tx.send(text.to_string());
        } else {
            debug!(chat_id = chat_id, "No pending talk request for chat_id");
        }
        Ok(())
    }

    async fn edit_message(
        &self,
        _chat_id: &str,
        _message_id: &str,
        _text: &str,
    ) -> Result<(), GatewayError> {
        Ok(())
    }

    async fn send_file(
        &self,
        _chat_id: &str,
        _file_path: &str,
        _caption: Option<&str>,
    ) -> Result<(), GatewayError> {
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.running.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn platform_name(&self) -> &str {
        "aipc_talk"
    }
}

// ---------------------------------------------------------------------------
// WebSocket connection handler
// ---------------------------------------------------------------------------

async fn handle_talk_connection(
    stream: tokio::net::TcpStream,
    peer: std::net::SocketAddr,
    pending: Arc<RwLock<HashMap<String, oneshot::Sender<String>>>>,
    inbound_tx: Arc<RwLock<Option<mpsc::Sender<IncomingMessage>>>>,
) -> Result<(), GatewayError> {
    let mut ws_stream = tokio_tungstenite::accept_async(stream)
        .await
        .map_err(|e| GatewayError::ConnectionFailed(format!("WebSocket upgrade failed: {e}")))?;

    while let Some(msg) = ws_stream.next().await {
        let msg = msg.map_err(|e| {
            GatewayError::ConnectionFailed(format!("WebSocket read error: {e}"))
        })?;

        let text = match msg {
            WsMessage::Text(t) => t,
            WsMessage::Close(_) => break,
            WsMessage::Ping(data) => {
                let _ = ws_stream.send(WsMessage::Pong(data)).await;
                continue;
            }
            _ => continue,
        };

        debug!(%text, "Talk received WS message from {peer}");

        let request: TalkRequest = match serde_json::from_str(&text) {
            Ok(req) => req,
            Err(e) => {
                warn!("Talk invalid JSON from {peer}: {e}");
                continue;
            }
        };

        let chat_id = request.request_id.clone();

        let (reply_tx, reply_rx) = oneshot::channel::<String>();
        {
            let mut pending = pending.write().await;
            pending.insert(chat_id.clone(), reply_tx);
        }

        let incoming = IncomingMessage::new("aipc_talk", &chat_id, "talk-client", &request.text, true);

        {
            let tx_guard = inbound_tx.read().await;
            if let Some(tx) = tx_guard.as_ref() {
                if let Err(e) = tx.send(incoming).await {
                    error!("Talk failed to send inbound message: {e}");
                    let mut pending = pending.write().await;
                    pending.remove(&chat_id);
                    continue;
                }
            } else {
                warn!("Talk inbound sender not set");
                let mut pending = pending.write().await;
                pending.remove(&chat_id);
                continue;
            }
        }

        let status;
        let reply_text;
        match reply_rx.await {
            Ok(text) => {
                reply_text = text;
                status = "ok";
            }
            Err(_) => {
                reply_text = "agent did not respond in time".to_string();
                status = "error";
            }
        }

        {
            let mut pending = pending.write().await;
            pending.remove(&chat_id);
        }

        let response = TalkResponse {
            request_id: request.request_id.clone(),
            text: reply_text,
            status: status.to_string(),
        };

        let request_id = request.request_id;
        let response_json = serde_json::to_string(&response).unwrap_or_else(|_| {
            format!(
                r#"{{"request_id":"{}","text":"serialization error","status":"error"}}"#,
                request_id
            )
        });

        if let Err(e) = ws_stream.send(WsMessage::Text(response_json.into())).await {
            warn!("Talk failed to send WS response to {peer}: {e}");
            break;
        }
    }

    Ok(())
}
