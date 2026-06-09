//! Single ACP connection state machine + NDJSON dispatch.
//!
//! Each client connection gets its own AcpConnection that drives the
//! full ACP protocol lifecycle: initialize -> session/new -> prompt -> streaming.
//!
//! Streaming architecture: the prompt executor runs inline (awaited in the
//! read loop). Events accumulate in an mpsc channel (capacity 256) during
//! execution. After the executor completes, `bridge_events` drains the
//! channel and writes each event as an NDJSON session/update notification.
//! For short responses this is effectively a single flush; for responses
//! that overflow the channel buffer, the executor blocks until the bridge
//! catches up, providing natural backpressure.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use crate::event_bridge::bridge_events;
use crate::executor::{PromptExecutor, StreamEvent};
use crate::ndjson::{NdjsonReader, NdjsonWriter};
use crate::platform::IpcStream;
use crate::session::{MetaUpdate, PipeSession};

use hermes_acp::protocol::{AcpRequest, AcpResponse, StopReason};

// ---------------------------------------------------------------------------
// ACP method constants
// ---------------------------------------------------------------------------

const METHOD_INITIALIZE: &str = "initialize";
const METHOD_AUTHENTICATE: &str = "authenticate";
const METHOD_SESSION_NEW: &str = "session/new";
const METHOD_SESSION_PROMPT: &str = "session/prompt";
const METHOD_SESSION_CANCEL: &str = "session/cancel";
const METHOD_SESSION_PING: &str = "session/ping";
const METHOD_SESSION_SET_MODE: &str = "session/set_mode";
const METHOD_CHERRY_SHUTDOWN: &str = "cherry/shutdown";

// ---------------------------------------------------------------------------
// Connection state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnectionState {
    Connected,
    Initialized,
    SessionReady,
    Active,
}

// ---------------------------------------------------------------------------
// Agent info for initialize response
// ---------------------------------------------------------------------------

/// Agent brand information returned in initialize response.
#[derive(Debug, Clone)]
pub struct AgentInfo {
    pub name: String,
    pub title: String,
    pub version: String,
}

// ---------------------------------------------------------------------------
// Callback for connection metadata updates (written back to server map).
// ---------------------------------------------------------------------------

/// Callback invoked when client metadata (client_name, session_id) changes.
/// The server uses this to keep its ConnectionInfo snapshot up to date.
pub type ConnectionMetaCb = Arc<dyn Fn(String, MetaUpdate) + Send + Sync>;

// ---------------------------------------------------------------------------
// AcpConnection
// ---------------------------------------------------------------------------

/// Manages one ACP client connection.
pub struct AcpConnection {
    id: String,
    state: ConnectionState,
    session: Option<PipeSession>,
    agent_info: AgentInfo,
    executor: Arc<dyn PromptExecutor>,
    prompt_active: Arc<AtomicBool>,
    cancel_flag: Arc<AtomicBool>,
    meta_cb: Option<ConnectionMetaCb>,
    prompt_timeout_secs: u64,
}

impl AcpConnection {
    pub fn new(id: String, agent_info: AgentInfo, executor: Arc<dyn PromptExecutor>) -> Self {
        Self {
            id,
            state: ConnectionState::Connected,
            session: None,
            agent_info,
            executor,
            prompt_active: Arc::new(AtomicBool::new(false)),
            cancel_flag: Arc::new(AtomicBool::new(false)),
            meta_cb: None,
            prompt_timeout_secs: 300,
        }
    }

    /// Attach a metadata callback so the connection can push updates
    /// back to the server-level ConnectionInfo map.
    pub fn with_meta_cb(mut self, cb: ConnectionMetaCb) -> Self {
        self.meta_cb = Some(cb);
        self
    }

    /// Set the prompt execution timeout (seconds).
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.prompt_timeout_secs = secs;
        self
    }

    pub fn id(&self) -> &str { &self.id }

    pub fn client_name(&self) -> Option<&str> {
        self.session.as_ref().and_then(|s| s.client_name.as_deref())
    }

    pub fn is_cherry(&self) -> bool {
        self.client_name().map(|n| n == "ai-cherry").unwrap_or(false)
    }

    pub fn session_id(&self) -> Option<&str> {
        self.session.as_ref().map(|s| s.session_id.as_str())
    }

    /// Run the connection: read NDJSON requests, dispatch, write responses.
    pub async fn run(mut self, stream: Box<dyn IpcStream>) {
        let (reader, writer) = tokio::io::split(stream);
        let mut nd_reader = NdjsonReader::new(reader);
        let mut nd_writer = NdjsonWriter::new(writer);

        info!(conn_id = %self.id, "ACP connection started");
        // TODO: Persist conversation history on disconnect for cross-connection continuity.

        while let Some(result) = nd_reader.read_line().await {
            let line = match result {
                Ok(l) => l,
                Err(e) => {
                    warn!(conn_id = %self.id, error = %e, "read error");
                    break;
                }
            };

            let request: AcpRequest = match serde_json::from_str(&line) {
                Ok(r) => r,
                Err(e) => {
                    // Build error response directly -- avoids serializing
                    // an empty AcpResponse that could produce invalid JSON-RPC.
                    let err = json!({
                        "jsonrpc": "2.0",
                        "id": null,
                        "error": { "code": -32700, "message": format!("Parse error: {}", e) }
                    });
                    let _ = nd_writer.write_json(&err).await;
                    continue;
                }
            };

            let req_id = request.id.clone();
            let method = request.method.clone();
            let params = request.params.clone();

            debug!(conn_id = %self.id, method = %method, "dispatching request");

            let req_id_val = req_id;
            let (response, events_opt) = self.dispatch(method, params).await;

            // Drain streaming events BEFORE the final JSON-RPC response,
            // so the client sees content chunks before the stopReason signal.
            if let Some(rx) = events_opt {
                let sid = self.session_id().unwrap_or("").to_string();
                if !bridge_events(rx, &mut nd_writer, &sid).await {
                    warn!(conn_id = %self.id, "write error during streaming, client disconnected");
                    break;
                }
            }

            // Send the JSON-RPC response, patching id to match the request.
            if let Ok(mut resp_json) = serde_json::to_value(&response) {
                if let Some(rid) = req_id_val {
                    resp_json["id"] = rid;
                }
                if nd_writer.write_json(&resp_json).await.is_err() {
                    warn!(conn_id = %self.id, "write error, client disconnected");
                    break;
                }
            }

        }
        info!(conn_id = %self.id, "ACP connection closed");
    }

    async fn dispatch(
        &mut self,
        method: String,
        params: Option<Value>,
    ) -> (AcpResponse, Option<mpsc::Receiver<StreamEvent>>) {
        match method.as_str() {
            METHOD_INITIALIZE => (self.handle_initialize(params), None),
            METHOD_AUTHENTICATE => (
                AcpResponse::error(None, -32601, "Method not found (Named Pipe trust boundary)"),
                None,
            ),
            METHOD_SESSION_NEW => (self.handle_session_new(params), None),
            METHOD_SESSION_PROMPT => self.handle_session_prompt(params).await,
            METHOD_SESSION_CANCEL => (self.handle_session_cancel(), None),
            METHOD_SESSION_PING => (AcpResponse::success(None, json!({})), None),
            METHOD_SESSION_SET_MODE => (self.handle_set_mode(params), None),
            METHOD_CHERRY_SHUTDOWN => {
                info!(conn_id = %self.id, "cherry/shutdown received");
                (AcpResponse::success(None, json!({})), None)
            }
            _ => (
                AcpResponse::error(None, -32601, format!("Method not found: {}", method)),
                None,
            ),
        }
    }

    fn fire_meta_update(&self) {
        if let Some(cb) = &self.meta_cb {
            cb(
                self.id.clone(),
                MetaUpdate {
                    client_name: self.client_name().map(String::from),
                    session_id: self.session_id().map(String::from),
                    client_title: self.session.as_ref().and_then(|s| s.client_title.clone()),
                },
            );
        }
    }

    fn handle_initialize(&mut self, params: Option<Value>) -> AcpResponse {
        if let Some(p) = &params {
            if let Some(ci) = p.get("clientInfo") {
                let name = ci.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let title = ci.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let version = ci.get("version").and_then(|v| v.as_str()).unwrap_or("").to_string();
                info!(
                    conn_id = %self.id,
                    client_name = %name,
                    client_title = %title,
                    "client identified"
                );
                if self.session.is_none() {
                    let mut session = PipeSession::new(format!("pending-{}", self.id));
                    session.client_name = Some(name);
                    session.client_title = Some(title);
                    session.client_version = Some(version);
                    self.session = Some(session);
                } else {
                    let s = self.session.as_mut().unwrap();
                    s.client_name = Some(name);
                    s.client_title = Some(title);
                    s.client_version = Some(version);
                }
            }
        }

        self.state = ConnectionState::Initialized;
        self.fire_meta_update();

        AcpResponse::success(
            None,
            json!({
                "protocolVersion": 1,
                "agentInfo": {
                    "name": self.agent_info.name,
                    "title": self.agent_info.title,
                    "version": self.agent_info.version
                },
                "agentCapabilities": {
                    "promptCapabilities": { "streaming": true },
                    "sessionCapabilities": { "fork": false, "list": false, "resume": false }
                },
                "authMethods": []
            }),
        )
    }

    fn handle_session_new(&mut self, params: Option<Value>) -> AcpResponse {
        if self.state != ConnectionState::Initialized {
            return AcpResponse::error(None, -32600, "Not initialized");
        }

        let session_id = format!("acp:main:{}", uuid::Uuid::new_v4());
        let mut session = PipeSession::new(session_id.clone());

        if let Some(old) = self.session.take() {
            session.client_name = old.client_name;
            session.client_title = old.client_title;
            session.client_version = old.client_version;
        }

        if let Some(p) = &params {
            if let Some(cwd) = p.get("cwd").and_then(|v| v.as_str()) {
                session.cwd = Some(cwd.to_string());
            }
            if let Some(meta) = p.get("_meta") {
                session.source = meta.get("source").and_then(|v| v.as_str()).map(String::from);
                session.channel = meta.get("channel").and_then(|v| v.as_str()).map(String::from);
                session.skill_id = meta.get("skillId").and_then(|v| v.as_str()).map(String::from);
            }
        }

        self.session = Some(session);
        self.state = ConnectionState::SessionReady;

        info!(conn_id = %self.id, session_id = %session_id, "session created");
        self.fire_meta_update();

        AcpResponse::success(None, json!({ "sessionId": session_id }))
    }

    async fn handle_session_prompt(
        &mut self,
        params: Option<Value>,
    ) -> (AcpResponse, Option<mpsc::Receiver<StreamEvent>>) {
        if self.state != ConnectionState::SessionReady && self.state != ConnectionState::Active {
            return (AcpResponse::error(None, -32600, "No active session"), None);
        }

        if self.prompt_active.load(Ordering::Acquire) {
            return (AcpResponse::error(None, -32600, "Prompt already in progress"), None);
        }

        let session = match &self.session {
            Some(s) => s,
            None => return (AcpResponse::error(None, -32600, "No session"), None),
        };

        let prompt_text = params
            .as_ref()
            .and_then(|p| p.get("prompt"))
            .and_then(|p| p.as_array())
            .and_then(|arr| arr.first())
            .and_then(|item| item.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();

        if prompt_text.is_empty() {
            return (AcpResponse::error(None, -32600, "Empty prompt"), None);
        }

        self.state = ConnectionState::Active;
        self.prompt_active.store(true, Ordering::Release);
        self.cancel_flag.store(false, Ordering::Release);

        debug!(
            conn_id = %self.id,
            session_id = %session.session_id,
            prompt_len = prompt_text.len(),
            "executing prompt"
        );

        let (event_tx, event_rx) = mpsc::channel::<StreamEvent>(256);

        let executor = self.executor.clone();
        let session_clone = session.clone();
        let _cancel_flag = self.cancel_flag.clone();

        // TODO: For true cancellation the executor itself needs to check
        // cancel_flag (e.g. abort the reqwest SSE stream). The select!
        // branch below only breaks the outer await, it cannot interrupt
        // an in-flight HTTP connection.
        let result = tokio::select! {
            r = executor.execute(&session_clone, &prompt_text, &session_clone.history, event_tx) => r,
            _ = tokio::time::sleep(Duration::from_secs(self.prompt_timeout_secs)) => {
                Err(format!("Prompt timed out ({}s)", self.prompt_timeout_secs))
            }
        };

        let cancelled = self.cancel_flag.load(Ordering::Acquire);
        self.prompt_active.store(false, Ordering::Release);
        self.state = ConnectionState::SessionReady;

        let response = match result {
            Ok(pr) => {
                let stop = if cancelled {
                    StopReason::Cancelled
                } else {
                    pr.stop_reason
                };
                info!(
                    conn_id = %self.id,
                    session_id = %session.session_id,
                    stop_reason = ?stop,
                    "prompt completed"
                );

                if let Some(s) = &mut self.session {
                    s.history.push(json!({
                        "role": "user",
                        "content": prompt_text
                    }));
                }

                AcpResponse::success(
                    None,
                    json!({
                        "stopReason": match stop {
                            StopReason::EndTurn => "end_turn",
                            StopReason::Cancelled => "cancelled",
                            StopReason::MaxTokens => "max_tokens",
                            StopReason::Refusal => "refusal",
                            StopReason::Error => "error",
                        }
                    }),
                )
            }
            Err(e) => {
                warn!(conn_id = %self.id, error = %e, "prompt failed");
                AcpResponse::error(None, -32603, format!("Prompt execution error: {}", e))
            }
        };

        (response, Some(event_rx))
    }

    fn handle_session_cancel(&mut self) -> AcpResponse {
        if self.prompt_active.load(Ordering::Acquire) {
            self.cancel_flag.store(true, Ordering::Release);
            info!(conn_id = %self.id, "prompt cancelled");
        }
        AcpResponse::success(None, json!({}))
    }

    fn handle_set_mode(&mut self, params: Option<Value>) -> AcpResponse {
        let mode_id = params
            .and_then(|p| p.get("modeId").and_then(|v| v.as_str()).map(String::from))
            .unwrap_or_default();

        if let Some(s) = &mut self.session {
            s.mode = if mode_id.is_empty() { None } else { Some(mode_id.clone()) };
            s.skill_id = if mode_id.is_empty() { None } else { Some(mode_id) };
        }

        AcpResponse::success(None, json!({}))
    }
}

