# hermes-server 全面审查报告

> **审查日期**：2026-06-11
> **审查范围**：文档对齐、Python 版本接口一致性、Desktop 需求满足度
> **审查依据**：
> - 设计文档：`docs/server/TECHNICAL_DESIGN.md`、`DEVELOPMENT_PLAN.md`、`DESKTOP_COMPATIBILITY_PLAN.md`
> - Python 参考：`C:\workspace\hermes-agent\tui_gateway\server.py`、`ws.py`、`transport.py`
> - Python REST：`C:\workspace\hermes-agent\hermes_cli\web_server.py`
> - Desktop Electron：`C:\workspace\hermes-agent\apps\desktop\electron\main.cjs`
> - Rust 实现：`crates/hermes-server/src/` 全部源文件

---

## 一、文档对齐审查

### 1.1 架构实现 vs TECHNICAL_DESIGN.md

| 设计文档要求 | 实现状态 | 偏差说明 |
|---|---|---|
| **独立二进制入口** (`hermes-ultra`) | ✅ 已实现 | `Cargo.toml` 中有 `[[bin]] name = "hermes-ultra"` |
| **目录结构** | ⚠️ 部分偏差 | 缺少 `src/rpc/slash.rs`、`src/rpc/delegation.rs`、`src/rpc/misc.rs`、`src/core/ticket.rs`、`src/rest/plugins.rs`、`src/rest/mcp.rs` |
| **Transport trait** | ✅ 已实现 | `ReplaceableTransport` + `NullTransport` + `WsTransport` |
| **AppState 结构** | ✅ 已实现 | 所有字段已按设计实现 |
| **JSON-RPC 协议类型** | ✅ 已实现 | `JsonRpcRequest`、`JsonRpcResponse`、`JsonRpcError`、`JsonRpcEvent` |
| **Profile 隔离** | ✅ 已实现 | `active_profile`、`profile_home()`、`profile_persistence()` |
| **Token 认证** | ✅ 已实现 | `HERMES_DASHBOARD_SESSION_TOKEN` 环境变量读取 |
| **常数时间比较** | ✅ 已实现 | `subtle::ConstantTimeEq` |
| **交互事件系统** | ✅ 已实现 | `PendingInteractions` + `oneshot` channel |
| **事件广播** | ✅ 已实现 | `broadcast::Sender` |
| **Cron 数据目录** | ✅ 已实现 | `~/.hermes/cron/` |

**结论**：架构实现与文档基本对齐，但部分文件/模块按设计文档应存在但实际未创建。

### 1.2 开发计划 vs DEVELOPMENT_PLAN.md

| Phase | 状态 | 关键偏差 |
|---|---|---|
| Phase 1 (基础框架) | ✅ 完成 | 无偏差 |
| Phase 2 (核心 REST API) | ✅ 完成 | 无偏差 |
| Phase 3 (WS 基础) | ✅ 完成 | 无偏差 |
| Phase 4 (会话管理 RPC) | ✅ 完成 | 无偏差 |
| Phase 5 (事件推送) | ✅ 完成 | 无偏差 |
| Phase 6 (历史加载+AgentLoop) | ✅ 完成 | `agent_builder.rs` 已集成 |
| Phase 7 (Provider 扩展) | ✅ 完成 | OpenAI/Anthropic/OpenRouter |
| Phase 8 (集成测试) | ✅ 完成 | 10 个测试通过 |
| Phase 9 (Desktop 兼容性) | ✅ 完成 | Token、Profile、认证中间件 |
| Phase 10 (核心功能补齐) | ⚠️ 部分完成 | REST API 覆盖率约 60%，JSON-RPC 约 30% |
| Phase 11 (高级功能) | ⚠️ 部分完成 | Gateway 控制、记忆系统、事件广播已做；OAuth/Ticket 延后 |
| Phase 12 (阻断项修复) | ✅ 完成 | Token 统一、WS 认证硬化、常数时间比较、日志别名 |
| Phase 13 (关键事件) | ⚠️ 部分完成 | `tool.start/complete/thinking.delta` 已绑定；`approval/clarify/sudo/secret.request` 已实现但仅通过 `interaction_dispatch.rs`，未绑定到 AgentCallbacks |
| Phase 14 (设置页 REST) | ✅ 完成 | Skills、Toolsets、Cron、Messaging、Profile CRUD |

**关键偏差**：Phase 10-11 中大量 REST 端点和 JSON-RPC 方法仍处于占位符或未实现状态。

### 1.3 Desktop 兼容性计划 vs DESKTOP_COMPATIBILITY_PLAN.md

| 阶段 | 任务 | 状态 | 偏差 |
|---|---|---|---|
| **阶段 1** | Token 统一 | ✅ 完成 | `state.rs` 读取环境变量 |
| **阶段 1** | WS 认证硬化 | ✅ 完成 | `subtle::ConstantTimeEq`，无 token 拒绝 |
| **阶段 1** | HTTP 中间件硬化 | ✅ 完成 | 常数时间比较，白名单 |
| **阶段 1** | 日志路径别名 | ✅ 完成 | `/api/logs` 路由已添加 |
| **阶段 2** | 绑定工具回调 | ✅ 完成 | `agent_builder.rs` 中 `on_tool_start/complete` |
| **阶段 2** | 扩展交互事件 | ⚠️ 部分 | `interaction_dispatch.rs` 实现了 approval/clarify/sudo/secret，但未在 `AgentCallbacks` 中添加对应回调 |
| **阶段 3** | Skills REST | ✅ 完成 | 文件系统扫描 |
| **阶段 3** | Toolsets REST | ⚠️ 部分 | 仅返回默认 "all" toolset，无 ToolsetManager 集成 |
| **阶段 3** | Cron REST | ✅ 完成 | `FileJobPersistence` 已集成 |
| **阶段 3** | Messaging REST | ✅ 完成 | Mock 实现 |
| **阶段 3** | Profile CRUD | ✅ 完成 | 创建/重命名/删除 |

---

## 二、Python 版本接口一致性审查

### 2.1 JSON-RPC 方法对比

**Python 版本有 82 个 @method 方法，Rust 当前实现约 27 个。**

#### 已实现的 RPC 方法（27 个）

```
session.create, session.list, session.resume, session.close
session.history, session.interrupt, session.title, session.usage, session.delete
prompt.submit, prompt.background
config.get, config.set, config.show
model.options, model.save_key, model.disconnect
tools.list, tools.show, tools.configure
skills.manage, skills.reload
approval.respond, clarify.respond, sudo.respond, secret.respond
```

#### ❌ 缺失的 RPC 方法（55 个）—— 高优先级

**Session 管理（10 个）：**
```
session.most_recent, session.cwd.set, session.active_list, session.activate
session.undo, session.compress, session.save, session.status, session.branch, session.steer
```

**Prompt/消息（2 个）：**
```
preview.restart, clipboard.paste
```

**附件与输入（6 个）：**
```
image.attach, image.attach_bytes, pdf.attach, file.attach
image.detach, input.detect_drop
```

**终端与响应（1 个）：**
```
terminal.resize, terminal.read.respond
```

**Config/Setup（1 个）：**
```
setup.status, setup.runtime_check  (部分实现)
```

**Tools/System（3 个）：**
```
process.stop, reload.mcp, reload.env
```

**Commands/CLI（3 个）：**
```
commands.catalog, cli.exec, command.resolve, command.dispatch
```

**补全与粘贴（2 个）：**
```
paste.collapse, complete.path, complete.slash
```

**Voice（3 个）：**
```
voice.toggle, voice.record, voice.tts
```

**Insights/Rollback（4 个）：**
```
insights.get, rollback.list, rollback.restore, rollback.diff
```

**Browser/Plugins/Tools（5 个）：**
```
browser.manage, plugins.list, plugins.manage
config.show  (重复?), toolsets.list, agents.list
```

**Cron/Skills/Shell（3 个）：**
```
cron.manage, shell.exec
```

**Handoff（3 个）：**
```
handoff.request, handoff.state, handoff.fail
```

**Delegation/Subagent（3 个）：**
```
delegation.status, delegation.pause, subagent.interrupt
```

**Spawn Tree（3 个）：**
```
spawn_tree.save, spawn_tree.list, spawn_tree.load
```

**其他（5 个）：**
```
session.delete  (已作为 RPC 实现)
```

**覆盖率：27/82 = 32.9%**

### 2.2 REST 端点对比

**Python 版本有 100+ REST 端点，Rust 当前实现约 40 个。**

#### 已实现的 REST 端点（约 40 个）

```
GET  /api/status
GET  /api/system/stats
GET  /api/config, PUT /api/config
GET  /api/config/defaults, GET /api/config/schema
GET  /api/config/raw, PUT /api/config/raw
GET  /api/sessions, GET /api/sessions/search
GET  /api/sessions/{id}, DELETE /api/sessions/{id}, PATCH /api/sessions/{id}
GET  /api/sessions/{id}/messages, GET /api/sessions/{id}/export
POST /api/sessions/prune
GET  /api/env, PUT /api/env, DELETE /api/env
POST /api/env/reveal
GET  /api/model/info, GET /api/model/options, POST /api/model/set
GET  /api/model/recommended-default, GET /api/model/auxiliary
POST /api/ops/doctor, POST /api/ops/backup, POST /api/ops/import
POST /api/ops/dump, GET /api/ops/logs
GET  /api/logs (alias)
POST /api/pub, GET /api/events
GET  /api/ws (WebSocket)
GET  /api/memory, PUT /api/memory/provider, POST /api/memory/reset
GET  /api/profiles/sessions, GET /api/profiles, POST /api/profiles
PATCH /api/profiles/{name}, DELETE /api/profiles/{name}
GET  /api/profiles/active, POST /api/profiles/active
GET  /api/skills, PUT /api/skills/toggle
GET  /api/tools/toolsets, PUT /api/tools/toolsets/{name}
GET  /api/tools/toolsets/{name}/config, PUT /api/tools/toolsets/{name}/provider
POST /api/tools/toolsets/{name}/post-setup
GET  /api/cron/jobs, POST /api/cron/jobs, GET /api/cron/jobs/{id}
PUT  /api/cron/jobs/{id}, DELETE /api/cron/jobs/{id}
GET  /api/cron/jobs/{id}/runs
POST /api/cron/jobs/{id}/pause, POST /api/cron/jobs/{id}/resume
POST /api/cron/jobs/{id}/trigger
GET  /api/messaging/platforms, PUT /api/messaging/platforms/{id}
POST /api/messaging/platforms/{id}/test
POST /api/gateway/start, POST /api/gateway/stop, POST /api/gateway/restart
GET  /health
```

#### ❌ 缺失的 REST 端点（约 60+ 个）

**会话管理（10 个）：**
```
GET  /api/sessions/stats
GET  /api/sessions/empty/count
DELETE /api/sessions/empty
POST /api/sessions/bulk-delete
GET  /api/sessions/{id}/latest-descendant
GET  /api/sessions/{id}/branch  (branch 相关)
POST /api/sessions/{id}/compress
POST /api/sessions/{id}/undo
POST /api/sessions/{id}/steer
```

**文件与媒体（8 个）：**
```
GET  /api/media
GET  /api/files, GET /api/files/read
POST /api/files/upload, POST /api/files/mkdir
DELETE /api/files
GET  /api/files/download  (可能)
```

**OAuth 提供商（7 个）：**
```
GET  /api/auth/providers
GET  /api/providers/oauth
DELETE /api/providers/oauth/{provider_id}
POST /api/providers/oauth/{provider_id}/start
POST /api/providers/oauth/{provider_id}/submit
GET  /api/providers/oauth/{provider_id}/poll/{session_id}
DELETE /api/providers/oauth/sessions/{session_id}
```

**音频（3 个）：**
```
POST /api/audio/transcribe
GET  /api/audio/elevenlabs/voices
POST /api/audio/speak
```

**操作与诊断（5 个）：**
```
POST /api/ops/prompt-size
POST /api/ops/config-migrate
POST /api/ops/debug-share
POST /api/hermes/update
GET  /api/hermes/update/check
GET  /api/actions/{name}/status
```

**Curator（3 个）：**
```
GET  /api/curator, PUT /api/curator/paused, POST /api/curator/run
```

**Cron 补充（2 个）：**
```
GET  /api/cron/delivery-targets
GET  /api/cron/jobs/{id}/runs  (已有但返回空数组)
```

**MCP（7 个）：**
```
GET  /api/mcp/servers, POST /api/mcp/servers
DELETE /api/mcp/servers/{name}, POST /api/mcp/servers/{name}/test
PUT  /api/mcp/servers/{name}/enabled
GET  /api/mcp/catalog, POST /api/mcp/catalog/install
```

**消息平台（5 个）：**
```
POST /api/messaging/platforms/{id}/test  (已有)
POST /api/messaging/telegram/onboarding/...
```

**其他（约 20 个）：**
```
GET  /api/portal
POST /api/providers/validate
POST /api/auth/ws-ticket
POST /login, GET /auth/callback
```

**覆盖率：约 40/100+ = <40%**

### 2.3 事件类型对比

**Python 版本发射约 25+ 种事件，Rust 当前定义约 12 种，实际触发约 6 种。**

#### 已定义且已触发的事件

```
gateway.ready           ✅ (ws/handler.rs)
message.start           ✅ (stream_chunk_to_events)
message.delta           ✅ (stream_chunk_to_events)
message.complete        ✅ (stream_chunk_to_events)
thinking.delta          ✅ (AgentCallbacks::on_thinking)
tool.start              ✅ (AgentCallbacks::on_tool_start)
tool.complete           ✅ (AgentCallbacks::on_tool_complete)
error                   ✅ (prompt.rs 错误处理)
```

#### 已定义但未触发的事件

```
approval.request        ⚠️ (定义在 events.rs 但仅在 interaction_dispatch.rs 中发送，未绑定到 AgentCallbacks)
clarify.request         ⚠️ (同上)
sudo.request            ⚠️ (同上)
secret.request          ⚠️ (同上)
```

#### ❌ 缺失的事件类型

```
session.info            ❌ (Python 最频繁事件，Rust 未实现)
reasoning.available     ❌
reasoning.delta         ❌
tool.generating         ❌
tool.progress           ❌
status.update           ❌
notification.show       ❌
notification.clear      ❌
terminal.read.request   ❌
background.complete     ❌
preview.restart.progress ❌
preview.restart.complete ❌
review.summary          ❌
skin.changed            ❌
voice.transcript        ❌
voice.status            ❌
browser.progress        ❌
```

**事件覆盖率：约 8/25 = 32%**

### 2.4 认证机制对比

| 机制 | Python | Rust | 状态 |
|---|---|---|---|
| **Token 模式** | `X-Hermes-Session-Token` Header | ✅ 相同 | 一致 |
| **Bearer 回退** | `Authorization: Bearer <token>` | ✅ 相同 | 一致 |
| **WS Token** | `?token=<token>` | ✅ 相同 | 一致 |
| **WS Ticket** | `?ticket=<ticket>` (30s TTL) | ✅ 解析 | 未实现 Ticket 生成端点 |
| **OAuth Cookie** | `hermes_session_at` + `hermes_session_rt` | ❌ 未实现 | 延后 |
| **Host Header 验证** | DNS Rebinding 防护 | ❌ 未实现 | 安全漏洞 |
| **CORS 限制** | 严格 localhost 正则 | ⚠️ permissive | 安全漏洞 |
| **Rate Limiting** | `/api/env/reveal` 端点限制 30 秒内最多 5 次请求 | ❌ 未实现 | 功能缺失 |
| **常数时间比较** | `hmac.compare_digest` | ✅ `subtle::ConstantTimeEq` | 一致 |

**关键偏差**：
1. CORS 设置为 `permissive()` 而非严格的 localhost 正则，存在安全风险
2. 缺少 Host Header 验证，存在 DNS Rebinding 攻击面
3. `/api/env/reveal` 无速率限制
4. OAuth 认证完全未实现

### 2.5 Session 管理对比

| 功能 | Python | Rust | 状态 |
|---|---|---|---|
| **创建** | 轻量 session + 50ms 延迟构建 Agent | ✅ 类似 | 一致 |
| **恢复** | 从 DB 恢复历史，跨 profile | ⚠️ 部分 | 恢复历史但未加载消息到 Agent |
| **关闭** | `_close_session_by_id` → `_teardown_session` → `_finalize_session` | ⚠️ 简化 | 仅移除内存状态 + DB end_session |
| **WS 断开处理** | `close_on_disconnect` + 20s 孤儿回收 | ❌ 未实现 | 严重缺失 |
| **并发控制** | `_sessions_lock` (RLock) + `history_lock` | ✅ `RwLock` | 一致 |
| **inflight_turn** | 流式消息快照 | ❌ 未实现 | Desktop 可能无法正确显示流式状态 |
| **model_override** | 会话级模型覆盖 | ❌ 未实现 | 功能缺失 |
| **profile_home** | 指向其他 profile 的 home | ⚠️ 部分 | 有字段但未在 resume 中使用 |
| **pending_title** | 待生成标题 | ❌ 未实现 | 功能缺失 |
| **attached_images** | 附加图片 | ❌ 未实现 | 功能缺失 |
| **slash_worker** | `_SlashWorker` 子进程 | ❌ 未实现 | 功能缺失 |
| **TTL 清理** | 6 小时 TTL，5 分钟扫描 | ❌ 未实现 | 内存泄漏风险 |

---

## 三、Desktop 需求满足度审查

### 3.1 后端进程启动

| 需求 | Desktop 期望 | Rust 实现 | 状态 |
|---|---|---|---|
| **命令格式** | `python -m hermes_cli.main dashboard --no-open --host 127.0.0.1 --port <port>` | `hermes-ultra dashboard` (独立二进制) | ✅ 可满足 |
| **Token 注入** | `HERMES_DASHBOARD_SESSION_TOKEN` | ✅ 读取环境变量 | 一致 |
| **HERMES_HOME** | 强制一致 | ✅ 使用相同路径 | 一致 |
| **端口范围** | 9120-9199 动态扫描 | ⚠️ 需验证 | 应由启动器决定 |
| **Profile 参数** | `--profile <name>` | ❌ CLI 未实现 | 缺失 |
| **工作目录** | `TERMINAL_CWD` + 用户设置 | ⚠️ 部分 | `cwd` 字段存在但未读取环境变量 |

**关键偏差**：Rust 二进制缺少 `--profile` CLI 参数，Desktop 切换 profile 时无法传递。

### 3.2 WebSocket 连接

| 需求 | Desktop 期望 | Rust 实现 | 状态 |
|---|---|---|---|
| **URL 格式** | `ws://127.0.0.1:<port>/api/ws?token=<token>` | ✅ 相同 | 一致 |
| **认证** | `?token=` query param | ✅ 已支持 | 一致 |
| **Ticket 模式** | `?ticket=` (OAuth) | ✅ 解析 | Ticket 生成未实现 |
| **连接探测** | 10s 超时，750ms 就绪宽限期 | ✅ 由 ws-handler 支持 | 一致 |
| **gateway.ready** | 连接后立即收到 | ✅ 已实现 | 一致 |
| **自动重连** | Desktop 有重连逻辑 | ✅ 无状态服务器，支持重连 | 一致 |

### 3.3 REST API 调用

| 需求 | Desktop 实际调用 | Rust 实现 | 状态 |
|---|---|---|---|
| **Token 传递** | `X-Hermes-Session-Token` Header | ✅ 已实现 | 一致 |
| **GET /api/status** | 后端就绪探测 | ✅ 已实现 | 一致 |
| **GET /api/sessions** | 会话列表 | ✅ 已实现 | 一致 |
| **GET /api/profiles/sessions** | 跨 profile 聚合 | ✅ 已实现 | 一致 |
| **GET /api/sessions/{id}/messages** | 会话消息 | ✅ 已实现 | 一致 |
| **DELETE /api/sessions/{id}** | 删除会话 | ✅ 已实现 | 一致 |
| **GET /api/logs** | 读取日志 | ✅ 别名已实现 | 一致 |
| **GET /api/model/options** | 模型列表 | ✅ 已实现 | 一致 |
| **GET /api/config** | 配置读取 | ✅ 已实现 | 一致 |
| **POST /api/auth/ws-ticket** | OAuth ticket | ❌ 未实现 | 阻塞远程模式 |
| **GET /api/auth/providers** | OAuth 提供商 | ❌ 未实现 | 阻塞远程模式 |

### 3.4 事件订阅

| 事件 | Desktop 订阅 | Rust 触发 | 状态 |
|---|---|---|---|
| `gateway.ready` | ✅ | ✅ | 一致 |
| `message.start` | ✅ | ✅ | 一致 |
| `message.delta` | ✅ | ✅ | 一致 |
| `message.complete` | ✅ | ✅ | 一致 |
| `thinking.delta` | ✅ | ✅ (通过 AgentCallbacks) | 一致 |
| `tool.start` | ✅ | ✅ (通过 AgentCallbacks) | 一致 |
| `tool.complete` | ✅ | ✅ (通过 AgentCallbacks) | 一致 |
| `tool.progress` | ⚠️ | ❌ | 缺失 |
| `approval.request` | ✅ | ⚠️ (仅 interaction_dispatch) | 部分 |
| `clarify.request` | ✅ | ⚠️ (仅 interaction_dispatch) | 部分 |
| `session.info` | ✅ (最频繁) | ❌ | **严重缺失** |
| `status.update` | ✅ | ❌ | 缺失 |
| `error` | ✅ | ✅ | 一致 |
| `voice.transcript` | ⚠️ | ❌ | 缺失 |
| `browser.progress` | ⚠️ | ❌ | 缺失 |

**最严重缺失**：`session.info` 是 Python 中最频繁发射的事件（每次状态变化都发），Desktop 依赖此事件更新 UI。Rust 完全未实现。

### 3.5 Profile 与多后端

| 需求 | Desktop 期望 | Rust 实现 | 状态 |
|---|---|---|---|
| **Primary backend** | 主 profile 后端 | ✅ 支持 | 一致 |
| **Backend pool** | LRU 池，最多 3 个 | ❌ 未实现 | 功能缺失 |
| **Per-profile 端口** | 每个 profile 独立端口+token | ❌ 未实现 | 功能缺失 |
| **Profile 切换** | `POST /api/profiles/active` + invalidate caches | ✅ 已实现 | 一致 |
| **跨 profile 会话聚合** | `GET /api/profiles/sessions` | ✅ 已实现 | 一致 |

### 3.6 交互流程

| 交互 | Desktop 期望 | Rust 实现 | 状态 |
|---|---|---|---|
| **Approval 弹窗** | `approval.request` → 用户响应 → `approval.respond` | ⚠️ 部分 | 事件发送，但未验证端到端 |
| **Clarify 弹窗** | `clarify.request` → 用户响应 → `clarify.respond` | ⚠️ 部分 | 同上 |
| **Sudo 弹窗** | `sudo.request` → 用户响应 → `sudo.respond` | ⚠️ 部分 | 同上 |
| **Secret 弹窗** | `secret.request` → 用户响应 → `secret.respond` | ⚠️ 部分 | 同上 |
| **超时** | 60s 默认超时 | ✅ 已实现 | 一致 |
| **自动拒绝** | 超时后自动 deny | ❌ 未实现 | 返回错误而非自动拒绝 |

---

## 四、关键差距总结

### 4.1 P0 - 阻断性问题（Desktop 无法正常工作）

1. **❌ `session.info` 事件缺失** — Desktop 最依赖的事件，用于更新会话状态、标题、消息数等 UI
2. **❌ WS 断开处理缺失** — 无 `close_on_disconnect` 逻辑，无 20s 孤儿回收，会话泄漏
3. **❌ Session TTL 清理缺失** — 无 6 小时 TTL 扫描，内存泄漏
4. **❌ CORS 过于宽松** — `permissive()` 允许任意来源，存在安全风险
5. **❌ Host Header 验证缺失** — 无 DNS Rebinding 防护
6. **❌ `inflight_turn` 状态缺失** — Desktop 无法正确显示"正在输入"状态
7. **❌ `--profile` CLI 参数缺失** — Desktop 切换 profile 时无法传递

### 4.2 P1 - 严重功能缺失

8. **RPC 方法覆盖率仅 33%** — 55/82 方法未实现
9. **REST 端点覆盖率 <40%** — 60+ 端点未实现
10. **事件类型覆盖率 32%** — 17/25 事件未实现
11. **OAuth 认证完全缺失** — 远程模式不可用
12. **Ticket 认证未实现** — `/api/auth/ws-ticket` 端点缺失
13. **MCP 端点缺失** — Desktop 设置页 MCP 标签无法工作
14. **文件上传/管理端点缺失** — 附件功能无法使用
15. **音频端点缺失** — 语音功能无法使用
16. **技能 toggle 未持久化** — 仅返回 mock 响应

### 4.3 P2 - 功能降级

17. **Toolset 管理不完整** — 仅返回默认 "all" toolset
18. **Cron trigger 未实现** — 仅返回 mock 响应
19. **消息平台为 mock** — Telegram/Slack/Discord 配置不生效
20. **Gateway restart 为占位符** — 仅返回成功响应
21. **Profile 后端池未实现** — 多 profile 并发受限
22. **缺少 `model_override`** — 会话级模型切换不可用
23. **缺少 `attached_images`** — 图片附件会话状态未维护

---

## 五、建议修复优先级

### 阶段 1：Desktop 连接阻断修复（1-2 天）

1. **实现 `session.info` 事件发射**
   - 在 `session.create/resume/close/title` 等方法中发射
   - 在消息计数变化时发射（每次 DB 写入后）
   - 格式与 Python 一致：含 `session_id`, `message_count`, `title`, `model`, `cwd` 等

2. **修复 CORS 配置**
   - 从 `permissive()` 改为严格的 localhost/127.0.0.1 正则匹配
   - 或基于 `HERMES_DESKTOP` 环境变量自动切换（开发宽松/生产严格）

3. **添加 Host Header 验证中间件**
   - 中间件中检查 `Host` header，仅允许 `localhost`、`127.0.0.1`、`::1`
   - 防御 DNS Rebinding（CVE-2023-XXXX 类似漏洞）

4. **实现 WS 断开清理**
   - `close_on_disconnect` 逻辑
   - 20s 孤儿会话回收定时器

5. **添加 `--profile` CLI 参数**
   - `hermes-ultra dashboard --profile <name>`
   - Desktop 切换 profile 时需要传递此参数

### 阶段 2：核心事件补齐（2-3 天）

6. **补齐关键事件类型**
   - `status.update` (含 kind: process/goal/ready/compressing)
   - `tool.progress`
   - `tool.generating`
   - `reasoning.available` / `reasoning.delta`

7. **实现 `inflight_turn` 跟踪**
   - 在 `prompt.submit` 开始时设置
   - 在流式结束时清除
   - 随 `session.info` 一起发射

### 阶段 3：关键 RPC 方法补齐（3-5 天）

8. **Session 管理方法**
   - `session.undo`, `session.branch`, `session.compress`
   - `session.cwd.set`, `session.status`

9. **附件方法**
   - `image.attach`, `pdf.attach`, `file.attach`
   - `image.detach`

10. **其他高频方法**
    - `setup.status`, `setup.runtime_check`
    - `complete.path`, `complete.slash`
    - `toolsets.list`

### 阶段 4：REST 端点补齐（5-7 天）

11. **会话管理端点**
    - `GET /api/sessions/stats`
    - `GET /api/sessions/empty/count`, `DELETE /api/sessions/empty`
    - `POST /api/sessions/bulk-delete`

12. **MCP 端点**
    - `GET/POST /api/mcp/servers/*`
    - `GET /api/mcp/catalog`

13. **文件管理端点**
    - `GET/POST/DELETE /api/files/*`
    - `POST /api/files/upload`

14. **OAuth/Ticket 端点**
    - `GET /api/auth/providers`
    - `POST /api/auth/ws-ticket`
    - `GET/POST /login`, `GET /auth/callback`

### 阶段 5：功能完善（持续）

15. **实现技能 toggle 持久化**
16. **集成 ToolsetManager**
17. **实现 Cron trigger 执行**
18. **实现音频端点（优先 OpenAI）**
19. **实现 Session TTL 清理**
20. **实现 Profile 后端池**

---

## 六、测试建议

1. **端到端测试**：使用 Desktop Electron 实际连接 Rust 后端
2. **事件完整性测试**：记录 Python 后端的事件流，对比 Rust 后端
3. **API 覆盖率测试**：自动化对比 REST/RPC 端点实现率
4. **安全测试**：CORS、Host Header、Token 比较、Rate Limiting
5. **性能测试**：1000+ 会话并发，内存泄漏检测

---

*报告结束*
