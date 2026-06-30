//! Desktop / TUI JSON-RPC over WebSocket (`/api/ws`), compatible with Python `tui_gateway`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use axum::extract::ws::{Message as WsMessage, WebSocket};
use futures::{SinkExt, StreamExt};
use hermes_agent::agent_loop::ToolRegistry as AgentToolRegistry;
use hermes_agent::session_persistence::SessionPersistence;
use hermes_agent::{RunConversationParams, split_messages_for_run_conversation};
use hermes_config::GatewayConfig;
use hermes_core::{Message, MessageRole, StreamChunk};
use hermes_gateway::GatewayRuntimeContext;
use hermes_gateway::gateway::IncomingMessage;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::task::AbortHandle;
use ulid::Ulid;

use crate::{
    HTTP_PLATFORM, HttpServerState, bridge_tool_registry, build_agent_for_gateway_context,
    build_provider, extract_last_assistant_reply,
};

const DESKTOP_BACKEND_CONTRACT: i32 = 1;
const DESKTOP_PLATFORM: &str = "desktop";

pub struct SessionStore {
    inner: Mutex<HashMap<String, DesktopSession>>,
}

impl SessionStore {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }
}

struct DesktopSession {
    stored_session_id: String,
    cwd: String,
    cols: u32,
    history: Mutex<Vec<Message>>,
    running: AtomicBool,
    turn_abort: Mutex<Option<AbortHandle>>,
    profile: Option<String>,
}

struct RpcContext {
    state: HttpServerState,
    agent_tools: Arc<AgentToolRegistry>,
    outbound_tx: mpsc::UnboundedSender<String>,
}

pub async fn handle_desktop_ws(socket: WebSocket, state: HttpServerState) {
    let agent_tools = Arc::new(bridge_tool_registry(&state.tool_registry));
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel::<String>();
    let (mut ws_tx, mut ws_rx) = socket.split();

    let writer = tokio::spawn(async move {
        while let Some(text) = outbound_rx.recv().await {
            if ws_tx.send(WsMessage::Text(text.into())).await.is_err() {
                break;
            }
        }
    });

    let ctx = RpcContext {
        state: state.clone(),
        agent_tools,
        outbound_tx: outbound_tx.clone(),
    };

    emit_event(
        &outbound_tx,
        "gateway.ready",
        None,
        json!({ "skin": gateway_ready_skin(&ctx.state.config) }),
    );

    while let Some(msg) = ws_rx.next().await {
        let Ok(WsMessage::Text(text)) = msg else {
            if matches!(msg, Ok(WsMessage::Close(_)) | Err(_)) {
                break;
            }
            continue;
        };

        let frame: Value = match serde_json::from_str(text.as_ref()) {
            Ok(v) => v,
            Err(_) => {
                send_frame(
                    &outbound_tx,
                    json!({
                        "jsonrpc": "2.0",
                        "error": { "code": -32700, "message": "parse error" },
                        "id": null
                    }),
                );
                continue;
            }
        };

        if frame.get("id").is_none() {
            continue;
        }

        let response = dispatch_rpc(&ctx, &frame).await;
        send_frame(&outbound_tx, response);
    }

    writer.abort();
}

fn send_frame(tx: &mpsc::UnboundedSender<String>, frame: Value) {
    let _ = tx.send(frame.to_string());
}

fn emit_event(
    tx: &mpsc::UnboundedSender<String>,
    event_type: &str,
    session_id: Option<&str>,
    payload: Value,
) {
    let mut params = json!({ "type": event_type });
    if let Some(sid) = session_id {
        params["session_id"] = json!(sid);
    }
    if !payload.is_null() {
        params["payload"] = payload;
    }
    send_frame(
        tx,
        json!({
            "jsonrpc": "2.0",
            "method": "event",
            "params": params
        }),
    );
}

fn rpc_ok(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn rpc_err(id: &Value, code: i32, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}

async fn dispatch_rpc(ctx: &RpcContext, frame: &Value) -> Value {
    let id = frame.get("id").cloned().unwrap_or(Value::Null);
    let method = frame
        .get("method")
        .and_then(|m| m.as_str())
        .unwrap_or("")
        .trim();
    let params = frame
        .get("params")
        .and_then(|p| p.as_object())
        .cloned()
        .unwrap_or_default();

    match method {
        "session.create" => session_create(ctx, &id, &params).await,
        "session.close" => session_close(ctx, &id, &params),
        "session.interrupt" => session_interrupt(ctx, &id, &params),
        "session.resume" => session_resume(ctx, &id, &params).await,
        "prompt.submit" => prompt_submit(ctx, &id, &params).await,
        "terminal.resize" => terminal_resize(ctx, &id, &params),
        "config.get" => config_get(ctx, &id, &params),
        "config.set" => rpc_ok(&id, json!({ "ok": true })),
        "setup.status" => setup_status(ctx, &id),
        "setup.runtime_check" => setup_runtime_check(ctx, &id),
        "commands.catalog" => commands_catalog(&id),
        "complete.slash" => rpc_ok(&id, json!({ "items": [] })),
        "complete.path" => complete_path(ctx, &id, &params),
        "model.options" => model_options(ctx, &id),
        "slash.exec" => slash_exec(ctx, &id, &params).await,
        "command.dispatch" => rpc_ok(
            &id,
            json!({ "type": "exec", "output": "command dispatch not implemented" }),
        ),
        "clarify.respond" | "approval.respond" | "sudo.respond" | "secret.respond" => {
            rpc_ok(&id, json!({ "ok": true, "resolved": true, "status": "ok" }))
        }
        "reload.mcp" => rpc_ok(&id, json!({ "ok": true })),
        _ => rpc_err(&id, -32601, &format!("unknown method: {method}")),
    }
}

fn new_runtime_session_id() -> String {
    Ulid::new().to_string().to_ascii_lowercase()[..8].to_string()
}

fn new_stored_session_id() -> String {
    Ulid::new().to_string()
}

fn resolve_model(config: &GatewayConfig) -> String {
    config
        .model
        .clone()
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| "openai:gpt-4o".to_string())
}

fn resolve_cwd(config: &GatewayConfig, params: &serde_json::Map<String, Value>) -> String {
    if let Some(cwd) = params.get("cwd").and_then(|v| v.as_str()) {
        let expanded = expand_path(cwd);
        if expanded.is_dir() {
            return expanded.to_string_lossy().into_owned();
        }
    }
    std::env::current_dir()
        .ok()
        .or_else(|| {
            config
                .home_dir
                .as_ref()
                .map(|h| PathBuf::from(h))
                .filter(|p| p.is_dir())
        })
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| ".".to_string())
}

fn expand_path(raw: &str) -> PathBuf {
    let trimmed = raw.trim();
    if trimmed == "~" {
        return dirs_home();
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        return dirs_home().join(rest);
    }
    PathBuf::from(trimmed)
}

fn dirs_home() -> PathBuf {
    hermes_config::hermes_home()
}

fn git_branch_for_cwd(cwd: &str) -> String {
    let output = std::process::Command::new("git")
        .args(["-C", cwd, "rev-parse", "--abbrev-ref", "HEAD"])
        .output();
    match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        _ => String::new(),
    }
}

fn session_info_payload(config: &GatewayConfig, session: &DesktopSession) -> Value {
    json!({
        "model": resolve_model(config),
        "tools": {},
        "skills": {},
        "cwd": session.cwd,
        "branch": git_branch_for_cwd(&session.cwd),
        "lazy": true,
        "desktop_contract": DESKTOP_BACKEND_CONTRACT,
        "profile_name": session.profile.clone().unwrap_or_else(|| "default".to_string())
    })
}

fn history_to_messages(history: &[Message]) -> Vec<Value> {
    history
        .iter()
        .filter_map(|m| {
            let role = match m.role {
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::System => "system",
                MessageRole::Tool => "tool",
            };
            let text = m.content.as_deref().unwrap_or("").trim();
            if text.is_empty() && m.role != MessageRole::Tool {
                return None;
            }
            Some(json!({ "role": role, "text": text }))
        })
        .collect()
}

fn session_create_payload(config: &GatewayConfig, sid: &str, session: &DesktopSession) -> Value {
    let history = session.history.lock().unwrap_or_else(|e| e.into_inner());
    json!({
        "session_id": sid,
        "stored_session_id": session.stored_session_id,
        "message_count": history.len(),
        "messages": history_to_messages(&history),
        "info": session_info_payload(config, session)
    })
}

async fn session_create(
    ctx: &RpcContext,
    id: &Value,
    params: &serde_json::Map<String, Value>,
) -> Value {
    let sid = new_runtime_session_id();
    let stored = new_stored_session_id();
    let cols = params.get("cols").and_then(|v| v.as_u64()).unwrap_or(96) as u32;
    let cwd = resolve_cwd(&ctx.state.config, params);
    let profile = params
        .get("profile")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(str::to_string);

    let session = DesktopSession {
        stored_session_id: stored,
        cwd,
        cols,
        history: Mutex::new(Vec::new()),
        running: AtomicBool::new(false),
        turn_abort: Mutex::new(None),
        profile,
    };

    ctx.state
        .desktop
        .inner
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(sid.clone(), session);

    let snapshot = ctx
        .state
        .desktop
        .inner
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&sid)
        .map(|s| session_create_payload(&ctx.state.config, &sid, s));

    match snapshot {
        Some(payload) => rpc_ok(id, payload),
        None => rpc_err(id, 5000, "session create failed"),
    }
}

fn session_close(ctx: &RpcContext, id: &Value, params: &serde_json::Map<String, Value>) -> Value {
    let sid = params
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let removed = ctx
        .state
        .desktop
        .inner
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(sid)
        .is_some();
    rpc_ok(id, json!({ "closed": removed }))
}

fn session_interrupt(
    ctx: &RpcContext,
    id: &Value,
    params: &serde_json::Map<String, Value>,
) -> Value {
    let sid = params
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let sessions = ctx
        .state
        .desktop
        .inner
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let Some(session) = sessions.get(sid) else {
        return rpc_err(id, 4001, "session not found");
    };
    if let Some(handle) = session.turn_abort.lock().unwrap().take() {
        handle.abort();
    }
    session.running.store(false, Ordering::SeqCst);
    rpc_ok(id, json!({ "status": "interrupted" }))
}

async fn session_resume(
    ctx: &RpcContext,
    id: &Value,
    params: &serde_json::Map<String, Value>,
) -> Value {
    let target = params
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if target.is_empty() {
        return rpc_err(id, 4004, "session_id required");
    }

    {
        let sessions = ctx
            .state
            .desktop
            .inner
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        for (sid, session) in sessions.iter() {
            if session.stored_session_id == target {
                let mut payload = session_create_payload(&ctx.state.config, sid, session);
                payload["resumed"] = json!(target);
                return rpc_ok(id, payload);
            }
        }
    }

    let home = ctx
        .state
        .config
        .home_dir
        .clone()
        .unwrap_or_else(|| hermes_config::hermes_home().to_string_lossy().into_owned());
    let sp = SessionPersistence::new(PathBuf::from(&home));
    let _record = match sp.get_session(&target) {
        Ok(Some(r)) => r,
        Ok(None) => return rpc_err(id, 4007, "session not found"),
        Err(e) => return rpc_err(id, 5000, &e.to_string()),
    };

    let _ = sp.reopen_session(&target);
    let sid = new_runtime_session_id();
    let cols = params.get("cols").and_then(|v| v.as_u64()).unwrap_or(96) as u32;
    let cwd = resolve_cwd(&ctx.state.config, params);

    let session = DesktopSession {
        stored_session_id: target.clone(),
        cwd,
        cols,
        history: Mutex::new(Vec::new()),
        running: AtomicBool::new(false),
        turn_abort: Mutex::new(None),
        profile: params
            .get("profile")
            .and_then(|v| v.as_str())
            .map(str::to_string),
    };

    ctx.state
        .desktop
        .inner
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(sid.clone(), session);

    let mut payload = ctx
        .state
        .desktop
        .inner
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&sid)
        .map(|s| session_create_payload(&ctx.state.config, &sid, s))
        .unwrap_or_else(|| json!({}));
    payload["resumed"] = json!(target);
    rpc_ok(id, payload)
}

async fn prompt_submit(
    ctx: &RpcContext,
    id: &Value,
    params: &serde_json::Map<String, Value>,
) -> Value {
    let sid = params
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let text = params
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if sid.is_empty() {
        return rpc_err(id, 4004, "session_id required");
    }
    if text.trim().is_empty() {
        return rpc_err(id, 4004, "text required");
    }

    let session_snapshot = {
        let sessions = ctx
            .state
            .desktop
            .inner
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let Some(session) = sessions.get(&sid) else {
            return rpc_err(id, 4001, "session not found");
        };
        if session.running.load(Ordering::SeqCst) {
            return rpc_err(id, 4009, "session busy");
        }
        session.running.store(true, Ordering::SeqCst);
        (
            session.stored_session_id.clone(),
            session.cwd.clone(),
            session.profile.clone(),
        )
    };

    let ctx_clone = RpcContext {
        state: ctx.state.clone(),
        agent_tools: ctx.agent_tools.clone(),
        outbound_tx: ctx.outbound_tx.clone(),
    };
    let sid_clone = sid.clone();
    let text_clone = text.clone();
    let join = tokio::spawn(async move {
        run_prompt_turn(&ctx_clone, &sid_clone, &text_clone, session_snapshot).await;
    });

    if let Ok(mut sessions) = ctx.state.desktop.inner.lock() {
        if let Some(session) = sessions.get_mut(&sid) {
            *session.turn_abort.lock().unwrap() = Some(join.abort_handle());
        }
    }

    rpc_ok(id, json!({ "status": "streaming" }))
}

async fn run_prompt_turn(
    ctx: &RpcContext,
    sid: &str,
    text: &str,
    (stored_session_id, cwd, profile): (String, String, Option<String>),
) {
    let finish = |ctx: &RpcContext, sid: &str| {
        if let Ok(mut sessions) = ctx.state.desktop.inner.lock() {
            if let Some(session) = sessions.get_mut(sid) {
                session.running.store(false, Ordering::SeqCst);
                *session.turn_abort.lock().unwrap() = None;
            }
        }
    };

    emit_event(&ctx.outbound_tx, "message.start", Some(sid), json!({}));

    let gateway_ctx = GatewayRuntimeContext {
        session_key: stored_session_id.clone(),
        session_id: stored_session_id.clone(),
        platform: DESKTOP_PLATFORM.to_string(),
        chat_id: stored_session_id.clone(),
        user_id: "desktop".to_string(),
        model: ctx.state.config.model.clone(),
        provider: None,
        personality: ctx.state.config.personality.clone(),
        home: profile
            .as_ref()
            .and_then(|_| ctx.state.config.home_dir.clone())
            .or_else(|| ctx.state.config.home_dir.clone()),
        ..Default::default()
    };

    let agent = build_agent_for_gateway_context(
        ctx.state.config.as_ref(),
        &gateway_ctx,
        ctx.agent_tools.clone(),
        ctx.state.tool_registry.clone(),
    );

    let user_message = Message::user(text.to_string());

    let history = {
        let sessions = ctx
            .state
            .desktop
            .inner
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let Some(session) = sessions.get(sid) else {
            emit_event(
                &ctx.outbound_tx,
                "error",
                Some(sid),
                json!({ "message": "session not found" }),
            );
            finish(ctx, sid);
            return;
        };
        let mut hist = session.history.lock().unwrap_or_else(|e| e.into_inner());
        hist.push(user_message.clone());
        hist.clone()
    };

    let (conv_history, run_user) = match split_messages_for_run_conversation(&history) {
        Some(parts) => parts,
        None => {
            emit_event(
                &ctx.outbound_tx,
                "error",
                Some(sid),
                json!({ "message": "invalid conversation history" }),
            );
            finish(ctx, sid);
            return;
        }
    };

    let event_tx = ctx.outbound_tx.clone();
    let sid_owned = sid.to_string();
    let stream_cb: Box<dyn Fn(StreamChunk) + Send + Sync> = Box::new(move |chunk: StreamChunk| {
        if let Some(delta) = chunk.delta {
            if let Some(text) = delta.content {
                if !text.is_empty() {
                    emit_event(
                        &event_tx,
                        "message.delta",
                        Some(&sid_owned),
                        json!({ "text": text }),
                    );
                }
            }
        }
    });

    let result = agent
        .run_conversation(RunConversationParams {
            user_message: run_user,
            conversation_history: conv_history,
            task_id: Some(stored_session_id.clone()),
            stream_callback: Some(stream_cb),
            persist_user_message: None,
            tools: None,
            persist_session: true,
        })
        .await;

    match result {
        Ok(conv) => {
            let reply = conv
                .final_response
                .clone()
                .unwrap_or_else(|| extract_last_assistant_reply(conv.messages()));
            if let Ok(mut sessions) = ctx.state.desktop.inner.lock() {
                if let Some(session) = sessions.get_mut(sid) {
                    let mut hist = session.history.lock().unwrap_or_else(|e| e.into_inner());
                    hist.push(Message::assistant(reply.clone()));
                }
            }
            emit_event(
                &ctx.outbound_tx,
                "message.complete",
                Some(sid),
                json!({
                    "text": reply,
                    "status": "complete",
                    "usage": {
                        "input_tokens": conv.input_tokens().unwrap_or(0),
                        "output_tokens": conv.output_tokens().unwrap_or(0)
                    }
                }),
            );
            if let Ok(sessions) = ctx.state.desktop.inner.lock() {
                if let Some(session) = sessions.get(sid) {
                    emit_event(
                        &ctx.outbound_tx,
                        "session.info",
                        Some(sid),
                        session_info_payload(&ctx.state.config, session),
                    );
                }
            }
            let _ = (cwd, profile);
        }
        Err(e) => {
            emit_event(
                &ctx.outbound_tx,
                "error",
                Some(sid),
                json!({ "message": e.to_string() }),
            );
        }
    }

    finish(ctx, sid);
}

fn terminal_resize(ctx: &RpcContext, id: &Value, params: &serde_json::Map<String, Value>) -> Value {
    let sid = params
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let cols = params.get("cols").and_then(|v| v.as_u64()).unwrap_or(96) as u32;
    let mut sessions = ctx
        .state
        .desktop
        .inner
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let Some(session) = sessions.get_mut(sid) else {
        return rpc_err(id, 4001, "session not found");
    };
    session.cols = cols;
    rpc_ok(id, json!({ "cols": cols }))
}

fn config_get(ctx: &RpcContext, id: &Value, params: &serde_json::Map<String, Value>) -> Value {
    let key = params.get("key").and_then(|v| v.as_str()).unwrap_or("");
    let config = ctx.state.config.as_ref();
    match key {
        "provider" => {
            let model = resolve_model(config);
            let (provider, _) = model.split_once(':').unwrap_or(("openai", model.as_str()));
            rpc_ok(
                id,
                json!({
                    "model": model,
                    "provider": provider,
                    "providers": config.llm_providers.keys().collect::<Vec<_>>()
                }),
            )
        }
        "profile" => {
            let home = hermes_config::hermes_home();
            rpc_ok(
                id,
                json!({
                    "home": home.to_string_lossy(),
                    "display": home.to_string_lossy()
                }),
            )
        }
        "project" => {
            let cwd = resolve_cwd(config, params);
            rpc_ok(
                id,
                json!({
                    "cwd": cwd,
                    "branch": git_branch_for_cwd(&cwd)
                }),
            )
        }
        "full" => match serde_json::to_value(config) {
            Ok(value) => rpc_ok(id, json!({ "config": value })),
            Err(e) => rpc_err(id, 5001, &e.to_string()),
        },
        "prompt" => rpc_ok(
            id,
            json!({ "prompt": config.system_prompt.clone().unwrap_or_default() }),
        ),
        "skin" => rpc_ok(id, json!({ "value": "default" })),
        "reasoning" => rpc_ok(id, json!({ "value": "medium", "display": "hide" })),
        "fast" => rpc_ok(id, json!({ "value": "normal" })),
        "busy" => rpc_ok(id, json!({ "value": false })),
        _ => rpc_err(id, 4002, &format!("unknown config key: {key}")),
    }
}

fn setup_status(ctx: &RpcContext, id: &Value) -> Value {
    rpc_ok(
        id,
        json!({ "provider_configured": provider_configured(&ctx.state.config) }),
    )
}

fn setup_runtime_check(ctx: &RpcContext, id: &Value) -> Value {
    let config = ctx.state.config.as_ref();
    let model = resolve_model(config);
    let provider = build_provider(config, &model);
    let configured = provider_configured(config);
    let api_key_missing = model.contains("missing") || !configured;
    if api_key_missing {
        return rpc_ok(
            id,
            json!({
                "ok": false,
                "provider": model.split_once(':').map(|(p, _)| p).unwrap_or("openai"),
                "model": model,
                "error": "No Hermes provider is configured."
            }),
        );
    }
    let _ = provider;
    rpc_ok(
        id,
        json!({
            "ok": true,
            "provider": model.split_once(':').map(|(p, _)| p).unwrap_or("openai"),
            "model": model
        }),
    )
}

fn provider_configured(config: &GatewayConfig) -> bool {
    config.llm_providers.values().any(|p| {
        p.api_key.as_ref().is_some_and(|k| !k.trim().is_empty())
            || p.api_key_env
                .as_ref()
                .and_then(|e| std::env::var(e).ok())
                .is_some_and(|v| !v.trim().is_empty())
    })
}

fn commands_catalog(id: &Value) -> Value {
    rpc_ok(
        id,
        json!({
            "pairs": [
                ["/help", "Show available commands"],
                ["/new", "Start a new session"],
                ["/clear", "Clear conversation history"],
                ["/status", "Show session status"]
            ],
            "sub": {},
            "canon": {},
            "categories": [],
            "skill_count": 0,
            "warning": ""
        }),
    )
}

fn complete_path(ctx: &RpcContext, id: &Value, params: &serde_json::Map<String, Value>) -> Value {
    let prefix = params.get("prefix").and_then(|v| v.as_str()).unwrap_or("");
    let cwd = resolve_cwd(&ctx.state.config, params);
    let base = Path::new(&cwd);
    let path = if prefix.is_empty() {
        base.to_path_buf()
    } else if Path::new(prefix).is_absolute() {
        PathBuf::from(prefix)
    } else {
        base.join(prefix)
    };
    let parent = if path.is_dir() {
        path.clone()
    } else {
        path.parent().unwrap_or(base).to_path_buf()
    };
    let mut items = Vec::new();
    if let Ok(read) = std::fs::read_dir(parent) {
        for entry in read.flatten().take(50) {
            let name = entry.file_name().to_string_lossy().into_owned();
            items.push(json!({ "label": name, "value": entry.path().to_string_lossy() }));
        }
    }
    rpc_ok(id, json!({ "items": items }))
}

fn model_options(ctx: &RpcContext, id: &Value) -> Value {
    let config = ctx.state.config.as_ref();
    let providers: Vec<Value> = config
        .llm_providers
        .iter()
        .map(|(name, cfg)| {
            json!({
                "name": name,
                "models": cfg.models.clone(),
                "base_url": cfg.base_url
            })
        })
        .collect();
    rpc_ok(id, json!({ "providers": providers }))
}

async fn slash_exec(
    ctx: &RpcContext,
    id: &Value,
    params: &serde_json::Map<String, Value>,
) -> Value {
    let sid = params
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let command = params
        .get("command")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if command.is_empty() {
        return rpc_err(id, 4004, "command required");
    }

    let stored_session_id = {
        let sessions = ctx
            .state
            .desktop
            .inner
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let Some(session) = sessions.get(sid) else {
            return rpc_err(id, 4001, "session not found");
        };
        session.stored_session_id.clone()
    };

    ctx.state.outbound.clear_chat(&stored_session_id);
    let incoming = IncomingMessage {
        platform: HTTP_PLATFORM.to_string(),
        chat_id: stored_session_id.clone(),
        user_id: "desktop".to_string(),
        text: if command.starts_with('/') {
            command.clone()
        } else {
            format!("/{command}")
        },
        media_urls: vec![],
        media_types: vec![],
        message_id: None,
        is_dm: false,
        interaction_id: None,
        interaction_token: None,
        role_ids: vec![],
        parent_channel_id: None,
        channel_prompt: None,
        channel_skills: vec![],
        channel_topic: None,
        message_thread_id: None,
    };

    match ctx.state.gateway.route_message(&incoming).await {
        Ok(()) => {
            let parts = ctx.state.outbound.drain_chat(&stored_session_id);
            let output = if parts.is_empty() {
                String::new()
            } else {
                parts.join("\n")
            };
            rpc_ok(id, json!({ "output": output }))
        }
        Err(e) => rpc_err(id, 5001, &e.to_string()),
    }
}

fn gateway_ready_skin(_config: &Arc<GatewayConfig>) -> Value {
    json!({
        "name": "default",
        "branding": {
            "agent_name": "Hermes",
            "prompt_symbol": ">",
            "welcome_text": "Hermes ready",
            "goodbye_text": "Goodbye"
        },
        "colors": {}
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_create_payload_has_session_id() {
        let config = GatewayConfig::default();
        let session = DesktopSession {
            stored_session_id: "stored-1".to_string(),
            cwd: "/tmp".to_string(),
            cols: 96,
            history: Mutex::new(Vec::new()),
            running: AtomicBool::new(false),
            turn_abort: Mutex::new(None),
            profile: None,
        };
        let payload = session_create_payload(&config, "abcd1234", &session);
        assert_eq!(payload["session_id"], "abcd1234");
        assert_eq!(payload["stored_session_id"], "stored-1");
        assert_eq!(
            payload["info"]["desktop_contract"],
            DESKTOP_BACKEND_CONTRACT
        );
    }
}
