# hermes-server Desktop 兼容性修复计划

> **最后更新**：2026-06-11
> 
> 本文档详细描述 Desktop 应用连接 Rust 后端的兼容性修复方案，包括阻断项修复、关键事件补齐和设置页 REST 端点实现。

## 目录

1. [背景与目标](#背景与目标)
2. [现状分析](#现状分析)
3. [阶段 1：修复阻断项](#阶段-1修复阻断项)
4. [阶段 2：补齐 Desktop 关键事件](#阶段-2补齐-desktop-关键事件)
5. [阶段 3：补齐 Desktop 设置页 REST 端点](#阶段-3补齐-desktop-设置页-rest-端点)
6. [验收标准](#验收标准)
7. [风险评估](#风险评估)

---

## 背景与目标

### 背景

`hermes-server`（Rust）作为 Python `tui_gateway` + `web_server.py` 的替代实现，需要与 Desktop Electron 应用完全兼容。经过代码审查，发现存在以下问题：

1. **Token 不统一**：Desktop 生成 token 注入 `HERMES_DASHBOARD_SESSION_TOKEN`，Rust 独立生成且不读环境变量
2. **WS 认证放行**：`ws/auth.rs` 无 token 时硬编码返回 `true`，存在安全漏洞
3. **事件推送不完整**：仅实现了 `message.delta/complete`，缺失 `tool.start/complete`、`approval.request` 等关键事件
4. **REST 端点缺失**：Desktop 设置页需要的 Skills、Toolsets、Cron、Messaging 端点未实现

### 目标

- P0：Desktop 能成功连接 Rust 后端（认证通过）
- P1：Desktop 聊天功能完整（工具动画、审批弹窗）
- P2：Desktop 设置页面正常工作

---

## 现状分析

### Desktop 实际使用的接口

#### REST 端点（60+ 个）

| 类别 | 端点 | 当前状态 |
|------|------|---------|
| **系统** | `GET /api/status` | ✅ 已实现 |
| **系统** | `GET /api/logs` | ❌ 缺失（有 `/api/ops/logs`） |
| **会话** | `GET /api/sessions` | ✅ 已实现 |
| **会话** | `GET /api/sessions/{id}/messages` | ✅ 已实现 |
| **模型** | `GET /api/model/options` | ✅ 已实现 |
| **配置** | `GET/PUT /api/config` | ✅ 已实现 |
| **环境变量** | `GET/PUT/DELETE /api/env` | ✅ 已实现 |
| **Skills** | `GET /api/skills` | ❌ 缺失 |
| **Skills** | `PUT /api/skills/toggle` | ❌ 缺失 |
| **Toolsets** | `GET /api/tools/toolsets` | ❌ 缺失 |
| **Cron** | `GET/POST/PUT/DELETE /api/cron/jobs/*` | ❌ 缺失 |
| **Messaging** | `GET/PUT /api/messaging/platforms/*` | ❌ 缺失 |
| **Gateway** | `POST /api/gateway/restart` | ⚠️ 占位符 |

#### JSON-RPC 方法

| 方法 | 当前状态 | 说明 |
|------|---------|------|
| `session.create` | ✅ | — |
| `session.list` | ✅ | — |
| `prompt.submit` | ✅ | 但事件推送不完整 |
| `approval.respond` | ✅ | 有响应处理 |
| `clarify.respond` | ✅ | 有响应处理 |
| `tools.list` | ⚠️ | 占位符实现 |
| `skills.manage` | ⚠️ | 占位符实现 |

#### 事件类型（Desktop 订阅）

| 事件 | 当前状态 | Desktop 用途 |
|------|---------|-------------|
| `gateway.ready` | ✅ | 连接就绪 |
| `message.start` | ✅ | 消息流开始 |
| `message.delta` | ✅ | 文本增量 |
| `message.complete` | ✅ | 消息完成 |
| `thinking.delta` | ⚠️ | 有定义但未触发 |
| `reasoning.delta` | ⚠️ | 有定义但未触发 |
| `tool.start` | ⚠️ | 有定义但未触发 |
| `tool.progress` | ❌ | 缺失 |
| `tool.complete` | ⚠️ | 有定义但未触发 |
| `approval.request` | ⚠️ | 有定义但未触发 |
| `clarify.request` | ⚠️ | 有定义但未触发 |

---

## 阶段 1：修复阻断项

### 1.1 Token 统一

**问题**：Rust `AppState::new()` 独立生成 token，不读 `HERMES_DASHBOARD_SESSION_TOKEN` 环境变量。

**方案**：

```rust
// crates/hermes-server/src/state.rs

impl AppState {
    pub fn new(config: GatewayConfig, hermes_home: std::path::PathBuf) -> Self {
        // 优先级：环境变量 > 自动生成
        let session_token = std::env::var("HERMES_DASHBOARD_SESSION_TOKEN")
            .unwrap_or_else(|_| generate_session_token());
        
        let (event_broadcast, _) = broadcast::channel(256);
        Self {
            config: Arc::new(RwLock::new(config)),
            hermes_home,
            session_token,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            active_profile: Arc::new(RwLock::new("default".to_string())),
            pending_interactions: crate::rpc::interaction::new_pending_interactions(),
            event_broadcast,
        }
    }
}
```

**验证**：
```bash
HERMES_DASHBOARD_SESSION_TOKEN=desktop_token cargo run -p hermes-server
curl -H "X-Hermes-Session-Token: desktop_token" http://127.0.0.1:9119/api/status
```

### 1.2 WS 认证硬化

**问题**：`ws/auth.rs` 无 token 时返回 `true`，任何人可连接 WebSocket。

**方案**：

```rust
// crates/hermes-server/src/ws/auth.rs

use subtle::ConstantTimeEq;

pub fn authenticate_ws(
    query_params: &[(String, String)],
    state: &AppState,
) -> bool {
    // 查找 ?token=... 或 ?ticket=...
    let token = query_params.iter()
        .find(|(k, _)| k == "token" || k == "ticket")
        .map(|(_, v)| v.as_str());
    
    match token {
        Some(t) => {
            let expected = state.session_token();
            // 常数时间比较
            t.as_bytes().ct_eq(expected.as_bytes()).into()
        }
        None => {
            tracing::warn!("WS connection rejected: missing token");
            false
        }
    }
}
```

**验证**：
```bash
# 无 token 应被拒绝
wscat -c ws://127.0.0.1:9119/api/ws
# 返回 401

# 有 token 应成功
wscat -c "ws://127.0.0.1:9119/api/ws?token=desktop_token"
# 收到 gateway.ready
```

### 1.3 HTTP 中间件硬化

**问题**：自定义 `constant_time_eq` 使用 `std::thread::sleep` 做延迟，长度不等时提前返回，仍可能泄露 timing 信息。

**方案**：

```rust
// crates/hermes-server/src/middleware.rs

use subtle::ConstantTimeEq;

fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        // 仍需常数时间比较，避免长度泄露
        let max_len = a.len().max(b.len());
        let mut a_padded = vec![0u8; max_len];
        let mut b_padded = vec![0u8; max_len];
        a_padded[..a.len().min(max_len)].copy_from_slice(&a.as_bytes()[..a.len().min(max_len)]);
        b_padded[..b.len().min(max_len)].copy_from_slice(&b.as_bytes()[..b.len().min(max_len)]);
        a_padded.ct_eq(&b_padded).into()
    } else {
        a.as_bytes().ct_eq(b.as_bytes()).into()
    }
}
```

**依赖**：
```toml
# crates/hermes-server/Cargo.toml
[dependencies]
subtle = "2"
```

### 1.4 日志路径别名

**问题**：Desktop 请求 `GET /api/logs`，但 Rust 只有 `/api/ops/logs`。

**方案**：

```rust
// crates/hermes-server/src/server.rs

// 在路由注册中添加别名
.route("/api/logs", get(crate::rest::ops::get_logs))
```

---

## 阶段 2：补齐 Desktop 关键事件

### 2.1 绑定工具调用回调

**问题**：`build_agent()` 未设置 `AgentCallbacks`，工具调用事件无法推送到 Desktop。

**方案**：

修改 `crates/hermes-server/src/core/agent_builder.rs`：

```rust
use hermes_agent::AgentCallbacks;
use crate::ws::rpc::JsonRpcEvent;

pub fn build_agent(
    config: &GatewayConfig,
    session_id: &str,
    session_key: &str,
    hermes_home: &std::path::Path,
    transport: crate::ws::transport::ReplaceableTransport,
) -> Option<AgentLoop> {
    let mut agent_config = AgentConfig {
        model: config.model.clone().unwrap_or_else(|| "gpt-4o".to_string()),
        session_id: Some(session_id.to_string()),
        gateway_session_key: Some(session_key.to_string()),
        hermes_home: Some(hermes_home.to_string_lossy().to_string()),
        stream: true,
        quiet_mode: false,
        ..Default::default()
    };

    let llm_provider = build_llm_provider(config, &mut agent_config)?;
    let tool_registry = Arc::new(ToolRegistry::new());
    
    let mut agent = AgentLoop::new(agent_config, tool_registry, llm_provider);
    
    // 绑定回调以推送事件到 Desktop
    let sid = session_id.to_string();
    let callbacks = AgentCallbacks {
        on_tool_start: Some(Box::new({
            let transport = transport.clone();
            let sid = sid.clone();
            move |tool_name, args| {
                let event = JsonRpcEvent::new(
                    crate::ws::events::types::TOOL_START,
                    Some(&sid),
                    Some(serde_json::json!({
                        "tool": tool_name,
                        "args": args,
                    })),
                );
                if let Ok(json) = serde_json::to_value(&event) {
                    let _ = transport.write(&json);
                }
            }
        })),
        on_tool_complete: Some(Box::new({
            let transport = transport.clone();
            let sid = sid.clone();
            move |tool_name, result| {
                let event = JsonRpcEvent::new(
                    crate::ws::events::types::TOOL_COMPLETE,
                    Some(&sid),
                    Some(serde_json::json!({
                        "tool": tool_name,
                        "result": result,
                    })),
                );
                if let Ok(json) = serde_json::to_value(&event) {
                    let _ = transport.write(&json);
                }
            }
        })),
        ..Default::default()
    };
    
    agent = agent.with_callbacks(callbacks);
    Some(agent)
}
```

**注意**：需要修改 `SessionState` 以持有 `ReplaceableTransport` 的克隆。

### 2.2 扩展 AgentCallbacks（交互阻塞）

**问题**：`AgentCallbacks` 缺少交互请求回调（approval/clarify/sudo/secret）。

**方案**：

修改 `crates/hermes-agent/src/agent_callbacks.rs`：

```rust
#[derive(Default)]
pub struct AgentCallbacks {
    // ... 现有回调 ...
    
    /// Called when an approval is requested (tool execution approval).
    pub on_approval_request: Option<Box<dyn Fn(&str, &serde_json::Value) + Send + Sync>>,
    /// Called when a clarification is requested.
    pub on_clarify_request: Option<Box<dyn Fn(&str, &serde_json::Value) + Send + Sync>>,
    /// Called when a sudo password is requested.
    pub on_sudo_request: Option<Box<dyn Fn(&str) + Send + Sync>>,
    /// Called when a secret/key is requested.
    pub on_secret_request: Option<Box<dyn Fn(&str) + Send + Sync>>,
}
```

然后在 `tool_executor.rs` 中调用这些回调（或在 `hermes-server` 层包装）。

### 2.3 扩展事件类型

**文件**：`crates/hermes-server/src/ws/events.rs`

添加 Desktop 需要的事件类型：

```rust
pub mod types {
    // ... 现有事件 ...
    
    pub const TOOL_PROGRESS: &str = "tool.progress";
    pub const REASONING_AVAILABLE: &str = "reasoning.available";
    pub const SUBAGENT_SPAWN_REQUESTED: &str = "subagent.spawn_requested";
    pub const SUBAGENT_START: &str = "subagent.start";
    pub const SUBAGENT_THINKING: &str = "subagent.thinking";
    pub const SUBAGENT_TOOL: &str = "subagent.tool";
    pub const SUBAGENT_PROGRESS: &str = "subagent.progress";
    pub const SUBAGENT_COMPLETE: &str = "subagent.complete";
    pub const TERMINAL_READ_REQUEST: &str = "terminal.read.request";
}
```

### 2.4 扩展事件适配器

**文件**：`crates/hermes-server/src/ws/event_adapter.rs`

扩展 `stream_chunk_to_events` 以支持更多事件：

```rust
pub fn stream_chunk_to_events(chunk: &StreamChunk) -> Vec<JsonRpcEvent> {
    let mut events = Vec::new();
    
    // message.delta
    if let Some(ref delta) = chunk.delta {
        if let Some(ref content) = delta.content {
            events.push(JsonRpcEvent::new(
                crate::ws::events::types::MESSAGE_DELTA,
                None,
                Some(serde_json::json!({ "content": content })),
            ));
        }
        
        // reasoning tokens
        if let Some(ref reasoning) = delta.reasoning {
            events.push(JsonRpcEvent::new(
                crate::ws::events::types::REASONING_DELTA,
                None,
                Some(serde_json::json!({ "content": reasoning })),
            ));
        }
    }
    
    // message.complete
    if let Some(ref reason) = chunk.finish_reason {
        events.push(JsonRpcEvent::new(
            crate::ws::events::types::MESSAGE_COMPLETE,
            None,
            Some(serde_json::json!({ "reason": reason })),
        ));
    }
    
    events
}
```

### 2.5 实现交互阻塞逻辑

**文件**：`crates/hermes-server/src/rpc/prompt.rs`

在 `prompt.submit` 中，当检测到需要交互时阻塞等待：

```rust
// 伪代码
async fn handle_prompt_submit(
    request: JsonRpcRequest,
    state: &AppState,
    session: &SessionState,
) -> Option<JsonRpcResponse> {
    // ... 现有逻辑 ...
    
    // 当 AgentLoop 触发交互请求时
    if let Some(interaction_type) = detect_interaction_request(&result) {
        let interaction_id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = tokio::sync::oneshot::channel::<String>();
        
        // 存入等待表
        state.pending_interactions.insert(interaction_id.clone(), tx);
        
        // 发送交互请求事件到 Desktop
        let event = JsonRpcEvent::new(
            &format!("{}.request", interaction_type),
            Some(&session_id),
            Some(serde_json::json!({
                "interaction_id": interaction_id,
                "message": "...",
            })),
        );
        session.transport.write(&serde_json::to_value(&event).unwrap_or_default());
        
        // 阻塞等待用户响应
        match rx.await {
            Ok(response) => {
                // 用户响应后继续执行
                continue_with_response(response);
            }
            Err(_) => {
                // 超时或取消
                return Some(JsonRpcResponse::err(
                    request.id,
                    JsonRpcError::new(-32000, "Interaction cancelled"),
                ));
            }
        }
    }
}
```

---

## 阶段 3：补齐 Desktop 设置页 REST 端点

### 3.1 Skills REST 端点

**新建**：`crates/hermes-server/src/rest/skills.rs`

```rust
use axum::{extract::State, Json};
use serde_json::json;

use crate::{error::AppError, state::AppState};

/// GET /api/skills
pub async fn get_skills(State(state): State<AppState>) -> Result<Json<serde_json::Value>, AppError> {
    // TODO: 接入真实的 SkillRegistry
    // 目前返回 mock 数据
    Ok(Json(json!({
        "skills": [
            {"name": "web_search", "enabled": true, "description": "Web search"},
            {"name": "code_execution", "enabled": false, "description": "Code execution"},
        ]
    })))
}

/// PUT /api/skills/toggle
pub async fn toggle_skill(
    State(state): State<AppState>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    let skill_name = payload["name"].as_str()
        .ok_or_else(|| AppError::BadRequest("Missing skill name".to_string()))?;
    let enabled = payload["enabled"].as_bool()
        .ok_or_else(|| AppError::BadRequest("Missing enabled flag".to_string()))?;
    
    // TODO: 接入真实的 SkillRegistry
    Ok(Json(json!({
        "status": "ok",
        "skill": skill_name,
        "enabled": enabled,
    })))
}
```

### 3.2 Toolsets REST 端点

**新建**：`crates/hermes-server/src/rest/toolsets.rs`

```rust
use axum::{extract::{Path, State}, Json};
use serde_json::json;

use crate::{error::AppError, state::AppState};

/// GET /api/tools/toolsets
pub async fn list_toolsets(State(state): State<AppState>) -> Result<Json<serde_json::Value>, AppError> {
    // TODO: 接入 ToolRegistry
    Ok(Json(json!({
        "toolsets": [
            {"name": "terminal", "enabled": true},
            {"name": "file_system", "enabled": true},
        ]
    })))
}

/// PUT /api/tools/toolsets/{name}
pub async fn toggle_toolset(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    let enabled = payload["enabled"].as_bool()
        .ok_or_else(|| AppError::BadRequest("Missing enabled flag".to_string()))?;
    
    Ok(Json(json!({
        "status": "ok",
        "toolset": name,
        "enabled": enabled,
    })))
}

/// GET /api/tools/toolsets/{name}/config
pub async fn get_toolset_config(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(json!({
        "toolset": name,
        "config": {}
    })))
}

/// PUT /api/tools/toolsets/{name}/provider
pub async fn set_toolset_provider(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    let provider = payload["provider"].as_str()
        .ok_or_else(|| AppError::BadRequest("Missing provider".to_string()))?;
    
    Ok(Json(json!({
        "status": "ok",
        "toolset": name,
        "provider": provider,
    })))
}

/// POST /api/tools/toolsets/{name}/post-setup
pub async fn run_post_setup(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(json!({
        "status": "ok",
        "toolset": name,
        "message": "Post-setup completed",
    })))
}
```

### 3.3 Cron REST 端点

**新建**：`crates/hermes-server/src/rest/cron.rs`

```rust
use axum::{extract::{Path, State}, Json};
use serde_json::json;

use crate::{error::AppError, state::AppState};

/// GET /api/cron/jobs
pub async fn list_jobs(State(state): State<AppState>) -> Result<Json<serde_json::Value>, AppError> {
    // TODO: 接入 hermes-cron
    Ok(Json(json!({ "jobs": [] })))
}

/// GET /api/cron/jobs/{id}
pub async fn get_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(json!({ "id": id, "status": "active" })))
}

/// GET /api/cron/jobs/{id}/runs
pub async fn get_job_runs(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(json!({ "runs": [] })))
}

/// POST /api/cron/jobs
pub async fn create_job(
    State(state): State<AppState>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(json!({ "status": "ok", "id": "job-123" })))
}

/// PUT /api/cron/jobs/{id}
pub async fn update_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(json!({ "status": "ok", "id": id })))
}

/// POST /api/cron/jobs/{id}/pause
pub async fn pause_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(json!({ "status": "ok", "id": id, "action": "paused" })))
}

/// POST /api/cron/jobs/{id}/resume
pub async fn resume_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(json!({ "status": "ok", "id": id, "action": "resumed" })))
}

/// POST /api/cron/jobs/{id}/trigger
pub async fn trigger_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(json!({ "status": "ok", "id": id, "action": "triggered" })))
}

/// DELETE /api/cron/jobs/{id}
pub async fn delete_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(json!({ "status": "ok", "id": id, "action": "deleted" })))
}
```

### 3.4 Messaging REST 端点

**新建**：`crates/hermes-server/src/rest/messaging.rs`

```rust
use axum::{extract::{Path, State}, Json};
use serde_json::json;

use crate::{error::AppError, state::AppState};

/// GET /api/messaging/platforms
pub async fn list_platforms(State(state): State<AppState>) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(json!({
        "platforms": [
            {"id": "slack", "name": "Slack", "enabled": false},
            {"id": "discord", "name": "Discord", "enabled": false},
        ]
    })))
}

/// PUT /api/messaging/platforms/{id}
pub async fn update_platform(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(json!({ "status": "ok", "platform": id })))
}

/// POST /api/messaging/platforms/{id}/test
pub async fn test_platform(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(json!({ "status": "ok", "platform": id, "connected": true })))
}
```

### 3.5 Profile CRUD

**文件**：`crates/hermes-server/src/rest/profiles.rs`

添加：

```rust
/// POST /api/profiles
pub async fn create_profile(
    State(state): State<AppState>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    let name = payload["name"].as_str()
        .ok_or_else(|| AppError::BadRequest("Missing profile name".to_string()))?;
    
    // 创建 profile 目录
    let profile_home = state.hermes_home.join("profiles").join(name);
    tokio::fs::create_dir_all(&profile_home).await
        .map_err(|e| AppError::Internal(format!("create profile dir: {}", e)))?;
    
    Ok(Json(json!({ "status": "ok", "profile": name })))
}

/// PATCH /api/profiles/{name}
pub async fn rename_profile(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    let new_name = payload["name"].as_str()
        .ok_or_else(|| AppError::BadRequest("Missing new name".to_string()))?;
    
    let old_path = state.hermes_home.join("profiles").join(&name);
    let new_path = state.hermes_home.join("profiles").join(new_name);
    
    tokio::fs::rename(&old_path, &new_path).await
        .map_err(|e| AppError::Internal(format!("rename profile: {}", e)))?;
    
    Ok(Json(json!({ "status": "ok", "old_name": name, "new_name": new_name })))
}

/// DELETE /api/profiles/{name}
pub async fn delete_profile(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let profile_path = state.hermes_home.join("profiles").join(&name);
    
    tokio::fs::remove_dir_all(&profile_path).await
        .map_err(|e| AppError::Internal(format!("delete profile: {}", e)))?;
    
    Ok(Json(json!({ "status": "ok", "profile": name })))
}
```

### 3.6 路由注册

**文件**：`crates/hermes-server/src/server.rs`

在路由中添加：

```rust
// Skills endpoints
.route("/api/skills", get(crate::rest::skills::get_skills))
.route("/api/skills/toggle", put(crate::rest::skills::toggle_skill))

// Toolsets endpoints
.route("/api/tools/toolsets", get(crate::rest::toolsets::list_toolsets))
.route("/api/tools/toolsets/{name}", put(crate::rest::toolsets::toggle_toolset))
.route("/api/tools/toolsets/{name}/config", get(crate::rest::toolsets::get_toolset_config))
.route("/api/tools/toolsets/{name}/provider", put(crate::rest::toolsets::set_toolset_provider))
.route("/api/tools/toolsets/{name}/post-setup", post(crate::rest::toolsets::run_post_setup))

// Cron endpoints
.route("/api/cron/jobs", get(crate::rest::cron::list_jobs))
.route("/api/cron/jobs", post(crate::rest::cron::create_job))
.route("/api/cron/jobs/{id}", get(crate::rest::cron::get_job))
.route("/api/cron/jobs/{id}", put(crate::rest::cron::update_job))
.route("/api/cron/jobs/{id}", delete(crate::rest::cron::delete_job))
.route("/api/cron/jobs/{id}/runs", get(crate::rest::cron::get_job_runs))
.route("/api/cron/jobs/{id}/pause", post(crate::rest::cron::pause_job))
.route("/api/cron/jobs/{id}/resume", post(crate::rest::cron::resume_job))
.route("/api/cron/jobs/{id}/trigger", post(crate::rest::cron::trigger_job))

// Messaging endpoints
.route("/api/messaging/platforms", get(crate::rest::messaging::list_platforms))
.route("/api/messaging/platforms/{id}", put(crate::rest::messaging::update_platform))
.route("/api/messaging/platforms/{id}/test", post(crate::rest::messaging::test_platform))

// Profile CRUD
.route("/api/profiles", post(crate::rest::profiles::create_profile))
.route("/api/profiles/{name}", patch(crate::rest::profiles::rename_profile))
.route("/api/profiles/{name}", delete(crate::rest::profiles::delete_profile))
```

**文件**：`crates/hermes-server/src/rest/mod.rs`

添加模块声明：

```rust
pub mod config;
pub mod cron;
pub mod env;
pub mod events;
pub mod gateway;
pub mod memory;
pub mod messaging;
pub mod models;
pub mod ops;
pub mod profiles;
pub mod sessions;
pub mod skills;
pub mod status;
pub mod toolsets;
```

---

## 验收标准

### 阶段 1 验收
- [ ] Desktop 启动 Rust 后端后，`GET /api/status` 返回 200
- [ ] WebSocket `ws://host:port/api/ws?token=<token>` 连接成功并收到 `gateway.ready`
- [ ] 无 token 的 WebSocket 连接被拒绝（401）
- [ ] `GET /api/logs` 返回日志数据

### 阶段 2 验收
- [ ] Desktop 发送消息后，WebSocket 收到 `tool.start` 事件（含 tool 名称和参数）
- [ ] 工具执行完成后，WebSocket 收到 `tool.complete` 事件（含执行结果）
- [ ] 需要审批的工具调用触发 `approval.request` 事件
- [ ] Desktop 响应审批后，Agent 继续执行
- [ ] 超时或取消时，Agent 收到错误提示

### 阶段 3 验收
- [ ] Desktop 设置页 Skills 标签能加载技能列表并切换启用状态
- [ ] Desktop 设置页 Toolsets 标签能加载工具集列表并配置
- [ ] Desktop 设置页 Cron 标签能创建、编辑、删除定时任务
- [ ] Desktop 设置页 Messaging 标签能配置消息平台
- [ ] Desktop Profile 管理能创建、重命名、删除 Profile

---

## 风险评估

| 风险 | 概率 | 影响 | 缓解措施 |
|------|------|------|---------|
| **Token 环境变量未传递** | 中 | 致命 | 检查 Desktop 启动代码，确认注入逻辑 |
| **AgentCallbacks 修改影响测试** | 中 | 中 | 修改前运行 `cargo test -p hermes-agent` |
| **ReplaceableTransport 回调性能** | 低 | 低 | 使用 `try_write` 避免阻塞 |
| **事件格式与 Desktop 预期不符** | 中 | 高 | 参考 Python 代码和 Desktop 前端代码调试 |
| **ToolRegistry/CronManager API 不完整** | 高 | 中 | 先 mock 实现，标记 TODO，后续接入真实 API |
| **交互阻塞超时处理** | 中 | 中 | 设置 60s 默认超时，可配置 |

---

## 时间估算

| 阶段 | 任务数 | 预计时间 |
|------|--------|---------|
| **阶段 1：修复阻断项** | 5 | 1 小时 |
| **阶段 2.1-2.4：工具事件绑定** | 4 | 2-3 小时 |
| **阶段 2.5：交互阻塞** | 1 | 1-2 小时 |
| **阶段 3：设置页 REST** | 5 个文件 | 3-4 小时 |
| **总计** | — | **7-10 小时** |

---

## 附录：Desktop 事件格式参考

### tool.start

```json
{
  "jsonrpc": "2.0",
  "method": "event",
  "params": {
    "type": "tool.start",
    "session_id": "abc123",
    "payload": {
      "tool": "execute_code",
      "args": {
        "code": "print('hello')",
        "language": "python"
      }
    }
  }
}
```

### tool.complete

```json
{
  "jsonrpc": "2.0",
  "method": "event",
  "params": {
    "type": "tool.complete",
    "session_id": "abc123",
    "payload": {
      "tool": "execute_code",
      "result": "hello\n",
      "is_error": false
    }
  }
}
```

### approval.request

```json
{
  "jsonrpc": "2.0",
  "method": "event",
  "params": {
    "type": "approval.request",
    "session_id": "abc123",
    "payload": {
      "interaction_id": "int-456",
      "tool": "execute_code",
      "message": "The agent wants to execute code. Approve?"
    }
  }
}
```

### approval.respond (Desktop → Server)

```json
{
  "jsonrpc": "2.0",
  "method": "approval.respond",
  "params": {
    "interaction_id": "int-456",
    "approved": true
  },
  "id": "req-789"
}
```
