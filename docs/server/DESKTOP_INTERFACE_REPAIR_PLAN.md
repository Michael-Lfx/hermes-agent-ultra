# Desktop 接口兼容性修复计划

> **版本**：2026-06-11  
> **适用范围**：`hermes-server` (Rust) → Desktop Electron App  
> **当前兼容性评分**：~75%  
> **目标兼容性评分**：≥95%

---

## 一、审查结论摘要

### 1.1 已实现（完全兼容）

| 类别 | 覆盖情况 |
|------|----------|
| 核心会话生命周期 | ✅ `session.create/resume/close/interrupt/usage` |
| 配置/环境变量 | ✅ `config.get/set/show`, `env` CRUD, `env/reveal` |
| 模型管理 | ✅ `model.info/options/set/recommended-default/auxiliary` |
| 技能/工具管理 | ✅ `skills` CRUD, `toolsets` CRUD + config |
| WebSocket 实时事件 | ✅ `gateway.ready`, `session.info`, `message.*`, `thinking.delta`, `reasoning.delta`, `status.update`, `tool.*`, `approval/clarify/sudo/secret.request`, `background.complete`, `error` |
| 音频功能 | ✅ `audio/transcribe`, `audio/speak`, `audio/elevenlabs/voices` |
| 高频 RPC | ✅ `slash.exec`, `reload.mcp/env`, `complete.path/slash` |
| Cron / Messaging / Profiles | ✅ 完整 CRUD |

### 1.2 缺失（不兼容）

| # | 缺失项 | Desktop 用途 | 优先级 | 预估工作量 |
|---|--------|-------------|--------|-----------|
| 1 | `GET /api/sessions?archived=` 归档过滤 | 会话列表按归档状态过滤 | 🟡 P1 | 4h |
| 2 | `session.steer` JSON-RPC | Agent 生成中实时引导方向 | 🟡 P1 | 6h |
| 3 | `image.attach` / `image.attach_bytes` | 图片上传到会话 | 🟡 P1 | 6h |
| 4 | `file.attach` | 非图片文件上传到会话 | 🟡 P1 | 6h |
| 5 | `handoff.request/state/fail` | 会话移交到消息平台 | 🟠 P2 | 8h |
| 6 | `GET/PUT /api/profiles/{name}/soul` | Profile 个性化配置 | 🟡 P1 | 4h |
| 7 | `GET /api/profiles/{name}/setup-command` | Profile 初始化命令 | 🟡 P1 | 2h |
| 8 | `GET /api/logs?file=&lines=&level=&component=` | 日志过滤查询 | 🟡 P1 | 3h |
| 9 | `reasoning.available` WebSocket 事件 | Reasoning 内容可用通知 | 🟡 P1 | 2h |
| 10 | `skin.changed` WebSocket 事件 | 主题变更通知 | 🔵 P3 | 1h |
| 11 | `/api/analytics/usage` | 使用统计 | 🟠 P2 | 4h |
| 12 | `/api/hermes/update` + `/check` | 后端更新触发/检查 | 🔵 P3 | 4h |
| 13 | `/api/actions/{name}/status` | Action 执行状态 | 🟠 P2 | 3h |

**注**：OAuth 认证相关端点（`/api/auth/*`, `/api/providers/*`）按原始计划已延期。

---

## 二、详细修复方案

### 阶段 1：数据库与核心修复（第 1 天）

#### 任务 1.1：会话表添加 `archived` 字段

**问题描述**：  
Desktop 发送 `GET /api/sessions?archived=exclude&order=recent`，但后端 `ListSessionsQuery` 无 `archived` 字段，参数被静默忽略。用户无法过滤已归档会话。

**影响范围**：  
- 前端会话列表显示混乱（已归档会话仍显示）
- 归档/取消归档功能后端无持久化

**实施步骤**：

1. **数据库 Schema 迁移**（`crates/hermes-agent/src/session_persistence/schema.rs`）
   ```rust
   // 在 SESSIONS_COLUMNS 中添加
   ("archived", "INTEGER NOT NULL DEFAULT 0")
   ```
   `reconcile_table` 会自动为旧数据库添加该列。

2. **SessionRecord 结构体扩展**（`crates/hermes-agent/src/session_persistence/queries.rs`）
   ```rust
   pub struct SessionRecord {
       // ... 现有字段 ...
       pub archived: bool,
   }
   ```
   在 `row_to_session` 中读取 `archived` 列（默认 `false`）。

3. **查询层支持**（`crates/hermes-agent/src/session_persistence/mod.rs`）
   - 在 `end_session` 中支持 `end_reason = "archived"` 时设置 `archived = 1`
   - 添加 `archive_session(session_id: &str, archived: bool) -> Result<(), AgentError>`

4. **REST API 适配**（`crates/hermes-server/src/rest/sessions.rs`）
   ```rust
   #[derive(Debug, Deserialize)]
   pub struct ListSessionsQuery {
       // ... 现有字段 ...
       pub archived: Option<String>, // "exclude" | "include" | "only"
   }
   ```
   在 `list_sessions` 中根据值过滤：
   - `"exclude"` → `WHERE archived = 0`
   - `"only"` → `WHERE archived = 1`
   - `"include"` 或 `None` → 不添加过滤

   在 `UpdateSession` 中添加 `pub archived: Option<bool>`，并在 `update_session` 中处理。

5. **Profile 聚合修正**（`crates/hermes-server/src/rest/profiles.rs`）
   将硬编码的 `"archived": false` 改为从 `SessionRecord.archived` 读取。

**验证命令**：
```bash
cargo test -p hermes-server
curl "http://localhost:9120/api/sessions?archived=exclude"
```

---

#### 任务 1.2：实现 `session.steer` JSON-RPC 方法

**问题描述**：  
Desktop 在 agent 生成过程中允许用户输入文本实时引导方向（如输入"简短一点"），但后端无此 RPC。

**影响范围**：  
- 用户无法实时干预 agent 生成内容
- 长回复场景下用户体验差

**实施步骤**：

1. **SessionState 扩展**（`crates/hermes-server/src/core/session.rs`）
   ```rust
   pub struct SessionState {
       // ... 现有字段 ...
       pub steer_queue: std::sync::Mutex<Vec<String>>,
   }
   
   impl SessionState {
       pub fn push_steer(&self, text: String) {
           if let Ok(mut guard) = self.steer_queue.lock() {
               guard.push(text);
           }
       }
       
       pub fn drain_steer(&self) -> Vec<String> {
           self.steer_queue.lock().map(|mut g| g.drain(..).collect()).unwrap_or_default()
       }
   }
   ```

2. **RPC 处理器实现**（`crates/hermes-server/src/rpc/prompt.rs`）
   ```rust
   pub async fn handle_steer(
       request: JsonRpcRequest,
       state: &AppState,
   ) -> Option<JsonRpcResponse> {
       let params = request.params.as_ref()?.as_object()?;
       let session_id = params.get("session_id")?.as_str()?;
       let text = params.get("text")?.as_str()?;
       
       let sessions = state.sessions.read().await;
       let session = sessions.get(session_id)?;
       
       if !session.is_running() {
           return Some(JsonRpcResponse::err(
               request.id,
               JsonRpcError::server_error(4003, "session not running".into()),
           ));
       }
       
       session.push_steer(text.to_string());
       
       Some(JsonRpcResponse::ok(request.id, json!({ "accepted": true })))
   }
   ```

3. **Steer 注入 agent**（`crates/hermes-server/src/rpc/prompt.rs` 的 `handle_submit`）
   - 在 `RunConversationParams` 构建前，调用 `session.drain_steer()`
   - 如果有 steer 文本，将其追加到 `user_message` 中：
     ```rust
     let steer_texts = session.drain_steer();
     let final_message = if steer_texts.is_empty() {
         text_owned
     } else {
         format!("{}\n\n[User guidance: {}]", text_owned, steer_texts.join("; "))
     };
     ```

4. **路由注册**（`crates/hermes-server/src/rpc/mod.rs`）
   ```rust
   "session.steer" => prompt::handle_steer(request, state).await,
   ```

**验证命令**：
```bash
cargo test -p hermes-server
# Desktop 测试：agent 生成中输入文本，观察输出变化
```

---

### 阶段 2：文件上传功能（第 2-3 天）

#### 任务 2.1：`image.attach` / `image.attach_bytes`

**问题描述**：  
Desktop 通过拖放或粘贴发送图片时，调用 `image.attach`（本地文件路径）或 `image.attach_bytes`（base64 数据），后端无实现。

**影响范围**：  
- 用户无法发送图片给 agent
- 截图、图片分析等功能完全不可用

**实施步骤**：

1. **新建附件模块**（`crates/hermes-server/src/rpc/attachment.rs`）

2. **实现 `handle_image_attach`**
   ```rust
   pub async fn handle_image_attach(
       request: JsonRpcRequest,
       state: &AppState,
   ) -> Option<JsonRpcResponse> {
       let params = request.params.as_ref()?.as_object()?;
       let session_id = params.get("session_id")?.as_str()?;
       
       // 本地模式：path 参数
       if let Some(path) = params.get("path").and_then(|v| v.as_str()) {
           let url = save_local_image(state, session_id, path).await?;
           return Some(JsonRpcResponse::ok(request.id, json!({ "ok": true, "url": url })));
       }
       
       // 远程模式：content_base64 + filename
       let content_base64 = params.get("content_base64")?.as_str()?;
       let filename = params.get("filename")?.as_str()?;
       let url = save_base64_image(state, session_id, content_base64, filename).await?;
       
       Some(JsonRpcResponse::ok(request.id, json!({ "ok": true, "url": url })))
   }
   ```

3. **实现 `handle_image_attach_bytes`**
   ```rust
   pub async fn handle_image_attach_bytes(
       request: JsonRpcRequest,
       state: &AppState,
   ) -> Option<JsonRpcResponse> {
       let params = request.params.as_ref()?.as_object()?;
       let session_id = params.get("session_id")?.as_str()?;
       let content_base64 = params.get("content_base64")?.as_str()?;
       let filename = params.get("filename")?.as_str()?;
       
       let bytes = BASE64.decode(content_base64).ok()?;
       let ext = Path::new(filename)
           .extension()
           .and_then(|e| e.to_str())
           .unwrap_or("png");
       
       let upload_dir = state.hermes_home.join("uploads").join(session_id);
       tokio::fs::create_dir_all(&upload_dir).await.ok()?;
       
       let file_path = upload_dir.join(format!("{}.{}\", uuid::Uuid::new_v4(), ext));
       tokio::fs::write(&file_path, &bytes).await.ok()?;
       
       let data_url = format!("data:image/{};base64,{}\", ext, BASE64.encode(&bytes));
       
       Some(JsonRpcResponse::ok(request.id, json!({
           "ok": true,
           "url": data_url,
           "file_path": file_path.to_string_lossy(),
       })))
   }
   ```

4. **路由注册**（`rpc/mod.rs`）
   ```rust
   "image.attach" => attachment::handle_image_attach(request, state).await,
   "image.attach_bytes" => attachment::handle_image_attach_bytes(request, state).await,
   ```

**验证命令**：
```bash
cargo test -p hermes-server
# Desktop 测试：拖放 PNG 图片到输入框，确认 agent 能分析图片内容
```

---

#### 任务 2.2：`file.attach`

**问题描述**：  
Desktop 发送非图片文件（如 `.py`, `.md`）时调用 `file.attach`，后端需要保存文件并返回引用 ID。

**影响范围**：  
- 用户无法发送文档、代码文件给 agent
- `@file:` 引用语法无法解析

**实施步骤**：

1. **实现 `handle_file_attach`**（`attachment.rs`）
   ```rust
   pub async fn handle_file_attach(
       request: JsonRpcRequest,
       state: &AppState,
   ) -> Option<JsonRpcResponse> {
       let params = request.params.as_ref()?.as_object()?;
       let session_id = params.get("session_id")?.as_str()?;
       let name = params.get("name")?.as_str()?;
       
       let upload_dir = state.hermes_home.join("uploads").join(session_id);
       tokio::fs::create_dir_all(&upload_dir).await.ok()?;
       
       let file_ref = format!("{}-{}", uuid::Uuid::new_v4().to_string()[..8], name);
       let file_path = upload_dir.join(&file_ref);
       
       // 本地模式
       if let Some(path) = params.get("path").and_then(|v| v.as_str()) {
           tokio::fs::copy(path, &file_path).await.ok()?;
       }
       // 远程模式
       else if let Some(data_url) = params.get("data_url").and_then(|v| v.as_str()) {
           let base64_data = data_url.split_once(',').map(|(_, d)| d)?;
           let bytes = BASE64.decode(base64_data).ok()?;
           tokio::fs::write(&file_path, &bytes).await.ok()?;
       }
       
       Some(JsonRpcResponse::ok(request.id, json!({
           "ok": true,
           "ref": format!("@file:{}", name),
           "file_path": file_path.to_string_lossy(),
       })))
   }
   ```

2. **prompt.submit 中解析 @file: 引用**（`prompt.rs`）
   在 `handle_submit` 中，构建 `user_message` 前解析 `@file:filename`：
   ```rust
   let text_with_files = resolve_file_refs(&text, session_id, &state.hermes_home).await;
   ```
   `resolve_file_refs` 读取对应文件内容并内联到消息中。

3. **路由注册**（`rpc/mod.rs`）
   ```rust
   "file.attach" => attachment::handle_file_attach(request, state).await,
   ```

**验证命令**：
```bash
cargo test -p hermes-server
# Desktop 测试：拖放 .py 文件，确认 agent 能读取并分析代码
```

---

### 阶段 3：会话移交(handoff)（第 4 天）

#### 任务 3.1：`handoff.request` / `handoff.state` / `handoff.fail`

**问题描述**：  
Desktop 支持 `/handoff <platform>` 命令将当前会话移交到消息平台（Telegram/Slack 等）继续对话。

**影响范围**：  
- 用户无法跨平台延续对话
- `/handoff` slash 命令报错

**实施步骤**：

1. **状态存储**（`crates/hermes-server/src/state.rs`）
   ```rust
   #[derive(Debug, Clone, Serialize)]
   pub struct HandoffState {
       pub state: String,      // "pending" | "in_progress" | "completed" | "failed"
       pub platform: String,
       pub started_at: f64,
       pub completed_at: Option<f64>,
       pub error: Option<String>,
   }
   
   // 在 AppState 中添加
   pub handoff_states: Arc<RwLock<HashMap<String, HandoffState>>>,
   ```

2. **新建 handoff 模块**（`crates/hermes-server/src/rpc/handoff.rs`）

3. **实现 `handle_handoff_request`**
   ```rust
   pub async fn handle_handoff_request(
       request: JsonRpcRequest,
       state: &AppState,
   ) -> Option<JsonRpcResponse> {
       let params = request.params.as_ref()?.as_object()?;
       let session_id = params.get("session_id")?.as_str()?;
       let platform = params.get("platform")?.as_str()?;
       
       let handoff = HandoffState {
           state: "pending".to_string(),
           platform: platform.to_string(),
           started_at: chrono::Utc::now().timestamp() as f64,
           completed_at: None,
           error: None,
       };
       
       {
           let mut states = state.handoff_states.write().await;
           states.insert(session_id.to_string(), handoff.clone());
       }
       
       // TODO: 启动异步任务调用消息平台 API 完成实际移交
       // 目前先模拟成功（留待 Phase 6 集成消息平台）
       tokio::spawn(async move {
           tokio::time::sleep(std::time::Duration::from_secs(2)).await;
           // 模拟移交成功
       });
       
       Some(JsonRpcResponse::ok(request.id, json!({
           "ok": true,
           "state": "pending",
       })))
   }
   ```

4. **实现 `handle_handoff_state`**
   ```rust
   pub async fn handle_handoff_state(
       request: JsonRpcRequest,
       state: &AppState,
   ) -> Option<JsonRpcResponse> {
       let params = request.params.as_ref()?.as_object()?;
       let session_id = params.get("session_id")?.as_str()?;
       
       let states = state.handoff_states.read().await;
       let handoff = states.get(session_id)?;
       
       Some(JsonRpcResponse::ok(request.id, json!({
           "state": handoff.state,
           "platform": handoff.platform,
           "error": handoff.error,
       })))
   }
   ```

5. **实现 `handle_handoff_fail`**
   ```rust
   pub async fn handle_handoff_fail(
       request: JsonRpcRequest,
       state: &AppState,
   ) -> Option<JsonRpcResponse> {
       let params = request.params.as_ref()?.as_object()?;
       let session_id = params.get("session_id")?.as_str()?;
       let error = params.get("error").and_then(|v| v.as_str()).unwrap_or("unknown");
       
       {
           let mut states = state.handoff_states.write().await;
           if let Some(handoff) = states.get_mut(session_id) {
               handoff.state = "failed".to_string();
               handoff.error = Some(error.to_string());
           }
       }
       
       Some(JsonRpcResponse::ok(request.id, json!({ "ok": true })))
   }
   ```

6. **路由注册**（`rpc/mod.rs`）
   ```rust
   "handoff.request" => handoff::handle_handoff_request(request, state).await,
   "handoff.state" => handoff::handle_handoff_state(request, state).await,
   "handoff.fail" => handoff::handle_handoff_fail(request, state).await,
   ```

**验证命令**：
```bash
cargo test -p hermes-server
# Desktop 测试：/handoff telegram，观察状态流转
```

---

### 阶段 4：Profile 扩展与日志（第 5 天上午）

#### 任务 4.1：`/api/profiles/{name}/soul` 和 `/api/profiles/{name}/setup-command`

**问题描述**：  
Desktop Profile 设置页面支持读写 soul（个性化提示词）和 setup-command（初始化命令）。

**实施步骤**：

1. **在 `profiles.rs` 中添加端点**
   ```rust
   /// GET /api/profiles/{name}/soul
   pub async fn get_profile_soul(
       State(state): State<AppState>,
       Path(name): Path<String>,
   ) -> Result<Json<serde_json::Value>, AppError> {
       let soul_path = state.profile_home(Some(&name)).join("soul.md");
       let content = if soul_path.exists() {
           tokio::fs::read_to_string(&soul_path).await.unwrap_or_default()
       } else {
           String::new()
       };
       Ok(ok_json(json!({ "content": content })))
   }
   
   /// PUT /api/profiles/{name}/soul
   pub async fn put_profile_soul(
       State(state): State<AppState>,
       Path(name): Path<String>,
       Json(body): Json<serde_json::Value>,
   ) -> Result<Json<serde_json::Value>, AppError> {
       let content = body.get("content").and_then(|v| v.as_str()).unwrap_or("");
       let soul_path = state.profile_home(Some(&name)).join("soul.md");
       tokio::fs::write(&soul_path, content).await
           .map_err(|e| AppError::Internal(format!("write soul: {}", e)))?;
       Ok(ok_json(json!({ "ok": true })))
   }
   
   /// GET /api/profiles/{name}/setup-command
   pub async fn get_profile_setup_command(
       State(state): State<AppState>,
       Path(name): Path<String>,
   ) -> Result<Json<serde_json::Value>, AppError> {
       let setup_path = state.profile_home(Some(&name)).join(".setup_command");
       let command = if setup_path.exists() {
           tokio::fs::read_to_string(&setup_path).await.unwrap_or_default().trim().to_string()
       } else {
           String::new()
       };
       Ok(ok_json(json!({ "command": command })))
   }
   ```

2. **注册路由**（`server.rs`）
   ```rust
   .route("/api/profiles/{name}/soul", get(crate::rest::profiles::get_profile_soul))
   .route("/api/profiles/{name}/soul", put(crate::rest::profiles::put_profile_soul))
   .route("/api/profiles/{name}/setup-command", get(crate::rest::profiles::get_profile_setup_command))
   ```

---

#### 任务 4.2：`/api/logs` 查询参数支持

**问题描述**：  
Desktop 日志页面发送 `GET /api/logs?file=&lines=100&level=INFO&component=agent`，后端忽略参数。

**实施步骤**：

1. **修改 `ops.rs` 的 `get_logs`**
   ```rust
   #[derive(Debug, Deserialize)]
   pub struct LogsQuery {
       pub file: Option<String>,
       pub lines: Option<usize>,
       pub level: Option<String>,
       pub component: Option<String>,
   }
   
   pub async fn get_logs(
       State(state): State<AppState>,
       Query(query): Query<LogsQuery>,
   ) -> Result<Json<serde_json::Value>, AppError> {
       let max_lines = query.lines.unwrap_or(100).min(1000);
       let level_filter = query.level.as_deref().map(|l| l.to_uppercase());
       let component_filter = query.component.as_deref().map(|c| c.to_lowercase());
       
       // 读取日志文件（默认 ~/.hermes/logs/server.log）
       let log_path = state.hermes_home.join("logs").join("server.log");
       let content = if log_path.exists() {
           tokio::fs::read_to_string(&log_path).await.unwrap_or_default()
       } else {
           String::new()
       };
       
       let lines: Vec<String> = content
           .lines()
           .filter(|line| {
               if let Some(ref level) = level_filter {
                   if !line.contains(&format!("{}]", level)) {
                       return false;
                   }
               }
               if let Some(ref component) = component_filter {
                   if !line.to_lowercase().contains(component) {
                       return false;
                   }
               }
               true
           })
           .rev()
           .take(max_lines)
           .collect::<Vec<_>>()
           .into_iter()
           .rev()
           .collect();
       
       Ok(ok_json(json!({
           "lines": lines,
           "count": lines.len(),
       })))
   }
   ```

2. **更新路由签名**（`server.rs`）
   确保使用 `Query` 提取器。

---

### 阶段 5：WebSocket 事件补充（第 5 天下午）

#### 任务 5.1：`reasoning.available` 和 `skin.changed`

**实施步骤**：

1. **添加事件常量**（`events.rs`）
   ```rust
   pub const REASONING_AVAILABLE: &str = "reasoning.available";
   pub const SKIN_CHANGED: &str = "skin.changed";
   ```

2. **发送 `reasoning.available`**（`agent_builder.rs`）
   在 `on_thinking` 回调中，当 thinking 内容积累到一定长度（如 >50 字符）或工具调用开始时：
   ```rust
   on_thinking: Some(Box::new({
       let transport = transport.clone();
       let sid = sid.clone();
       let mut reasoning_sent = false;
       move |text| {
           // 发送 thinking.delta
           let event = JsonRpcEvent::new(
               crate::ws::events::types::THINKING_DELTA,
               Some(sid.clone()),
               Some(json!({ "content": text })),
           );
           // ...
           
           // 首次发送 reasoning.available
           if !reasoning_sent && text.len() > 50 {
               let avail_event = JsonRpcEvent::new(
                   crate::ws::events::types::REASONING_AVAILABLE,
                   Some(sid.clone()),
                   Some(json!({})),
               );
               if let Ok(val) = serde_json::to_value(&avail_event) {
                   let _ = transport.write(&val);
               }
               reasoning_sent = true;
           }
       }
   })),
   ```

3. **`skin.changed` 占位**
   在 `server.rs` 中添加 TODO 注释，留待主题系统实现时补充。

---

## 三、实施路线图

```
第 1 天（阶段 1：数据库与核心）
├─ 上午：任务 1.1 — 添加 archived 字段
│   ├─ Schema 迁移
│   ├─ SessionRecord 扩展
│   ├─ REST API 适配
│   └─ 验证：curl /api/sessions?archived=exclude
│
└─ 下午：任务 1.2 — 实现 session.steer
    ├─ SessionState 扩展
    ├─ RPC 处理器
    ├─ Steer 注入 agent
    └─ 验证：Desktop 实时引导测试

第 2 天（阶段 2.1：图片上传）
├─ 上午：任务 2.1 — image.attach / image.attach_bytes
│   ├─ 新建 attachment.rs
│   ├─ 本地文件保存
│   ├─ Base64 解码保存
│   └─ 路由注册
│
└─ 下午：集成测试
    └─ Desktop 拖放 PNG 测试

第 3 天（阶段 2.2：文件上传）
├─ 上午：任务 2.2 — file.attach
│   ├─ 文件保存逻辑
│   ├─ @file: 引用解析
│   └─ prompt.submit 集成
│
└─ 下午：全面测试
    └─ Desktop 拖放 .py/.md 测试

第 4 天（阶段 3：Handoff）
├─ 上午：任务 3.1 — handoff 状态机
│   ├─ AppState 扩展
│   ├─ request / state / fail 实现
│   └─ 路由注册
│
└─ 下午：测试与完善
    └─ /handoff telegram 端到端测试

第 5 天（阶段 4+5：收尾）
├─ 上午：
│   ├─ 任务 4.1 — Profile soul / setup-command
│   └─ 任务 4.2 — 日志查询参数
│
└─ 下午：
    ├─ 任务 5.1 — WebSocket 事件补充
    └─ 全面回归测试 + 文档更新
```

---

## 四、风险评估与缓解

| 风险 | 可能性 | 影响 | 缓解措施 |
|------|--------|------|----------|
| `archived` 字段迁移失败 | 低 | 高 | 使用 `reconcile_table` 自动添加；已验证 `add_column_if_missing` 可安全处理 |
| 文件上传大文件内存溢出 | 中 | 中 | 限制单文件大小（10MB）；使用流式写入 |
| `session.steer` 修改 agent 内部逻辑引入 bug | 高 | 高 | 通过 steer_queue 解耦；不修改 agent_loop.rs； steer 文本作为用户消息追加 |
| handoff 平台集成复杂度 | 低 | 中 | 先实现状态机骨架；平台 API 调用留空（TODO）；返回 pending 让前端轮询 |
| @file: 引用解析与现有消息格式冲突 | 中 | 中 | 使用明确前缀 `@file:`；在 prompt.submit 中替换为文件内容；保留原始引用作为 fallback |

---

## 五、验证清单

### 5.1 单元测试

```bash
# 每完成一个任务后必做
cargo check -p hermes-server
cargo test -p hermes-server

# 全部完成后
cargo test -p hermes-server -- --nocapture
cargo clippy -p hermes-server -- -D warnings
```

### 5.2 Desktop 集成测试

| # | 测试场景 | 预期结果 | 负责阶段 |
|---|---------|---------|---------|
| 1 | `GET /api/sessions?archived=exclude` | 不返回已归档会话 | 1.1 |
| 2 | 归档一个会话后刷新列表 | 该会话消失 | 1.1 |
| 3 | Agent 生成中输入 "简短一点" | 后续输出变短 | 1.2 |
| 4 | 拖放 PNG 到输入框 | Agent 能描述图片内容 | 2.1 |
| 5 | 粘贴截图（base64） | 同场景 4 | 2.1 |
| 6 | 拖放 .py 文件 | Agent 能读取并分析代码 | 2.2 |
| 7 | `/handoff telegram` | 状态从 pending → completed/failed | 3.1 |
| 8 | Profile 设置 soul.md | 读写成功 | 4.1 |
| 9 | 日志页面过滤 ERROR | 只显示错误日志 | 4.2 |
| 10 | Agent 思考时观察 WebSocket | 收到 `reasoning.available` | 5.1 |

---

## 六、后续工作（P2/P3）

以下项目在本次 5 天计划后不处理，留作后续迭代：

| # | 项目 | 优先级 | 说明 |
|---|------|--------|------|
| 1 | `/api/analytics/usage` | 🟠 P2 | 使用统计需要聚合多表数据 |
| 2 | `/api/hermes/update` | 🔵 P3 | 后端自动更新机制复杂，需单独设计 |
| 3 | `/api/actions/{name}/status` | 🟠 P2 | Action 系统尚未完全实现 |
| 4 | `skin.changed` | 🔵 P3 | 主题系统尚未实现 |
| 5 | OAuth 认证端点 | ⚪ 延期 | 已在原始计划中明确延期 |
| 6 | handoff 实际平台集成 | 🟠 P2 | 当前仅实现状态机，实际 API 调用需集成 hermes-messaging |

---

## 七、文档变更记录

| 日期 | 版本 | 变更说明 |
|------|------|---------|
| 2026-06-11 | v1.0 | 初始版本，基于 Desktop 接口审查结果编制 |

---

**下一步行动**：确认计划后，从 **第 1 天 任务 1.1** 开始执行。
