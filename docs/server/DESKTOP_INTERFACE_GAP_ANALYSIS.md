# Desktop 接口兼容性缺口分析报告

> **分析日期**：2026-06-11
> **分析范围**：Desktop Electron 应用（main.cjs + renderer）实际调用的所有接口
> **分析依据**：
> - `C:\workspace\hermes-agent\apps\desktop\electron\main.cjs`（226 KB，6219 行）
> - `C:\workspace\hermes-agent\apps\desktop\src\hermes.ts`
> - `C:\workspace\hermes-agent\apps\desktop\src\types\hermes.ts`
> - `C:\workspace\hermes-agent\apps\desktop\src\app\session\hooks\use-prompt-actions.ts`
> - `C:\workspace\hermes-agent\apps\desktop\src\lib\voice-playback.ts`
> - `C:\workspace\hermes-agent\apps\desktop\src\app\settings\*.tsx`

---

## 一、Desktop 接口总览

Desktop 通过三类机制与后端通信：

| 机制 | 用途 | 入口 |
|------|------|------|
| **REST API** | 配置/CRUD 操作 | `window.hermesDesktop.api({ path, method, body })` |
| **JSON-RPC over WS** | 实时聊天会话交互 | `gateway.request(method, params)` |
| **原生 IPC** | 文件系统/剪贴板/窗口管理 | `window.hermesDesktop.<namespace>.<method>()` |

**关键发现**：
- Desktop 前端不使用 `ipcRenderer.invoke('hermes:api')`，而是通过统一 IPC Bridge `window.hermesDesktop`
- REST 调用通过 Electron main process 代理到后端（带 `X-Hermes-Session-Token` Header）
- WS 由 Renderer 直接维护，main.cjs 仅提供 URL 和连接探测
- **音频接口使用 base64 data URL（非 multipart）**

---

## 二、REST API 缺口分析

### 2.1 主进程直接调用（main.cjs 内部）

| 优先级 | 方法 | 路径 | 用途 | Rust 状态 | 缺口 |
|--------|------|------|------|-----------|------|
| P0 | GET | `/api/status` | 后端健康探测 | ✅ 已实现 | 无 |
| P0 | POST | `/api/auth/ws-ticket` | OAuth 网关 WS ticket | ❌ 未实现 | **阻断远程模式** |
| P1 | GET | `/api/auth/providers` | OAuth 提供商列表 | ❌ 未实现 | 远程模式 |
| P1 | GET | `/api/profiles/sessions` | 跨 Profile 会话聚合 | ✅ 已实现 | 无 |
| P1 | GET | `/api/sessions` | 远程 Profile 会话列表 | ✅ 已实现 | 无 |

### 2.2 IPC 代理调用（Renderer → Main → Backend）

#### 2.2.1 会话管理（高频）

| 优先级 | 方法 | 路径 | 用途 | Rust 状态 | 缺口 |
|--------|------|------|------|-----------|------|
| P0 | GET | `/api/sessions?limit=&offset=&min_messages=&archived=&order=` | 侧边栏会话列表 | ✅ 已实现 | 无 |
| P0 | GET | `/api/profiles/sessions?...` | 跨 Profile 统一会话列表 | ✅ 已实现 | 无 |
| P0 | GET | `/api/sessions/{id}/messages` | 读取会话消息 | ✅ 已实现 | 无 |
| P0 | PATCH | `/api/sessions/{id}` | 重命名/归档会话 | ✅ 已实现 | 无 |
| P0 | DELETE | `/api/sessions/{id}` | 删除会话 | ✅ 已实现 | 无 |
| P0 | GET | `/api/sessions/search?q=` | 会话搜索 | ✅ 已实现 | 无 |
| P1 | GET | `/api/sessions/stats` | 会话统计 | ❌ 未实现 | **缺失** |
| P1 | GET | `/api/sessions/empty/count` | 空会话计数 | ❌ 未实现 | **缺失** |
| P1 | DELETE | `/api/sessions/empty` | 清理空会话 | ❌ 未实现 | **缺失** |
| P1 | POST | `/api/sessions/bulk-delete` | 批量删除会话 | ❌ 未实现 | **缺失** |

#### 2.2.2 配置管理（高频）

| 优先级 | 方法 | 路径 | 用途 | Rust 状态 | 缺口 |
|--------|------|------|------|-----------|------|
| P1 | GET | `/api/config` | 读取 Hermes 配置 | ✅ 已实现 | 无 |
| P1 | GET | `/api/config/defaults` | 配置默认值 | ✅ 已实现 | 无 |
| P1 | GET | `/api/config/schema` | 配置 schema | ✅ 已实现 | 无 |
| P1 | PUT | `/api/config` | 保存配置 | ✅ 已实现 | 无 |
| P1 | GET | `/api/env` | 读取环境变量列表 | ✅ 已实现 | 无 |
| P1 | PUT | `/api/env` | 设置环境变量 | ✅ 已实现 | 无 |
| P1 | DELETE | `/api/env` | 删除环境变量 | ✅ 已实现 | 无 |
| P1 | POST | `/api/env/reveal` | 回显环境变量明文 | ❌ 未实现 | **缺失** |
| P1 | POST | `/api/providers/validate` | 验证 Provider Key | ❌ 未实现 | **缺失** |

#### 2.2.3 模型与 Provider（中频）

| 优先级 | 方法 | 路径 | 用途 | Rust 状态 | 缺口 |
|--------|------|------|------|-----------|------|
| P1 | GET | `/api/model/info` | 全局模型信息 | ✅ 已实现 | 无 |
| P1 | GET | `/api/model/options` | 可用模型选项 | ✅ 已实现 | 无 |
| P1 | GET | `/api/model/recommended-default?provider=` | 推荐默认模型 | ✅ 已实现 | 无 |
| P1 | POST | `/api/model/set` | 设置全局模型 | ✅ 已实现 | 无 |
| P1 | GET | `/api/model/auxiliary` | 辅助模型列表 | ✅ 已实现 | 无 |
| P2 | GET | `/api/auth/providers` | OAuth 提供商列表 | ❌ 未实现 | 远程模式 |
| P2 | POST | `/api/providers/oauth/{id}/start` | 启动 OAuth 登录 | ❌ 未实现 | 远程模式 |
| P2 | POST | `/api/providers/oauth/{id}/submit` | 提交 OAuth 授权码 | ❌ 未实现 | 远程模式 |
| P2 | GET | `/api/providers/oauth/{id}/poll/{sessionId}` | 轮询 OAuth 结果 | ❌ 未实现 | 远程模式 |
| P2 | DELETE | `/api/providers/oauth/sessions/{sessionId}` | 取消 OAuth 会话 | ❌ 未实现 | 远程模式 |

#### 2.2.4 技能与工具（设置页）

| 优先级 | 方法 | 路径 | 用途 | Rust 状态 | 缺口 |
|--------|------|------|------|-----------|------|
| P1 | GET | `/api/skills` | 技能列表 | ✅ 已实现 | 无 |
| P1 | PUT | `/api/skills/toggle` | 启用/禁用技能 | ⚠️ Mock | **未持久化** |
| P1 | GET | `/api/tools/toolsets` | 工具集列表 | ⚠️ Mock | 仅返回默认 "all" |
| P1 | PUT | `/api/tools/toolsets/{name}` | 更新工具集启用状态 | ⚠️ Mock | **未持久化** |
| P1 | GET | `/api/tools/toolsets/{name}/config` | 工具集配置 | ⚠️ Mock | 仅返回全量 schemas |
| P1 | PUT | `/api/tools/toolsets/{name}/provider` | 选择工具集 Provider | ⚠️ Mock | **未持久化** |
| P1 | POST | `/api/tools/toolsets/{name}/post-setup` | 工具集安装后配置 | ⚠️ Mock | **未执行** |

#### 2.2.5 消息平台（设置页）

| 优先级 | 方法 | 路径 | 用途 | Rust 状态 | 缺口 |
|--------|------|------|------|-----------|------|
| P2 | GET | `/api/messaging/platforms` | 消息平台列表 | ✅ 已实现 | Mock 数据 |
| P2 | PUT | `/api/messaging/platforms/{id}` | 更新消息平台 | ✅ 已实现 | Mock 响应 |
| P2 | POST | `/api/messaging/platforms/{id}/test` | 测试消息平台连接 | ✅ 已实现 | Mock 响应 |

#### 2.2.6 Cron 作业（低频）

| 优先级 | 方法 | 路径 | 用途 | Rust 状态 | 缺口 |
|--------|------|------|------|-----------|------|
| P2 | GET | `/api/cron/jobs` | 列出任务 | ✅ 已实现 | `FileJobPersistence` |
| P2 | GET | `/api/cron/jobs/{id}` | 任务详情 | ✅ 已实现 | `FileJobPersistence` |
| P2 | GET | `/api/cron/jobs/{id}/runs?limit=` | 运行历史 | ⚠️ Mock | 返回空数组 |
| P2 | POST | `/api/cron/jobs` | 创建任务 | ✅ 已实现 | `FileJobPersistence` |
| P2 | PUT | `/api/cron/jobs/{id}` | 更新任务 | ✅ 已实现 | `FileJobPersistence` |
| P2 | POST | `/api/cron/jobs/{id}/pause` | 暂停任务 | ✅ 已实现 | `FileJobPersistence` |
| P2 | POST | `/api/cron/jobs/{id}/resume` | 恢复任务 | ✅ 已实现 | `FileJobPersistence` |
| P2 | POST | `/api/cron/jobs/{id}/trigger` | 手动触发 | ⚠️ Mock | **未执行** |
| P2 | DELETE | `/api/cron/jobs/{id}` | 删除任务 | ✅ 已实现 | `FileJobPersistence` |
| P2 | GET | `/api/cron/delivery-targets` | 投递目标 | ❌ 未实现 | **缺失** |

#### 2.2.7 Profile 管理（设置页）

| 优先级 | 方法 | 路径 | 用途 | Rust 状态 | 缺口 |
|--------|------|------|------|-----------|------|
| P1 | GET | `/api/profiles` | Profile 列表 | ✅ 已实现 | 无 |
| P1 | POST | `/api/profiles` | 创建 Profile | ✅ 已实现 | 无 |
| P1 | PATCH | `/api/profiles/{name}` | 重命名 Profile | ✅ 已实现 | 无 |
| P1 | DELETE | `/api/profiles/{name}` | 删除 Profile | ✅ 已实现 | 无 |
| P2 | GET | `/api/profiles/{name}/soul` | 读取 Profile Soul | ❌ 未实现 | **缺失** |
| P2 | PUT | `/api/profiles/{name}/soul` | 更新 Profile Soul | ❌ 未实现 | **缺失** |
| P2 | GET | `/api/profiles/{name}/setup-command` | 读取 Profile 启动命令 | ❌ 未实现 | **缺失** |

#### 2.2.8 系统与状态（中频）

| 优先级 | 方法 | 路径 | 用途 | Rust 状态 | 缺口 |
|--------|------|------|------|-----------|------|
| P1 | GET | `/api/status` | 后端就绪探测 | ✅ 已实现 | 无 |
| P1 | GET | `/api/logs?file=&lines=&level=&component=` | 读取后端日志 | ✅ 已实现 | 别名 |
| P1 | POST | `/api/gateway/restart` | 重启 Gateway | ⚠️ 占位符 | **未执行** |
| P1 | POST | `/api/hermes/update` | 触发 Hermes 更新 | ❌ 未实现 | **缺失** |
| P1 | GET | `/api/hermes/update/check?force=` | 检查后端更新 | ❌ 未实现 | **缺失** |
| P2 | GET | `/api/actions/{name}/status?lines=` | 动作状态日志 | ❌ 未实现 | **缺失** |
| P2 | GET | `/api/analytics/usage?days=` | 用量统计 | ❌ 未实现 | **缺失** |

#### 2.2.9 音频（P2 功能）

**关键发现**：Desktop 音频接口使用 base64 data URL，非 multipart。

| 优先级 | 方法 | 路径 | 请求格式 | 响应格式 | Rust 状态 | 缺口 |
|--------|------|------|----------|----------|-----------|------|
| P2 | POST | `/api/audio/transcribe` | `{data_url, mime_type}` | `{ok, transcript, provider}` | ❌ 未实现 | **语音输入阻断** |
| P2 | POST | `/api/audio/speak` | `{text}` | `{ok, data_url, mime_type, provider}` | ❌ 未实现 | **语音输出阻断** |
| P2 | GET | `/api/audio/elevenlabs/voices` | - | `{available, voices: [{voice_id, name, label}]}` | ❌ 未实现 | **语音列表阻断** |

**音频实现方案（OpenAI 优先）**：
- **STT**：解析 base64 data URL → temp 文件 → `SttEngine(Whisper)` → 返回 `{ok, transcript, provider: "openai"}`
- **TTS**：`MultiTtsBackend(OpenAI tts-1)` → 读取音频文件 → base64 data URL → 返回 `{ok, data_url, mime_type, provider: "openai"}`
- **Voice 列表**：返回 OpenAI 6 个 voice（alloy/echo/fable/onyx/nova/shimmer），兼容 ElevenLabs 响应格式

#### 2.2.10 MCP（设置页关键）

| 优先级 | 方法 | 路径 | 用途 | Rust 状态 | 缺口 |
|--------|------|------|------|-----------|------|
| P2 | GET | `/api/mcp/servers` | 列出 MCP 服务器 | ❌ 未实现 | **设置页阻断** |
| P2 | POST | `/api/mcp/servers` | 添加 MCP 服务器 | ❌ 未实现 | **设置页阻断** |
| P2 | DELETE | `/api/mcp/servers/{name}` | 删除 MCP 服务器 | ❌ 未实现 | **设置页阻断** |
| P2 | POST | `/api/mcp/servers/{name}/test` | 测试 MCP 连接 | ❌ 未实现 | **设置页阻断** |
| P2 | PUT | `/api/mcp/servers/{name}/enabled` | 启用/禁用 MCP | ❌ 未实现 | **设置页阻断** |
| P2 | GET | `/api/mcp/catalog` | MCP 目录浏览 | ❌ 未实现 | 延后 |
| P2 | POST | `/api/mcp/catalog/install` | 一键安装 MCP | ❌ 未实现 | 延后 |

---

## 三、JSON-RPC 方法缺口分析

### 3.1 Desktop 通过 WS 调用的方法

| 方法 | 用途 | 调用位置 | Rust 状态 | 缺口 |
|------|------|----------|-----------|------|
| `session.create` | 创建会话 | 聊天初始化 | ✅ 已实现 | 无 |
| `session.list` | 列出会话 | 会话切换 | ✅ 已实现 | 无 |
| `session.close` | 关闭会话 | 会话管理 | ✅ 已实现 | 无 |
| `prompt.submit` | 提交消息 | 聊天输入 | ✅ 已实现 | 无 |
| `session.interrupt` | 中断会话 | 停止生成 | ✅ 已实现 | 无 |
| `config.set` | 设置配置 | 模型切换 | ✅ 已实现 | 无 |
| `slash.exec` | 执行 slash 命令 | 模型控制菜单 | ❌ 未实现 | **功能缺失** |
| `reload.mcp` | 重载 MCP 配置 | MCP 设置页 | ❌ 未实现 | **功能缺失** |
| `reload.env` | 重载环境变量 | 设置页 | ❌ 未实现 | **功能缺失** |
| `terminal.read.respond` | 终端输入响应 | 消息流处理 | ⚠️ 未绑定 | **事件缺失** |

### 3.2 缺失的高频方法（Desktop 未直接调用但 Python 有）

| 方法 | 用途 | Rust 状态 | 缺口 |
|------|------|-----------|------|
| `session.undo` | 撤销操作 | ❌ 未实现 | 功能缺失 |
| `session.branch` | 分支会话 | ❌ 未实现 | 功能缺失 |
| `session.compress` | 压缩历史 | ❌ 未实现 | 功能缺失 |
| `complete.path` | 路径补全 | ❌ 未实现 | 输入体验 |
| `complete.slash` | 命令补全 | ❌ 未实现 | 输入体验 |
| `toolsets.list` | 列出工具集 | ❌ 未实现 | 设置页 |
| `setup.status` | 设置状态 | ❌ 未实现 | 诊断 |
| `setup.runtime_check` | 运行时检查 | ❌ 未实现 | 诊断 |

---

## 四、WebSocket 事件缺口分析

### 4.1 Desktop 订阅的事件（按优先级排序）

| 事件名 | Desktop 用途 | 优先级 | Rust 状态 | 缺口 |
|--------|-------------|--------|-----------|------|
| `gateway.ready` | WS 握手成功 | P0 | ✅ 已实现 | 无 |
| `message.start` | 新消息流开始 | P0 | ✅ 已实现 | 无 |
| `message.delta` | 消息内容增量 | P0 | ✅ 已实现 | 无 |
| `message.complete` | 消息流结束 | P0 | ✅ 已实现 | 无 |
| `thinking.delta` | 思考过程增量 | P1 | ✅ 已实现 | 无 |
| `tool.start` | 工具调用开始 | P1 | ✅ 已实现 | 无 |
| `tool.complete` | 工具调用完成 | P1 | ✅ 已实现 | 无 |
| `error` | 后端错误 | P1 | ✅ 已实现 | 无 |
| `session.info` | **会话元数据更新（最频繁）** | **P0** | ❌ **未实现** | **严重缺失** |
| `status.update` | 状态栏更新 | P1 | ❌ 未实现 | **缺失** |
| `tool.progress` | 工具执行进度 | P1 | ❌ 未实现 | **缺失** |
| `tool.generating` | 工具正在生成 | P1 | ❌ 未实现 | **缺失** |
| `reasoning.delta` | 推理过程增量 | P1 | ❌ 未实现 | **缺失** |
| `reasoning.available` | 推理内容可用通知 | P1 | ❌ 未实现 | **缺失** |
| `approval.request` | 请求用户审批 | P1 | ⚠️ 部分 | 仅在 interaction_dispatch |
| `clarify.request` | 请求用户澄清 | P1 | ⚠️ 部分 | 仅在 interaction_dispatch |
| `sudo.request` | 请求 sudo 密码 | P1 | ⚠️ 部分 | 仅在 interaction_dispatch |
| `secret.request` | 请求密钥/凭据 | P1 | ⚠️ 部分 | 仅在 interaction_dispatch |
| `background.complete` | 后台任务完成 | P2 | ❌ 未实现 | 功能缺失 |
| `notification.show` | 通知显示 | P2 | ❌ 未实现 | 功能缺失 |
| `notification.clear` | 通知清除 | P2 | ❌ 未实现 | 功能缺失 |
| `voice.transcript` | 语音转录结果 | P2 | ❌ 未实现 | 功能缺失 |
| `voice.status` | 语音状态更新 | P2 | ❌ 未实现 | 功能缺失 |
| `browser.progress` | 浏览器操作进度 | P2 | ❌ 未实现 | 功能缺失 |
| `skin.changed` | 主题皮肤变更 | P2 | ❌ 未实现 | 功能缺失 |

### 4.2 `session.info` 事件详细规格

**Python 中最频繁发射的事件，Desktop 依赖此事件更新侧边栏 UI。**

**触发时机**：
- `session.create` 成功后
- `session.resume` 成功后
- `session.close` 后
- `session.title` 更新后
- 消息计数变化后（每次 DB 写入后）
- `prompt.submit` 开始前/后
- 会话状态变化时

**事件格式**：
```json
{
  "jsonrpc": "2.0",
  "method": "event",
  "params": {
    "type": "session.info",
    "session_id": "abc123",
    "payload": {
      "session_id": "abc123",
      "session_key": "20250611_123456_abc123",
      "message_count": 42,
      "title": "Session Title",
      "model": "gpt-4o",
      "cwd": "/home/user/project",
      "cols": 120,
      "running": false,
      "branch": false,
      "lazy": true,
      "desktop_contract": 2,
      "profile_name": "default",
      "tools": ["web_search", "execute_code"],
      "skills": ["coding", "analysis"],
      "last_active": "2026-06-11T12:34:56Z"
    }
  }
}
```

---

## 五、认证机制缺口

| 机制 | Desktop 期望 | Rust 实现 | 状态 | 影响 |
|------|-------------|-----------|------|------|
| **Token 模式** | `X-Hermes-Session-Token` Header | ✅ 已实现 | 一致 | 无 |
| **WS Token** | `?token=<token>` | ✅ 已实现 | 一致 | 无 |
| **CORS** | 严格 localhost 正则 | ⚠️ `permissive()` | **安全漏洞** | 允许任意来源 |
| **Host Header** | DNS Rebinding 防护 | ❌ 未实现 | **安全漏洞** | 攻击面 |
| **Rate Limit** | `/api/env/reveal` 限流 | ❌ 未实现 | 功能缺失 | 无防护 |
| **OAuth Cookie** | `hermes_session_at/rt` | ❌ 未实现 | 远程模式阻断 | 远程不可用 |
| **WS Ticket** | `?ticket=<ticket>` (30s TTL) | ✅ 解析 | Ticket 生成未实现 | 远程模式 |
| `/api/auth/ws-ticket` | 铸造一次性 ticket | ❌ 未实现 | **远程模式阻断** | WS 无法连接 |
| `/api/auth/providers` | OAuth 提供商列表 | ❌ 未实现 | **远程模式阻断** | 登录不可用 |

---

## 六、缺口汇总与优先级

### 6.1 P0 - Desktop 启动阻断（必须立即修复）

| # | 缺口项 | 影响 | 预估工作量 |
|---|--------|------|-----------|
| 1 | `session.info` 事件未实现 | 侧边栏无法显示会话状态 | 1 天 |
| 2 | CORS 过于宽松 (`permissive()`) | 安全风险 | 2 小时 |
| 3 | Host Header 验证缺失 | DNS Rebinding 攻击面 | 2 小时 |
| 4 | WS 断开处理缺失 | 会话泄漏 | 1 天 |
| 5 | `--profile` CLI 参数缺失 | Profile 切换失败 | 4 小时 |
| 6 | `inflight_turn` 状态缺失 | "正在输入"状态不显示 | 4 小时 |

### 6.2 P1 - 高频功能缺失（影响日常体验）

| # | 缺口项 | 影响 | 预估工作量 |
|---|--------|------|-----------|
| 7 | `POST /api/env/reveal` | 环境变量明文显示 | 4 小时 |
| 8 | `GET /api/sessions/stats` | 侧边栏统计 | 4 小时 |
| 9 | `GET/DELETE /api/sessions/empty` | 空会话清理 | 4 小时 |
| 10 | `POST /api/sessions/bulk-delete` | 批量删除 | 4 小时 |
| 11 | `status.update` 事件 | 状态栏更新 | 4 小时 |
| 12 | `tool.progress/generating` 事件 | 工具动画 | 4 小时 |
| 13 | `approval/clarify/sudo/secret` 绑定到 AgentCallbacks | 审批弹窗 | 1 天 |
| 14 | `slash.exec` RPC | 模型控制菜单 | 4 小时 |
| 15 | `reload.mcp/reload.env` RPC | 热重载 | 4 小时 |
| 16 | `complete.path/slash` | 输入补全 | 1 天 |

### 6.3 P2 - 设置页功能缺失（影响设置体验）

| # | 缺口项 | 影响 | 预估工作量 |
|---|--------|------|-----------|
| 17 | MCP 端点（7个） | MCP 设置页无法工作 | 2-3 天 |
| 18 | 技能 toggle 持久化 | 技能设置不保存 | 4 小时 |
| 19 | ToolsetManager 集成 | 工具集配置不完整 | 1-2 天 |
| 20 | `GET /api/cron/delivery-targets` | Cron 投递目标 | 2 小时 |
| 21 | `POST /api/cron/jobs/{id}/trigger` 执行 | Cron 手动触发 | 1 天 |
| 22 | `POST /api/audio/transcribe` | 语音输入 | 1 天 |
| 23 | `POST /api/audio/speak` | 语音输出 | 1 天 |
| 24 | `GET /api/audio/elevenlabs/voices` | 语音列表 | 2 小时 |
| 25 | OAuth 完整流程 | 远程模式不可用 | 3-5 天 |

### 6.4 P3 - 低频/延后功能

| # | 缺口项 | 影响 | 预估工作量 |
|---|--------|------|-----------|
| 26 | 文件管理端点 | 附件上传 | 2-3 天 |
| 27 | `session.undo/branch/compress` | 会话管理增强 | 2-3 天 |
| 28 | Session TTL 清理 | 内存泄漏 | 1 天 |
| 29 | Profile 后端池 | 多 profile 并发 | 2-3 天 |
| 30 | `image.attach/pdf.attach/file.attach` | 附件功能 | 2-3 天 |

---

## 七、实施路线图（Desktop 优先）

### 阶段 1：Desktop 启动修复（2-3 天）
**目标**：Desktop 能启动并显示会话列表

- [ ] 实现 `session.info` 事件（最高优先级）
- [ ] 修复 CORS 为严格 localhost
- [ ] 添加 Host Header 验证中间件
- [ ] 实现 WS 断开清理逻辑
- [ ] 添加 `--profile` CLI 参数
- [ ] 实现 `inflight_turn` 跟踪

### 阶段 2：高频功能补齐（3-4 天）
**目标**：日常高频操作正常工作

- [ ] `POST /api/env/reveal`
- [ ] `GET /api/sessions/stats`
- [ ] `GET/DELETE /api/sessions/empty`
- [ ] `POST /api/sessions/bulk-delete`
- [ ] `status.update` 事件
- [ ] `tool.progress/generating` 事件
- [ ] `slash.exec` RPC
- [ ] `reload.mcp/reload.env` RPC
- [ ] `complete.path/slash`

### 阶段 3：交互事件完善（2-3 天）
**目标**：审批/澄清弹窗正常工作

- [ ] 将 `interaction_dispatch` 绑定到 `AgentCallbacks`
- [ ] `approval.request` 事件验证端到端
- [ ] `clarify/sudo/secret.request` 验证
- [ ] 超时自动拒绝（deny）

### 阶段 4：设置页功能（4-5 天）
**目标**：设置页面所有标签正常工作

- [ ] MCP 端点（7个）
- [ ] 技能 toggle 持久化
- [ ] ToolsetManager 集成
- [ ] `GET /api/cron/delivery-targets`
- [ ] Cron trigger 执行

### 阶段 5：音频功能（2-3 天）
**目标**：语音输入/输出可用

- [ ] `POST /api/audio/transcribe`（OpenAI Whisper）
- [ ] `POST /api/audio/speak`（OpenAI TTS）
- [ ] `GET /api/audio/elevenlabs/voices`（返回 OpenAI voices）

### 阶段 6：高级功能（可选延后）

- [ ] OAuth 完整流程（如果不使用远程模式可延后）
- [ ] 文件管理端点
- [ ] `session.undo/branch/compress`
- [ ] Session TTL 清理
- [ ] Profile 后端池

---

## 八、预期时间线

| 阶段 | 时间 | Desktop 可用度 |
|------|------|---------------|
| 阶段 1 | 2-3 天 | Desktop 能启动、连接、显示会话列表 |
| 阶段 1-2 | 1 周 | 日常聊天、设置页核心功能可用 |
| 阶段 1-3 | 1.5 周 | 审批/澄清弹窗、语音输入输出可用 |
| 阶段 1-4 | 2.5-3 周 | 设置页所有标签基本可用 |
| 阶段 1-5 | 3-3.5 周 | 完整聊天体验（含语音） |

---

## 九、待确认决策

1. **远程模式（OAuth）是否必须？**
   - 如果 Desktop 只使用本地模式 → OAuth 可延后到阶段 6
   - 如果需要远程托管后端 → OAuth 需要提前到阶段 2

2. **MCP 设置页是否立即需要？**
   - Desktop `mcp-settings.tsx` 调用 `reload.mcp` 和 MCP REST 端点
   - 如果用户不使用 MCP → 可延后
   - 如果需要 → 阶段 4 优先实现

3. **语音功能是否必须？**
   - 已确认：优先使用 OpenAI 端点（Whisper/tts-1）
   - ElevenLabs 等其他提供商暂不考虑

---

*报告结束*
