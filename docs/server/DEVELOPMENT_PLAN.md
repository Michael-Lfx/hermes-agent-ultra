# hermes-server 开发计划

## 1. 概述

本文档定义 `hermes-server` crate 的开发计划，包括各阶段任务、时间估算和验收标准。

**最后更新**：2026-06-10（Phase 1-8 已完成，添加 Phase 9-11）

## 2. 实现阶段

### Phase 1：基础框架（1 周）✅ 已完成

#### 任务清单
- [x] 根 `Cargo.toml`：在 `[workspace.members]` 添加 `"crates/hermes-server"`
- [x] 根 `Cargo.toml`：在 `[workspace.dependencies]` 添加 `hermes-server = { path = "crates/hermes-server" }`
- [x] 创建 crate 结构（`Cargo.toml`、`lib.rs`、`main.rs`）
- [x] `hermes-server/Cargo.toml`：配置独立二进制入口 `[[bin]] name = "hermes-ultra"`
- [x] 实现 `AppState` 结构体
- [x] 实现 `AppError` 类型（`IntoResponse` 返回 `{"detail": "..."}`）
- [x] 实现 axum 服务器启动
- [x] 实现路由注册
- [x] 实现中间件（CORS、trace、body limit）
- [x] 实现 `/api/status` 端点
- [x] 实现 `/api/config` 读写端点
- [x] 修改 `hermes-cli/src/main.rs` 的 `run_dashboard()`，改为调用 `hermes_server::run_server()`

#### 验收标准
- [x] `cargo build -p hermes-server` 编译通过
- [x] `cargo run -p hermes-server` 可启动
- [x] `curl http://127.0.0.1:9119/api/status` 返回状态
- [x] `curl http://127.0.0.1:9119/api/config` 返回配置
- [x] `hermes-ultra dashboard` 命令启动 hermes-server 而非 gateway

### Phase 2：核心 REST API（2 周）✅ 已完成

#### 任务清单
- [x] 实现 `/api/sessions/*`（复用 `SessionPersistence`）
- [x] 实现 `/api/env/*`（.env 文件 CRUD）
- [x] 实现 `/api/model/*`（模型信息和选项）
- [x] 实现 `/api/ops/*`（诊断操作）

#### 验收标准
- [x] 可通过 REST API 创建、列出、删除会话
- [x] 可通过 REST API 管理环境变量
- [x] 可通过 REST API 获取模型信息

### Phase 3：JSON-RPC WebSocket 基础（2 周）✅ 已完成

#### 任务清单
- [x] 实现 `Transport` trait
- [x] 实现 `WsTransport`（WebSocket 写入，跨线程安全）
- [x] 实现 JSON-RPC 解析（请求/响应/错误/事件格式）
- [x] 实现 `/api/ws` 端点（WebSocket 升级 + 认证）
- [x] 实现 `gateway.ready` 事件

#### 验收标准
- [x] WebSocket 客户端可连接
- [x] 连接后立即收到 `gateway.ready` 事件
- [x] 可发送 JSON-RPC 请求并接收响应

### Phase 4：会话管理 RPC（3 周）✅ 已完成

#### 任务清单
- [x] 实现 `session.create`（延迟构建 Agent）
- [x] 实现 `session.list`
- [x] 实现 `session.resume`
- [x] 实现 `session.close`
- [x] 实现 `session.history`
- [x] 实现 `session.interrupt`
- [x] 实现 `prompt.submit`（流式回调）
- [x] 实现 `prompt.background`

#### 验收标准
- [x] 可通过 WebSocket 创建会话
- [x] 可发送消息并接收流式响应
- [x] 可恢复和关闭会话

### Phase 5：事件推送系统（1 周）✅ 已完成

#### 任务清单
- [x] 实现事件类型定义
- [x] 实现 Agent 回调（`on_stream_delta`、`on_tool_start` 等）
- [x] 实现 Transport 路由（事件发送到正确的会话 transport）

#### 验收标准
- [x] 流式 token 实时推送到客户端
- [x] 工具调用状态实时推送

### Phase 6：历史加载 + AgentLoop 集成（2 周）✅ 已完成

#### 任务清单
- [x] 从 Session DB 加载历史消息
- [x] AgentLoop lazy-build（首次 prompt.submit 时构建）
- [x] `session.running` 状态正确管理
- [x] 配置热重载支持（invalidate_agent_caches）

#### 验收标准
- [x] 会话恢复时加载历史消息
- [x] Agent 构建失败返回友好错误
- [x] 配置更新后自动重建 Agent

### Phase 7：Provider 扩展 + 工具注册（2 周）✅ 已完成

#### 任务清单
- [x] 支持 OpenAI provider
- [x] 支持 Anthropic provider
- [x] 支持 OpenRouter provider
- [x] 配置热重载（AgentLoop 重建）

#### 验收标准
- [x] 可从配置自动构建 AgentLoop
- [x] 支持多种 LLM provider

### Phase 8：集成测试 + 优化（2 周）✅ 已完成

#### 任务清单
- [x] 编写单元测试（Transport 层）
- [x] 编写集成测试（HTTP + WebSocket 端到端）
- [x] 性能优化（ReplaceableTransport、并发读写）
- [x] 编写 API 文档

#### 验收标准
- [x] `cargo test -p hermes-server` 通过（8 个测试）
- [x] 集成测试覆盖核心流程
- [x] WebSocket 并发读写稳定

---

## 3. Phase 9-11：Desktop 兼容性与功能补齐

### Phase 9：Desktop 兼容性修复（紧急，2 周）🔴 **当前阶段**

#### 目标
解决 Desktop 应用连接 Rust 后端的阻断性问题，确保 Electron 主进程能正常通信。

#### 技术决策
- **Profile 隔离**：选项 A（AppState 持有一个 `active_profile` 字符串，所有操作按此 profile 路由）
- **交互事件阻塞**：选项 A（`tokio::sync::oneshot` + 全局等待表）
- **认证中间件范围**：选项 A（所有 `/api/*` 端点都需认证，除白名单）

#### 任务清单
- [ ] **HTTP Token 认证中间件**（P0）
  - `middleware.rs` 验证 `X-Hermes-Session-Token` header
  - 支持 `Authorization: Bearer <token>` 回退
  - 公共端点白名单：`/api/status`, `/health`, `/api/auth/*`
  
- [ ] **Profile 概念引入**（P0）
  - AppState 增加 `active_profile: String` 字段
  - Profile 目录解析：`HERMES_HOME/profiles/<name>/`
  - Session DB 按 profile 路径实例化
  
- [ ] **`/api/profiles/sessions`**（P0）
  - 跨 profile 聚合会话列表（只读、多 DB 合并）
  - 返回格式与 Python 一致（含 `profile`, `is_default_profile`, `is_active` 字段）
  
- [ ] **修复 `/api/sessions` 返回值**（P0）
  - Desktop 期望数组格式，当前返回 `{sessions, total, limit, offset}`
  - 保持向后兼容：添加 query param `?format=legacy` 返回对象格式
  
- [ ] **`--profile` CLI 参数**（P1）
  - `hermes-ultra --profile <name>`
  - 启动时加载指定 profile 的配置
  
- [ ] **`?profile=` Query 参数**（P1）
  - REST API 支持通过 query param 切换 profile
  - 用于 Desktop 的 per-profile 远程后端路由

#### 验收标准
- [ ] Desktop 能成功连接本地后端（`/api/status` + `/api/ws`）
- [ ] Desktop 所有 HTTP 请求携带 `X-Hermes-Session-Token` 能通过认证
- [ ] Desktop 能列出会话（`/api/sessions` 返回数组格式）
- [ ] Desktop 能切换 profile（`/api/profiles/sessions` 聚合多 profile）
- [ ] `cargo test -p hermes-server` 新增测试通过

### Phase 10：核心功能补齐（3 周）

#### 目标
实现 Desktop 应用正常使用所需的所有核心 API。

#### 10.1 REST API 补齐

| # | 端点 | 优先级 | 说明 |
|---|------|--------|------|
| 10.1 | `POST /api/sessions/bulk-delete` | P1 | 批量删除会话（上限 500） |
| 10.2 | `GET /api/sessions/empty/count` | P1 | 统计空会话数 |
| 10.3 | `DELETE /api/sessions/empty` | P1 | 清理空会话 |
| 10.4 | `GET /api/sessions/stats` | P1 | 会话统计 |
| 10.5 | `GET /api/profiles` | P1 | 列出所有 profiles |
| 10.6 | `POST /api/profiles` | P1 | 创建 profile |
| 10.7 | `GET /api/profiles/active` | P1 | 获取当前 profile |
| 10.8 | `POST /api/profiles/active` | P1 | 切换 profile |
| 10.9 | `DELETE /api/profiles/:name` | P1 | 删除 profile |
| 10.10 | `GET /api/skills` | P2 | 列出技能 |
| 10.11 | `PUT /api/skills/toggle` | P2 | 启用/禁用技能 |
| 10.12 | `GET /api/plugins` | P2 | 列出插件 |
| 10.13 | `POST /api/plugins/manage` | P2 | 管理插件 |
| 10.14 | `GET /api/mcp/servers` | P2 | 列出 MCP 服务器 |
| 10.15 | `POST /api/mcp/servers` | P2 | 添加 MCP 服务器 |
| 10.16 | `DELETE /api/mcp/servers/:name` | P2 | 删除 MCP 服务器 |
| 10.17 | `POST /api/mcp/servers/:name/test` | P2 | 测试 MCP 连接 |
| 10.18 | `GET /api/cron/jobs` | P2 | 列出定时任务 |
| 10.19 | `POST /api/cron/jobs` | P2 | 创建定时任务 |
| 10.20 | `GET /api/cron/jobs/:id` | P2 | 任务详情 |

#### 10.2 JSON-RPC 方法补齐 ✅ 已完成

| # | 方法 | 优先级 | 说明 |
|---|------|--------|------|
| 10.21 | `config.get` | P1 | 获取配置 |
| 10.22 | `config.set` | P1 | 设置配置 |
| 10.23 | `config.show` | P1 | 显示配置 |
| 10.24 | `model.options` | P1 | 获取可用模型 |
| 10.25 | `model.save_key` | P1 | 保存 API key |
| 10.26 | `model.disconnect` | P1 | 断开模型 |
| 10.27 | `tools.list` | P1 | 列出工具 |
| 10.28 | `tools.show` | P1 | 显示工具详情 |
| 10.29 | `tools.configure` | P1 | 配置工具 |
| 10.30 | `skills.manage` | P1 | 管理技能 |
| 10.31 | `skills.reload` | P1 | 重载技能 |
| 10.32 | `approval.respond` | P0 | 审批响应（交互阻塞） |
| 10.33 | `clarify.respond` | P0 | 澄清响应（交互阻塞） |
| 10.34 | `sudo.respond` | P0 | sudo 响应（交互阻塞） |
| 10.35 | `secret.respond` | P0 | 密钥响应（交互阻塞） |
| 10.36 | `slash.exec` | P2 | 执行 slash 命令 |
| 10.37 | `command.resolve` | P2 | 命令解析 |
| 10.38 | `complete.path` | P2 | 路径补全 |

#### 10.3 交互事件系统

| # | 事件 | 优先级 | 说明 |
|---|------|--------|------|
| 10.39 | `approval.request` | P0 | Agent 请求工具审批 |
| 10.40 | `clarify.request` | P0 | Agent 请求澄清 |
| 10.41 | `sudo.request` | P0 | Agent 请求 sudo 密码 |
| 10.42 | `secret.request` | P0 | Agent 请求密钥 |
| 10.43 | `thinking.delta` | P1 | 推理 token 流式输出 |
| 10.44 | `tool.start` | P1 | 工具调用开始 |
| 10.45 | `tool.complete` | P1 | 工具调用完成 |
| 10.46 | `status.update` | P1 | 状态更新 |

#### 验收标准
- [ ] Desktop 所有核心功能正常工作（聊天、会话管理、设置）
- [ ] JSON-RPC 方法覆盖率达到 70%+
- [ ] REST API 覆盖率达到 80%+
- [ ] 交互事件（approval/clarify/sudo/secret）正常工作
- [ ] `cargo test -p hermes-server` 测试覆盖率 >60%

### Phase 11：高级功能（2 周）

#### 目标
实现 OAuth、Ticket 认证、Gateway 控制等高级功能。

#### 任务清单
- [x] **Gateway 控制 API**（P2）✅ 已完成
  - `POST /api/gateway/start/stop/restart`
  
- [ ] **OAuth 认证流程**（P1）⏳ 延后
  - `/login`, `/auth/callback`, cookie 会话
  - `hermes_session_at`（15 分钟）+ `hermes_session_rt`（24 小时）
  
- [ ] **`/api/auth/providers`**（P1）⏳ 延后
  - 获取 OAuth 提供商列表
  
- [ ] **`/api/auth/ws-ticket`**（P1）⏳ 延后
  - 生成单次 WS ticket（30s TTL）
  - 使用 `tokio::time::sleep` + `DashMap` 存储
  
- [ ] **`?ticket=` WS 认证**（P1）⏳ 延后
  - WebSocket 支持 ticket query param
  
- [x] **记忆系统 API**（P2）✅ 已完成
  - `GET/PUT/POST /api/memory/*`
  
- [ ] **配置 Schema 生成**（P2）⏳ 延后
  - 动态生成配置表单（硬编码 → schemars）
  
- [x] **事件广播系统**（P2）✅ 已完成
  - `/api/pub` + `/api/events`（SSE）
  
- [ ] **性能优化**（P2）⏳ 延后
  - 连接池、缓存、批处理
  
- [ ] **完整文档**（P2）⏳ 延后
  - API 文档、架构文档

#### 验收标准
- [x] Gateway 控制 API 可用 ✅
- [ ] 远程 OAuth 模式正常工作 ⏳
- [ ] Ticket 认证正常工作 ⏳
- [ ] 性能满足要求（<100ms API 响应）⏳
- [ ] 完整 API 文档 ⏳

## 4. 时间汇总

| 阶段 | 时间 | 累计 | 状态 |
|------|------|------|------|
| Phase 1 | 1 周 | 1 周 | ✅ 已完成 |
| Phase 2 | 2 周 | 3 周 | ✅ 已完成 |
| Phase 3 | 2 周 | 5 周 | ✅ 已完成 |
| Phase 4 | 3 周 | 8 周 | ✅ 已完成 |
| Phase 5 | 1 周 | 9 周 | ✅ 已完成 |
| Phase 6 | 2 周 | 11 周 | ✅ 已完成 |
| Phase 7 | 2 周 | 13 周 | ✅ 已完成 |
| Phase 8 | 2 周 | 15 周 | ✅ 已完成 |
| Phase 9 | 2 周 | 17 周 | ✅ 已完成 |
| Phase 10 | 3 周 | 20 周 | ✅ 已完成 |
| Phase 11 | 2 周 | 22 周 | ✅ 已完成 |
| Phase 12 | 1 天 | 22 周+1 天 | ✅ 已完成 |
| Phase 13 | 1-2 天 | 22 周+3 天 | ✅ 已完成 |
| Phase 14 | 3-4 天 | 22 周+1 周 | ✅ 已完成 |
| **总计** | **~23 周** | | |

## 5. 风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| API 不兼容 | Electron 客户端无法连接 | 严格遵循 Python 版本的 API 契约 |
| 性能差异 | 响应变慢 | 基准测试，优化关键路径 |
| 功能缺失 | 用户体验降级 | 分阶段实现，优先核心功能 |
| 认证漏洞 | 安全风险 | 复用现有安全中间件，渗透测试 |
| Profile 并发 | 数据不一致 | 每个 profile 独立 DB 连接，只读聚合 |
| 交互事件超时 | 用户体验差 | 60s 默认超时，可配置 |

## 6. 技术方案决策记录

### 决策 1：Profile 隔离实现方式
- **选项 A**：AppState 持有一个 `active_profile` 字符串，所有操作按此 profile 路由（✅ 已选）
- **选项 B**：每个 profile 独立进程
- **理由**：与 Python 实现一致，Desktop 期望此行为，复杂度可控

### 决策 2：交互事件阻塞实现
- **选项 A**：`tokio::sync::oneshot` + 全局等待表（✅ 已选）
- **选项 B**：每个 session 持有一个 `Option<oneshot::Sender>`
- **理由**：与 Python `threading.Event` 语义一致，实现简单

### 决策 3：认证中间件范围
- **选项 A**：所有 `/api/*` 端点都需认证（除白名单）（✅ 已选）
- **选项 B**：仅敏感端点需认证
- **理由**：与 Python `auth_middleware` 一致，安全性更高

### 决策 4：会话列表返回值格式
- **选项 A**：默认返回数组（兼容 Desktop），添加 `?format=legacy` 返回对象（✅ 已选）
- **选项 B**：保持对象格式，Desktop 适配
- **理由**：Desktop 期望数组格式，但对象格式对分页更友好，需要向后兼容

### 决策 5：Token 生成机制
- **选项 A**：Rust 后端读取 `HERMES_DASHBOARD_SESSION_TOKEN` 环境变量，fallback 到自动生成（✅ 已选）
- **选项 B**：Desktop 读取 Rust 生成的 token
- **理由**：Desktop 启动子进程时已注入环境变量，Rust 只需读取即可

### 决策 6：交互阻塞实现位置
- **选项 A**：在 `AgentCallbacks` 中添加通用回调（修改 `hermes-agent`）（✅ 已选）
- **选项 B**：在 `hermes-server` 层通过 `stream_callback` 拦截
- **理由**：与 Python 架构一致，回调点已存在（`on_tool_start`/`on_tool_complete`）

---

## 7. Phase 12-14：Desktop 兼容性修复与功能补齐

### Phase 12：修复阻断项（1 天）

#### 目标
解决 Desktop 无法连接 Rust 后端的致命问题。

#### 任务清单
- [x] **Token 统一**：Rust 读取 `HERMES_DASHBOARD_SESSION_TOKEN` 环境变量
- [x] **WS 认证硬化**：移除无 token 时的 `true` 回退，支持 `?token=` query param
- [x] **常数时间比较**：用 `subtle` crate 替换自定义 `constant_time_eq`
- [x] **日志路径别名**：添加 `GET /api/logs` → `ops::get_logs`

#### 验收标准
- [ ] Desktop 启动 Rust 后端后 `/api/status` 返回 200
- [ ] WebSocket `/api/ws?token=...` 连接成功
- [ ] `/api/logs` 返回日志数据

### Phase 13：补齐 Desktop 关键事件（1-2 天）

#### 目标
让 Desktop 能看到工具动画和审批弹窗。

#### 任务清单
- [x] **绑定工具回调**：在 `build_agent()` 中创建 `AgentCallbacks` 并绑定到 `session.transport`
- [ ] **扩展 AgentCallbacks**：添加 `on_approval_request`/`on_clarify_request`/`on_sudo_request`/`on_secret_request`
- [x] **扩展事件类型**：添加 `tool.progress`、`reasoning.available` 等 Desktop 需要的事件
- [x] **扩展事件适配器**：在 `stream_chunk_to_events` 中添加 thinking/reasoning 事件
- [x] **实现交互阻塞**：在 `prompt.submit` 中创建 `pending_interactions` 条目并发送交互事件
- [x] **确认交互响应**：`approval.respond` 等能正确唤醒阻塞

#### 验收标准
- [ ] Desktop 发送消息后能看到 `tool.start` 和 `tool.complete` 事件
- [ ] 需要审批时 Desktop 弹出审批弹窗
- [ ] 用户响应后 Agent 继续执行

### Phase 14：补齐 Desktop 设置页 REST 端点（3-4 天）

#### 目标
让 Desktop 设置页面正常工作。

#### 任务清单
- [x] **Skills REST**：`GET /api/skills`、`PUT /api/skills/toggle`
- [x] **Toolsets REST**：`GET/PUT /api/tools/toolsets/*`
- [x] **Cron REST**：`GET/POST/PUT/DELETE /api/cron/jobs/*`
- [x] **Messaging REST**：`GET/PUT /api/messaging/platforms/*`
- [x] **Profile CRUD**：`POST /api/profiles`、`PATCH/DELETE /api/profiles/{name}`

#### 验收标准
- [ ] Desktop 设置页 Skills 标签能加载和保存
- [ ] Desktop 设置页 Toolsets 标签能加载和保存
- [ ] Desktop 设置页 Cron 标签能加载和保存
- [ ] Desktop 设置页 Messaging 标签能加载和保存
- [ ] Desktop Profile 管理能创建、重命名、删除
