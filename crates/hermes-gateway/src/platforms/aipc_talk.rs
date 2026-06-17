//! AIPC Talk channel — WebSocket adapter for TTS-stream integration.
//!
//! Protocol (JSON over WebSocket text frames):
//!
//! Request:
//! ```json
//! {"request_id": "uuid", "text": "你好", "model": "?", "provider": "?"}
//! ```
//!
//! Immediate ack:
//! ```json
//! {"request_id": "uuid", "text": "queued", "status": "queued"}
//! ```
//!
//! Intermediate status (tool progress, lifecycle):
//! ```json
//! {"request_id": "uuid", "text": "searching web…", "status": "status"}
//! ```
//!
//! Final response (agent completes):
//! ```json
//! {"request_id": "uuid", "text": "agent reply", "status": "final"}
//! ```
//!
//! TTS client should only read aloud `status:"final"` messages.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use async_trait::async_trait;
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::{Notify, RwLock, mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::WebSocketStream;
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
    /// Pre-serialised JSON push frames (cron, async agent results).
    push_queue: Arc<RwLock<Vec<String>>>,
    push_notify: Arc<Notify>,

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
            push_queue: Arc::new(RwLock::new(Vec::new())),
            push_notify: Arc::new(Notify::new()),
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
        info!("Talk adapter starting on {}", self.config.bind_addr);

        let listener = TcpListener::bind(&self.config.bind_addr)
            .await
            .map_err(|e| {
                GatewayError::ConnectionFailed(format!(
                    "Talk adapter failed to bind {}: {e}",
                    self.config.bind_addr
                ))
            })?;

        info!(
            "Talk adapter WebSocket server listening on {}",
            self.config.bind_addr
        );

        let pending = self.pending.clone();
        let inbound_tx = self.inbound_tx.clone();
        let push_queue = self.push_queue.clone();
        let push_notify = self.push_notify.clone();
        let _shutdown = self.shutdown_notify.clone();

        let (shutdown_sender, mut shutdown_receiver) = oneshot::channel::<()>();
        *self.shutdown_tx.write().await = Some(shutdown_sender);

        self.running
            .store(true, std::sync::atomic::Ordering::SeqCst);

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    accept = listener.accept() => {
                        match accept {
                            Ok((stream, peer)) => {
                                debug!("Talk WS connection from {peer}");
                                let pending = pending.clone();
                                let inbound_tx = inbound_tx.clone();
                                let push_queue = push_queue.clone();
                                let push_notify = push_notify.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = handle_talk_connection(
                                        stream, peer, pending, inbound_tx,
                                        push_queue, push_notify,
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
        self.running
            .store(false, std::sync::atomic::Ordering::SeqCst);
        self.shutdown_notify.notify_one();
        Ok(())
    }

    async fn send_message(
        &self,
        chat_id: &str,
        text: &str,
        _parse_mode: Option<ParseMode>,
    ) -> Result<(), GatewayError> {
        debug!(chat_id = chat_id, text_len = text.len(), "Talk send_message: status");
        let json = make_response(chat_id, text, "status");
        self.push_queue.write().await.push(json);
        self.push_notify.notify_one();
        Ok(())
    }

    async fn send_message_in_thread(
        &self,
        chat_id: &str,
        text: &str,
        _parse_mode: Option<ParseMode>,
        _reply_to_message_id: Option<&str>,
        _message_thread_id: Option<&str>,
    ) -> Result<Option<String>, GatewayError> {
        debug!(chat_id = chat_id, text_len = text.len(), "Talk send_message_in_thread: final");
        let json = make_response(chat_id, text, "final");
        self.push_queue.write().await.push(json);
        self.push_notify.notify_one();
        // Consume oneshot (if any) to signal background task for cleanup
        let mut pending = self.pending.write().await;
        if let Some(tx) = pending.remove(chat_id) {
            let _ = tx.send(text.to_string());
        }
        Ok(None)
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
// Helpers
// ---------------------------------------------------------------------------

fn make_response(request_id: &str, text: &str, status: &str) -> String {
    let response = TalkResponse {
        request_id: request_id.to_string(),
        text: text.to_string(),
        status: status.to_string(),
    };
    serde_json::to_string(&response).unwrap_or_else(|_| {
        format!(
            r#"{{"request_id":"{}","text":"serialization error","status":"error"}}"#,
            request_id
        )
    })
}

async fn flush_push_queue(
    queue: &RwLock<Vec<String>>,
    ws: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    peer: std::net::SocketAddr,
) -> Result<(), GatewayError> {
    let mut q = queue.write().await;
    while let Some(json) = q.pop() {
        if let Err(e) = ws.send(WsMessage::Text(json.into())).await {
            warn!("Talk failed to send push to {peer}: {e}");
            return Err(GatewayError::ConnectionFailed(format!("ws send: {e}")));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// WebSocket connection handler
// ---------------------------------------------------------------------------

async fn handle_talk_connection(
    stream: tokio::net::TcpStream,
    peer: std::net::SocketAddr,
    pending: Arc<RwLock<HashMap<String, oneshot::Sender<String>>>>,
    inbound_tx: Arc<RwLock<Option<mpsc::Sender<IncomingMessage>>>>,
    push_queue: Arc<RwLock<Vec<String>>>,
    push_notify: Arc<Notify>,
) -> Result<(), GatewayError> {
    let mut ws_stream = tokio_tungstenite::accept_async(stream)
        .await
        .map_err(|e| GatewayError::ConnectionFailed(format!("WebSocket upgrade failed: {e}")))?;

    // Drain any push messages queued before this client connected
    {
        let q = push_queue.read().await;
        if !q.is_empty() {
            info!(peer = %peer, queued_count = q.len(), "Talk drain-at-connect: sending queued push messages");
        }
    }
    flush_push_queue(&push_queue, &mut ws_stream, peer).await?;

    loop {
        tokio::select! {
            biased;

            _ = push_notify.notified() => {
                flush_push_queue(&push_queue, &mut ws_stream, peer).await?;
            }

            msg = ws_stream.next() => {
                let msg = match msg {
                    Some(Ok(m)) => m,
                    Some(Err(e)) => {
                        warn!("Talk WS read error from {peer}: {e}");
                        break;
                    }
                    None => break,
                };

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

                let request_id = request.request_id.clone();

                // 1. Send immediate ack — client knows the request was accepted
                let ack_json = make_response(&request_id, "queued", "queued");
                if let Err(e) = ws_stream.send(WsMessage::Text(ack_json.into())).await {
                    warn!("Talk failed to send queued ack to {peer}: {e}");
                    break;
                }

                // 2. Register oneshot so the agent's send_message can resolve it
                let (reply_tx, reply_rx) = oneshot::channel::<String>();
                {
                    let mut p = pending.write().await;
                    p.insert(request_id.clone(), reply_tx);
                }

                let incoming = IncomingMessage::new(
                    "aipc_talk", &request_id, "talk-client", &request.text, true,
                );

                // 3. Dispatch to gateway pipeline
                {
                    let tx_guard = inbound_tx.read().await;
                    if let Some(tx) = tx_guard.as_ref() {
                        if let Err(e) = tx.send(incoming).await {
                            error!("Talk failed to send inbound message: {e}");
                            pending.write().await.remove(&request_id);
                            let err_json = make_response(
                                &request_id,
                                &format!("failed to dispatch: {e}"),
                                "error",
                            );
                            let _ = ws_stream.send(WsMessage::Text(err_json.into())).await;
                            continue;
                        }
                    } else {
                        warn!("Talk inbound sender not set");
                        pending.write().await.remove(&request_id);
                        let err_json = make_response(
                            &request_id,
                            "gateway not ready",
                            "error",
                        );
                        let _ = ws_stream.send(WsMessage::Text(err_json.into())).await;
                        continue;
                    }
                }

                // 4. Spawn background task: wait for send_message_in_thread to
                //    consume the oneshot, then send error if it never fired.
                let pending_clone = pending.clone();
                let pq = push_queue.clone();
                let pn = push_notify.clone();
                tokio::spawn(async move {
                    match reply_rx.await {
                        Ok(_) => {
                            // Final response already sent by send_message_in_thread
                        }
                        Err(_) => {
                            let json = make_response(
                                &request_id,
                                "agent did not respond in time",
                                "error",
                            );
                            pq.write().await.push(json);
                        }
                    }
                    pending_clone.write().await.remove(&request_id);
                    pn.notify_one();
                });
            }
        }
    }

    Ok(())
}
