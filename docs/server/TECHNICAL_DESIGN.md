# hermes-server 技术方案

## 1. 概述

`hermes-server` 是 Hermes Ultra 的 Desktop 后端服务 crate，完全参考 Python 版本的 `tui_gateway` 和 `web_server.py` 实现，目的是与 Python 版本功能保持完全一致。

### 1.1 设计目标

- **API 完全一致** — REST 和 WebSocket 端点与 Python 版本保持 100% 兼容
- **协议完全一致** — JSON-RPC 2.0 格式、事件类型、错误码完全复刻
- **独立 crate** — 不依赖 `hermes-http`，不整合其功能
- **Electron 兼容** — 与现有 Electron 主进程的通信方式完全兼容

### 1.2 hermes-cli 集成方式

当前 `hermes-cli` 的 `run_dashboard()` 函数写入 `api_server` 配置后调用 `run_gateway()`，启动的是 OpenAI 兼容 API（端口 8090）。需要改为启动 `hermes-server`。

#### 方案 A：独立二进制（推荐）

`hermes-server` 提供独立的 `main.rs` 二进制入口。`run_dashboard()` 改为：

```rust
// hermes-cli/src/main.rs
async fn run_dashboard(cli: Cli, host: String, port: u16, ...) -> Result<(), AgentError> {
    // ... 验证 host/port ...
    
    // 生成 session token 并启动 hermes-server
    hermes_server::run_server(addr, config).await
}
```

优点：独立进程，与 Gateway 完全解耦。

#### 方案 B：库调用

`hermes-server` 作为库被 `hermes-cli` 调用：

```rust
// hermes-cli 的 Cargo.toml 添加依赖
hermes-server = { path = "../hermes-server" }

// run_dashboard() 中调用
hermes_server::Server::new(config)
    .with_host(host)
    .with_port(port)
    .run()
    .await
```

#### 推荐：方案 A

- 与 Python 版本一致（web_server 是独立进程）
- Electron 主进程可以直接 spawn `hermes-ultra dashboard` 子进程
- 不影响现有 gateway run 逻辑

### 1.3 与现有 crate 的关系

| crate | 关系 | 说明 |
|-------|------|------|
| `hermes-http` | 无关 | 用户后续手动删除 |
| `hermes-gateway` | 依赖 | 使用 Gateway 核心、SessionManager、ToolRegistry |
| `hermes-agent` | 依赖 | 使用 AgentLoop 执行对话 |
| `hermes-config` | 依赖 | 配置读写 |
| `hermes-cron` | 依赖 | Cron 定时任务管理 |
| `hermes-mcp` | 依赖 | MCP 服务器管理 |
| `hermes-tools` | 依赖 | 工具注册表 |
| `hermes-core` | 依赖 | 核心类型和错误定义 |

## 2. 架构设计

### 2.1 目录结构

```
crates/hermes-server/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── main.rs                    # 入口：hermes-ultra dashboard
│   ├── server.rs                  # axum 服务器启动
│   ├── state.rs                   # AppState 全局状态
│   ├── error.rs                   # AppError 类型
│   │
│   ├── rest/                      # REST API 路由
│   │   ├── mod.rs
│   │   ├── status.rs              # GET /api/status, /api/system/stats
│   │   ├── config.rs              # /api/config/*
│   │   ├── env.rs                 # /api/env/*
│   │   ├── sessions.rs            # /api/sessions/*
│   │   ├── models.rs              # /api/model/*
│   │   ├── skills.rs              # /api/skills/*
│   │   ├── plugins.rs             # /api/plugins/*
│   │   ├── mcp.rs                 # /api/mcp/*
│   │   ├── cron.rs                # /api/cron/*
│   │   ├── profiles.rs            # /api/profiles/*
│   │   ├── ops.rs                 # /api/ops/*
│   │   ├── memory.rs              # /api/memory/*
│   │   └── gateway.rs             # /api/gateway/*
│   │
│   ├── ws/                        # WebSocket 处理
│   │   ├── mod.rs
│   │   ├── rpc.rs                 # JSON-RPC 2.0 协议解析
│   │   ├── handler.rs             # /api/ws 连接处理器
│   │   ├── transport.rs           # Transport trait + WsTransport
│   │   ├── auth.rs                # WS 认证（token/ticket）
│   │   └── events.rs              # 事件推送系统
│   │
│   ├── rpc/                       # JSON-RPC 方法处理器
│   │   ├── mod.rs
│   │   ├── session.rs             # session.* 方法
│   │   ├── prompt.rs              # prompt.* 方法
│   │   ├── model.rs               # model.* 方法
│   │   ├── config_rpc.rs          # config.* 方法
│   │   ├── tools.rs               # tools.*, toolsets.*
│   │   ├── skills.rs              # skills.*, plugins.*
│   │   ├── slash.rs               # slash.*, command.*, cli.*
│   │   ├── cron_rpc.rs            # cron.manage
│   │   ├── delegation.rs          # delegation.*, subagent.*, spawn_tree.*
│   │   └── misc.rs                # 其余方法
│   │
│   └── core/                      # 核心业务逻辑
│       ├── mod.rs
│       ├── session.rs             # 会话状态管理
│       ├── agent.rs               # Agent 生命周期
│       ├── env_store.rs           # .env 文件 CRUD
│       └── ticket.rs              # WS ticket 机制
```

### 2.2 核心类型定义

#### JSON-RPC 协议类型

```rust
// 请求
#[derive(Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: Option<String>,  // "2.0"
    pub method: String,
    pub params: Option<serde_json::Value>,
    pub id: Option<serde_json::Value>,
}

// 成功响应
#[derive(Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,  // "2.0"
    pub id: Option<serde_json::Value>,
    pub result: Option<serde_json::Value>,
    pub error: Option<JsonRpcError>,
}

// 错误响应
#[derive(Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

// 事件（服务端推送）
#[derive(Serialize)]
pub struct JsonRpcEvent {
    pub jsonrpc: String,  // "2.0"
    pub method: String,   // "event"
    pub params: EventParams,
}

#[derive(Serialize)]
pub struct EventParams {
    #[serde(rename = "type")]
    pub event_type: String,
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
}
```

#### Transport trait

```rust
#[async_trait]
pub trait Transport: Send + Sync {
    /// 写入 JSON 帧，返回 false 表示对端已断开
    fn write(&self, obj: &serde_json::Value) -> bool;
    
    /// 关闭传输
    fn close(&self);
}

// WebSocket 实现
pub struct WsTransport {
    tx: tokio::sync::mpsc::Sender<String>,
    closed: AtomicBool,
}
```

#### AppState

```rust
pub struct AppState {
    // 全局状态
    pub sessions: Arc<RwLock<HashMap<String, SessionState>>>,
    pub config: Arc<RwLock<GatewayConfig>>,
    pub hermes_home: PathBuf,
    
    // 会话管理
    pub session_limit: usize,
    pub session_ttl: Duration,
    
    // WS 认证
    pub session_token: String,
    pub internal_credential: String,
    
    // DB
    pub db: Arc<SessionPersistence>,
}

pub struct SessionState {
    pub agent: Option<Arc<AgentLoop>>,
    pub agent_ready: Arc<Notify>,
    pub transport: Arc<dyn Transport>,
    pub history: Vec<Message>,
    pub history_version: AtomicU64,
    pub running: AtomicBool,
    pub cwd: PathBuf,
    pub cols: u32,
    // ... 更多字段
}
```

## 3. REST API 设计

### 3.1 路由前缀

所有 REST API 使用 `/api/` 前缀，与 Python 版本保持一致。

### 3.2 错误格式

统一使用 `{"detail": "error message"}` 格式，HTTP 状态码：400, 401, 403, 404, 413, 429, 500, 502, 504。

### 3.3 端点清单

#### 系统状态
```
GET  /api/status                     # 服务状态
GET  /api/system/stats               # 系统信息
```

#### 配置管理
```
GET  /api/config                     # 获取配置
PUT  /api/config                     # 保存配置
GET  /api/config/defaults            # 默认配置
GET  /api/config/schema              # 配置 schema
GET  /api/config/raw                 # 原始 YAML
PUT  /api/config/raw                 # 保存原始 YAML
```

#### 环境变量
```
GET    /api/env                      # 列出环境变量
PUT    /api/env                      # 设置环境变量
DELETE /api/env                      # 删除环境变量
POST   /api/env/reveal               # 揭示真实值
```

#### 会话管理
```
GET    /api/sessions                 # 列出会话
GET    /api/sessions/search          # 搜索会话
GET    /api/sessions/stats           # 会话统计
GET    /api/sessions/empty/count     # 空会话计数
DELETE /api/sessions/empty           # 批量删除空会话
POST   /api/sessions/bulk-delete     # 批量删除
GET    /api/sessions/:id             # 会话详情
GET    /api/sessions/:id/messages    # 会话消息
DELETE /api/sessions/:id             # 删除会话
PATCH  /api/sessions/:id             # 更新会话
GET    /api/sessions/:id/export      # 导出会话
POST   /api/sessions/prune           # 清理旧会话
```

#### 模型管理
```
GET  /api/model/info                 # 模型元数据
GET  /api/model/options              # 可用模型列表
GET  /api/model/recommended-default  # 推荐默认模型
GET  /api/model/auxiliary             # 辅助模型
POST /api/model/set                  # 分配模型
```

#### 技能/插件
```
GET    /api/skills                   # 列出技能
PUT    /api/skills/toggle            # 启用/禁用技能
GET    /api/plugins                  # 列出插件
POST   /api/plugins/manage           # 管理插件
```

#### MCP 服务器
```
GET    /api/mcp/servers              # 列出 MCP 服务器
POST   /api/mcp/servers              # 添加 MCP 服务器
DELETE /api/mcp/servers/:name        # 删除 MCP 服务器
POST   /api/mcp/servers/:name/test   # 测试 MCP 连接
PUT    /api/mcp/servers/:name/enabled # 启用/禁用
```

#### Cron 定时任务
```
GET    /api/cron/jobs                # 列出任务
POST   /api/cron/jobs                # 创建任务
GET    /api/cron/jobs/:id            # 任务详情
PUT    /api/cron/jobs/:id            # 更新任务
DELETE /api/cron/jobs/:id            # 删除任务
POST   /api/cron/jobs/:id/pause      # 暂停任务
POST   /api/cron/jobs/:id/resume     # 恢复任务
POST   /api/cron/jobs/:id/trigger    # 立即触发
GET    /api/cron/jobs/:id/runs       # 运行历史
GET    /api/cron/delivery-targets    # 投递目标
```

#### Profile 管理
```
GET    /api/profiles                 # 列出 profiles
POST   /api/profiles                 # 创建 profile
GET    /api/profiles/active          # 当前 profile
POST   /api/profiles/active          # 切换 profile
PATCH  /api/profiles/:name           # 重命名
DELETE /api/profiles/:name           # 删除
```

#### Gateway 管理
```
POST   /api/gateway/restart          # 重启 gateway
POST   /api/gateway/start            # 启动 gateway
POST   /api/gateway/stop             # 停止 gateway
```

#### 操作/诊断
```
POST   /api/ops/doctor               # 诊断检查
POST   /api/ops/backup               # 备份
POST   /api/ops/import               # 导入
POST   /api/ops/dump                 # 状态转储
GET    /api/ops/logs                 # 读取日志
```

#### 记忆系统
```
GET    /api/memory                   # 记忆状态
PUT    /api/memory/provider          # 选择 provider
POST   /api/memory/reset             # 重置记忆
```

## 4. WebSocket 设计

### 4.0 线路协议（Wire Protocol）

WebSocket 上的 JSON-RPC 2.0 使用 **换行分隔的 JSON**（newline-delimited JSON）作为帧协议：

```
<json_frame>\n
```

**示例**：
```
{"jsonrpc":"2.0","method":"session.create","params":{"cols":120},"id":"req-1"}\n
{"jsonrpc":"2.0","id":"req-1","result":{"session_id":"abc123"}}\n
{"jsonrpc":"2.0","method":"event","params":{"type":"gateway.ready","payload":{}}}\n
```

**实现要点**：
- 每帧以 `\n` 结尾
- JSON 内部不使用 `\n`（紧凑格式 `serde_json::to_string`）
- 发送端：`serde_json::to_string(&msg) + "\n"`
- 接收端：按 `\n` 分割，解析每行为独立 JSON 帧
- 与 Python 版本的 `json.dumps(obj, ensure_ascii=False) + "\n"` 完全一致

### 4.1 JSON-RPC 2.0 协议

#### 请求格式
```json
{
    "jsonrpc": "2.0",
    "method": "session.create",
    "params": {"cols": 120, "cwd": "/path"},
    "id": "req-1"
}
```

#### 响应格式
```json
{
    "jsonrpc": "2.0",
    "id": "req-1",
    "result": {"session_id": "abc123", ...}
}
```

#### 错误响应
```json
{
    "jsonrpc": "2.0",
    "id": "req-1",
    "error": {"code": 4001, "message": "session not found"}
}
```

#### 事件（服务端推送）
```json
{
    "jsonrpc": "2.0",
    "method": "event",
    "params": {
        "type": "thinking.delta",
        "session_id": "abc123",
        "payload": {"text": "token..."}
    }
}
```

### 4.2 认证模式

| 模式 | 机制 | 场景 |
|------|------|------|
| Token | `?token=<session_token>` | 本地 Electron 连接 |
| Ticket | `?ticket=<single-use>` (30s TTL) | OAuth 远程模式 |

### 4.3 事件类型

| 事件类型 | 说明 |
|----------|------|
| `gateway.ready` | 网关启动就绪 |
| `session.info` | 会话元数据更新 |
| `thinking.delta` | 推理 token 流式输出 |
| `tool.start` | 工具调用开始 |
| `tool.complete` | 工具调用完成 |
| `status.update` | 状态更新 |
| `approval.request` | 工具审批请求 |
| `clarify.request` | 澄清选择请求 |

### 4.4 JSON-RPC 方法

#### Batch 1：会话管理
- `session.create`, `session.list`, `session.resume`, `session.close`
- `session.history`, `session.interrupt`, `session.title`, `session.usage`
- `prompt.submit`, `prompt.background`

#### Batch 2：模型/配置
- `model.options`, `model.save_key`, `model.disconnect`
- `config.get`, `config.set`, `config.show`
- `setup.status`, `setup.runtime_check`

#### Batch 3：工具/技能
- `tools.list`, `tools.show`, `tools.configure`, `toolsets.list`
- `skills.manage`, `skills.reload`
- `plugins.list`, `plugins.manage`

#### Batch 4：命令/补全
- `slash.exec`, `command.resolve`, `command.dispatch`
- `complete.path`, `complete.slash`

#### Batch 5：定时任务/子代理
- `cron.manage`, `agents.list`
- `delegation.status`, `delegation.pause`
- `subagent.interrupt`, `spawn_tree.save`, `spawn_tree.list`

#### Batch 6：交互响应
- `clarify.respond`, `approval.respond`, `sudo.respond`, `secret.respond`

## 5. 并发模型

### 5.1 运行时

使用 tokio 异步运行时，长耗时操作使用 `spawn_blocking`。

### 5.2 会话状态锁

使用 `tokio::sync::RwLock` 保护会话状态。

### 5.3 Agent 构建

延迟构建，通过 `tokio::sync::Notify` 通知就绪。

### 5.4 Transport 写入

使用 `tokio::sync::mpsc::channel` 实现跨线程安全写入。

### 5.5 阻塞式用户交互

Python 版本使用 `threading.Event` + `wait(timeout)` 实现审批/澄清等需要用户响应的阻塞操作。Rust 使用 `tokio::sync::oneshot::channel` 对应：

```rust
// 服务端：创建 oneshot channel，发送事件到客户端，等待响应
let (tx, rx) = tokio::sync::oneshot::channel();
pending_approvals.insert(request_id, tx);
emit("approval.request", session_id, payload);

// 等待用户响应（带超时）
let choice = tokio::time::timeout(Duration::from_secs(60), rx)
    .await
    .map_err(|_| "approval timeout")??;

// 客户端：通过 JSON-RPC 方法返回响应
// → approval.respond { request_id, choice }
// 服务端收到后：pending_approvals.remove(request_id).send(choice)
```

涉及的方法和对应交互流：

| 事件（服务端→客户端） | 响应方法（客户端→服务端） | 说明 |
|------------------------|---------------------------|------|
| `approval.request` | `approval.respond` | 工具执行审批 |
| `clarify.request` | `clarify.respond` | 澄清选择 |
| `sudo.request` | `sudo.respond` | sudo 密码输入 |
| `secret.request` | `secret.respond` | API key/密钥输入 |

## 6. JSON-RPC 错误码

### 6.1 标准 JSON-RPC 错误码

| code | 含义 | 场景 |
|------|------|------|
| `-32700` | Parse error | WS 收到非法 JSON |
| `-32600` | Invalid request | 请求不是 dict，或 method 非法 |
| `-32601` | Method not found | 方法名不在注册表中 |
| `-32602` | Invalid params | params 不是 dict |
| `-32603` | Internal error | dispatch 入口异常 |

### 6.2 业务错误码（4000-4099：客户端错误）

| code | 含义 |
|------|------|
| 4001 | no active session / session_id required |
| 4002 | config value required / unknown config key |
| 4003 | argv must be list[str] |
| 4004 | text required / empty command / invalid count |
| 4005 | blocked (dangerous/hardline command) |
| 4006 | session_id required / managed install |
| 4007 | session not found |
| 4008 | nothing to branch |
| 4009 | session busy |
| 4010 | agent does not support steer |
| 4011 | unknown command |
| 4012 | text/url required |
| 4013 | unknown voice action |
| 4014 | hash required / voice mode is off |
| 4015 | path required / unknown browser url |
| 4016 | cwd required / unsupported extension |
| 4017 | unknown skills action / working directory does not exist |
| 4018 | not a command / undo target gone |
| 4020 | text required (voice.tts) |
| 4021 | title required |
| 4022 | invalid/duplicate title |
| 4023 | cannot delete an active session |
| 4030 | path outside spawn-trees root |
| 4090 | active session limit |

### 6.3 业务错误码（5000-5099：服务端错误）

| code | 含义 |
|------|------|
| 5000 | resume failed / general internal |
| 5001 | config write error / shell.unavailable |
| 5002 | command timed out |
| 5003 | command error |
| 5004 | approval module error |
| 5005 | compress error |
| 5006 | state.db unavailable |
| 5007 | session title error |
| 5008 | branch/undo DB error |
| 5010 | process.stop error |
| 5011 | save failed |
| 5012 | command resolve error |
| 5013 | provider list error |
| 5015 | reload error |
| 5016 | setup/cli error |
| 5017 | insights DB error |
| 5020 | slash/rollback error |
| 5025 | voice unavailable / skills.reload |
| 5030 | slash worker error / config show error |
| 5031 | browser/tools error |
| 5032 | agent init timeout / plugins error |
| 5033 | model options / agents list error |
| 5034 | model save key / tools show error |
| 5035 | model disconnect / tools configure error |
| 5036 | session enumeration/delete error |

## 7. Profile 隔离设计

### 7.1 设计目标

支持 Desktop 应用的多 profile 功能，每个 profile 拥有独立的配置、会话数据库和状态。

### 7.2 实现方案（选项 A）

AppState 持有一个 `active_profile` 字符串，所有操作按此 profile 路由：

```rust
pub struct AppState {
    /// 当前激活的 profile 名称
    pub active_profile: Arc<RwLock<String>>,
    
    /// Profile 到配置的映射
    pub profile_configs: Arc<RwLock<HashMap<String, GatewayConfig>>>,
    
    /// 全局状态（原有字段）
    pub config: Arc<RwLock<GatewayConfig>>,
    pub hermes_home: PathBuf,
    pub session_token: String,
    pub sessions: Arc<RwLock<HashMap<String, SessionState>>>,
}
```

### 7.3 Profile 目录结构

```
HERMES_HOME/
├── config.yaml              # 默认配置
├── state.db                 # 默认 profile 数据库
└── profiles/
    ├── default/
    │   ├── config.yaml      # profile 配置
    │   └── state.db         # profile 数据库
    ├── work/
    │   ├── config.yaml
    │   └── state.db
    └── personal/
        ├── config.yaml
        └── state.db
```

### 7.4 Profile 路由逻辑

```rust
impl AppState {
    /// 获取当前 profile 的 hermes_home 路径
    pub fn profile_home(&self, profile: Option<&str>) -> PathBuf {
        let profile = profile.unwrap_or("default");
        if profile == "default" {
            self.hermes_home.clone()
        } else {
            self.hermes_home.join("profiles").join(profile)
        }
    }
    
    /// 获取当前 profile 的 SessionPersistence
    pub fn profile_persistence(&self, profile: Option<&str>) -> Result<SessionPersistence, AgentError> {
        let home = self.profile_home(profile);
        Ok(SessionPersistence::new(&home))
    }
    
    /// 跨 profile 聚合会话列表
    pub async fn aggregate_sessions(&self, limit: usize, offset: usize) -> Vec<SessionRecord> {
        // 遍历所有 profile 目录，只读打开 state.db
        // 合并排序后分页
    }
}
```

### 7.5 与 Desktop 的集成

- **Profile 切换**：`POST /api/profiles/active` 更新 `active_profile`，触发 `invalidate_agent_caches()`
- **Per-profile 会话**：`GET /api/sessions?profile=work` 读取指定 profile 的会话
- **跨 profile 聚合**：`GET /api/profiles/sessions` 合并所有 profile 的会话列表

## 8. 认证设计

### 8.1 Token 认证（本地模式）

**HTTP Header**：
```
X-Hermes-Session-Token: <32-char-token>
```

**Bearer 回退**：
```
Authorization: Bearer <token>
```

**WS Query**：
```
ws://127.0.0.1:9133/api/ws?token=<token>
```

### 8.2 OAuth 认证（远程模式）

**Cookie**：
- `hermes_session_at`：访问 token（15 分钟）
- `hermes_session_rt`：刷新 token（24 小时）

**Ticket 机制**：
```
POST /api/auth/ws-ticket  →  { "ticket": "<uuid>" }
ws://host/api/ws?ticket=<ticket>  // 30s TTL，单次使用
```

### 8.3 中间件实现

```rust
pub async fn request_guard(
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let path = req.uri().path();
    
    // 公共端点白名单
    if is_public_path(path) {
        return Ok(next.run(req).await);
    }
    
    // 验证 token
    let token = req.headers()
        .get("X-Hermes-Session-Token")
        .or_else(|| extract_bearer_token(req.headers()));
    
    match token {
        Some(t) if verify_token(t, &state.session_token) => Ok(next.run(req).await),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}
```

## 9. 交互事件系统

### 9.1 阻塞式交互流程

```rust
// 全局等待表
pub type PendingApprovals = Arc<RwLock<HashMap<String, oneshot::Sender<String>>>>;

// 服务端发送请求事件
async fn request_approval(
    session_id: &str,
    tool_name: &str,
    pending: &PendingApprovals,
    transport: &dyn Transport,
) -> Result<String, AgentError> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = tokio::sync::oneshot::channel();
    
    // 注册等待
    pending.write().await.insert(request_id.clone(), tx);
    
    // 发送事件到客户端
    transport.write(&json!({
        "jsonrpc": "2.0",
        "method": "event",
        "params": {
            "type": "approval.request",
            "session_id": session_id,
            "payload": {
                "request_id": request_id,
                "tool_name": tool_name,
            }
        }
    }));
    
    // 等待响应（60s 超时）
    match tokio::time::timeout(Duration::from_secs(60), rx).await {
        Ok(Ok(choice)) => Ok(choice),
        Ok(Err(_)) => Err(AgentError::Internal("approval channel closed".into())),
        Err(_) => {
            pending.write().await.remove(&request_id);
            Err(AgentError::Internal("approval timeout".into()))
        }
    }
}

// 客户端通过 JSON-RPC 返回响应
// approval.respond { "request_id": "...", "choice": "allow" }
// 服务端：pending.remove(request_id).send(choice)
```

### 9.2 支持的交互类型

| 事件（服务端→客户端） | 响应方法（客户端→服务端） | 说明 |
|------------------------|---------------------------|------|
| `approval.request` | `approval.respond` | 工具执行审批 |
| `clarify.request` | `clarify.respond` | 澄清选择 |
| `sudo.request` | `sudo.respond` | sudo 密码输入 |
| `secret.request` | `secret.respond` | API key/密钥输入 |

### 9.3 超时与清理

- **默认超时**：60 秒（可配置）
- **超时后**：自动拒绝（`deny`）
- **清理**：session 关闭时清除该 session 的所有 pending 请求

## 10. 依赖关系
