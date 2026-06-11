# Desktop 接口兼容性实施计划

> **创建日期**：2026-06-11
> **目标**：确保 Desktop Electron 应用连接 Rust 后端时，所有核心功能正常工作
> **原则**：Desktop 优先、快速迭代、先通后优

---

## 一、项目概述

### 1.1 当前状态

Desktop 已可启动并正常聊天（核心功能可用）：
- ✅ 启动流程（`/api/status` + WS 连接 + `gateway.ready`）
- ✅ 核心聊天（`prompt.submit` + `message.*` 事件）
- ✅ 工具调用（`tool.start/complete` + `thinking.delta`）
- ✅ 基础会话管理（CRUD + 消息加载）
- ✅ 基础设置页（配置/模型/环境变量读写）

### 1.2 问题概述

但存在严重体验问题：
- ❌ 侧边栏不更新（`session.info` 缺失）
- ❌ 设置页更改不保存（技能/Toolset 未持久化）
- ❌ 状态栏/工具动画缺失
- ❌ 语音输入/输出不可用
- ❌ 安全加固未完成

---

## 二、任务优先级总览

### Tier 0: 已可用（无需修复）
- 启动流程、核心聊天、工具调用、基础会话管理、基础设置页

### Tier 1: 体验严重下降（立即修复）
| # | 任务 | 影响 | 工作量 |
|---|------|------|--------|
| 1 | `session.info` 事件 | 侧边栏不更新 | 1 天 |
| 2 | `POST /api/env/reveal` | 环境变量明文显示 | 4 小时 |
| 3 | 技能 toggle 持久化 | 技能设置无效 | 4 小时 |
| 4 | Toolset 配置持久化 | 工具集设置无效 | 1 天 |

### Tier 2: 功能缺失（本周内）
| # | 任务 | 影响 | 工作量 |
|---|------|------|--------|
| 5 | `status.update` 事件 | 状态栏空白 | 4 小时 |
| 6 | `tool.progress/generating` | 工具无动画 | 4 小时 |
| 7 | 会话统计/清理端点 | 统计信息缺失 | 1 天 |
| 8 | `slash.exec` 等 RPC | 模型控制菜单 | 4 小时 |
| 9 | `reload.mcp/env` | 热重载无效 | 4 小时 |
| 10 | `complete.path/slash` | 输入无补全 | 1 天 |

### Tier 3: 音频功能（下周）
| # | 任务 | 影响 | 工作量 |
|---|------|------|--------|
| 11 | `POST /api/audio/transcribe` | 语音输入 | 1 天 |
| 12 | `POST /api/audio/speak` | 语音输出 | 1 天 |
| 13 | `GET /api/audio/elevenlabs/voices` | 语音列表 | 2 小时 |

### Tier 4: 安全加固（可并行）
| # | 任务 | 影响 | 工作量 |
|---|------|------|--------|
| 14 | CORS 严格化 | 安全漏洞 | 2 小时 |
| 15 | Host Header 验证 | DNS Rebinding | 2 小时 |
| 16 | Rate Limiting | 暴力破解 | 2 小时 |

### Tier 5: 高级功能（延后）
- MCP 端点（7个）、OAuth 流程、文件管理、`session.undo/branch/compress`

---

## 三、详细实施计划

### 阶段 1：体验修复（第 1-2 天）

#### 任务 1.1：`session.info` 事件实现

**优先级**：🔴 P0 | **工作量**：1 天

**问题**：Desktop 侧边栏依赖 `session.info` 更新消息数、标题、状态，完全缺失导致 UI 不刷新。

**实现步骤**：
1. 确认 `crates/hermes-server/src/ws/events.rs` 中 `SESSION_INFO` 已定义
2. 在 `session.create/resume/close/title` 成功后发射事件
3. 在 `prompt.submit` 开始前（`running: true`）和结束后（`running: false`）发射
4. 在消息计数变化时发射（每次 DB 写入后）
5. 格式：`{session_id, message_count, title, model, cwd, running, branch, desktop_contract, profile_name}`

**关键代码位置**：
- `crates/hermes-server/src/rpc/session.rs` - session 方法
- `crates/hermes-server/src/rpc/prompt.rs` - prompt.submit
- `crates/hermes-server/src/ws/event_adapter.rs` - 事件构建

**验收标准**：
- [ ] Desktop 创建会话后侧边栏显示正确标题
- [ ] 发送消息后消息计数实时更新
- [ ] 会话关闭后侧边栏状态更新

---

#### 任务 1.2：`POST /api/env/reveal` 端点

**优先级**：🔴 P1 | **工作量**：4 小时

**问题**：设置页"环境变量"标签点击"显示"按钮无反应。

**实现步骤**：
1. 在 `crates/hermes-server/src/rest/env.rs` 中添加 `reveal_env_var` handler
2. 读取 `.env` 文件，返回指定变量的明文值
3. 添加限流：30 秒内最多 5 次请求（返回 429）
4. 返回格式：`{"name": "...", "value": "..."}`

**验收标准**：
- [ ] Desktop 设置页点击"显示"后显示环境变量明文
- [ ] 快速点击多次后触发限流

---

#### 任务 1.3：技能 toggle 持久化

**优先级**：🔴 P1 | **工作量**：4 小时

**问题**：`PUT /api/skills/toggle` 是 mock，刷新后恢复默认状态。

**实现步骤**：
1. 修改 `crates/hermes-server/src/rest/skills.rs` 的 `toggle_skill`
2. 找到对应的 `.md` 文件，修改 YAML frontmatter 中的 `enabled` 字段
3. 保持文件其他内容不变
4. 返回实际更新后的状态

**验收标准**：
- [ ] Desktop 设置页启用/禁用技能后刷新保持状态
- [ ] 文件系统中文 frontmatter 正确更新

---

#### 任务 1.4：Toolset 配置持久化

**优先级**：🔴 P1 | **工作量**：1 天

**问题**：`PUT /api/tools/toolsets/{name}` 是 mock，启用状态/Provider 设置不保存。

**实现步骤**：
1. 设计 toolset 持久化格式（JSON/YAML）
2. 修改 `toggle_toolset` 保存状态到文件
3. 修改 `set_toolset_provider` 保存 provider 配置
4. `list_toolsets` 从文件读取而非返回硬编码

**验收标准**：
- [ ] Desktop 设置页工具集启用状态可保存
- [ ] Provider 配置可保存

---

### 阶段 2：状态与交互（第 3-4 天）

#### 任务 2.1：`status.update` 事件

**优先级**：🟡 P1 | **工作量**：4 小时

**问题**：状态栏不显示"正在思考"/"工具执行中"。

**实现步骤**：
1. 在 `prompt.submit` 开始时发射 `{kind: "processing", message: "Thinking..."}`
2. 在工具调用开始时发射 `{kind: "tool", tool: "..."}`
3. 在完成后发射 `{kind: "ready"}`
4. 绑定到 `AgentCallbacks` 或直接在 `prompt.rs` 中发射

---

#### 任务 2.2：`tool.progress` / `tool.generating` 事件

**优先级**：🟡 P1 | **工作量**：4 小时

**问题**：工具调用无进度动画。

**实现步骤**：
1. 在 `AgentCallbacks.on_tool_start` 后发射 `tool.generating`
2. 在工具执行过程中定期发射 `tool.progress`（如果有进度信息）
3. 在 `on_tool_complete` 前确保已发送 `tool.complete`

---

#### 任务 2.3：会话管理端点

**优先级**：🟡 P1 | **工作量**：1 天

包含：
- `GET /api/sessions/stats`：返回 `{total, today, week}`
- `GET /api/sessions/empty/count`：统计空会话
- `DELETE /api/sessions/empty`：删除空会话
- `POST /api/sessions/bulk-delete`：批量删除（上限 500）

---

### 阶段 3：功能补齐（第 5-6 天）

#### 任务 3.1：高频 RPC 方法

**优先级**：🟡 P1 | **工作量**：1 天

包含：
- `slash.exec`：执行 slash 命令
- `reload.mcp`：重载 MCP 配置
- `reload.env`：重载环境变量
- `complete.path`：路径补全
- `complete.slash`：命令补全

---

#### 任务 3.2：交互事件绑定

**优先级**：🟡 P1 | **工作量**：1 天

**问题**：`approval/clarify/sudo/secret` 仅在 `interaction_dispatch.rs` 中实现，未绑定到 `AgentCallbacks`。

**实现步骤**：
1. 检查 `hermes-agent` 的 `AgentCallbacks` 是否支持交互回调
2. 在 `agent_builder.rs` 中添加对应回调
3. 确保事件格式与 Desktop 预期一致
4. 测试超时自动拒绝逻辑（当前返回错误而非 deny）

---

### 阶段 4：音频功能（第 7-8 天）

#### 任务 4.1：`POST /api/audio/transcribe`（OpenAI Whisper）

**优先级**：🟠 P2 | **工作量**：1 天

**接口格式**：
- 请求：`{data_url: "data:audio/webm;base64,...", mime_type: "audio/webm"}`
- 响应：`{ok: true, transcript: "...", provider: "openai"}`

**实现步骤**：
1. 解析 base64 data URL，提取音频 bytes
2. 写入临时文件 `hermes_stt_{uuid}.webm`
3. 调用 `SttEngine::transcribe_file()`（配置 provider=openai, model=whisper-1）
4. 删除临时文件
5. 返回转录结果

---

#### 任务 4.2：`POST /api/audio/speak`（OpenAI TTS）

**优先级**：🟠 P2 | **工作量**：1 天

**接口格式**：
- 请求：`{text: "..."}`
- 响应：`{ok: true, data_url: "data:audio/mpeg;base64,...", mime_type: "audio/mpeg", provider: "openai"}`

**实现步骤**：
1. 调用 `MultiTtsBackend::synthesize()`（配置 provider=openai, model=tts-1, voice=alloy）
2. 读取生成的音频文件
3. base64 编码为 data URL
4. 删除临时文件
5. 返回 data URL

---

#### 任务 4.3：`GET /api/audio/elevenlabs/voices`

**优先级**：🟠 P2 | **工作量**：2 小时

**实现**：返回 OpenAI 6 个 voice，兼容 ElevenLabs 响应格式：
```json
{
  "available": true,
  "voices": [
    {"voice_id": "alloy", "name": "Alloy", "label": "Alloy"},
    {"voice_id": "echo", "name": "Echo", "label": "Echo"},
    {"voice_id": "fable", "name": "Fable", "label": "Fable"},
    {"voice_id": "onyx", "name": "Onyx", "label": "Onyx"},
    {"voice_id": "nova", "name": "Nova", "label": "Nova"},
    {"voice_id": "shimmer", "name": "Shimmer", "label": "Shimmer"}
  ]
}
```

---

### 阶段 5：安全加固（第 9 天）

#### 任务 5.1：CORS 严格化

**优先级**：🔵 P3 | **工作量**：2 小时

- 从 `CorsLayer::permissive()` 改为严格匹配 `^https?://(localhost|127\.0\.0\.1)(:\d+)?$`
- 或基于 `HERMES_DESKTOP` 环境变量切换

---

#### 任务 5.2：Host Header 验证

**优先级**：🔵 P3 | **工作量**：2 小时

- 中间件检查 `Host` header
- 仅允许 `localhost`、`127.0.0.1`、`::1`
- 其他返回 400

---

#### 任务 5.3：Rate Limiting

**优先级**：🔵 P3 | **工作量**：2 小时

- `/api/env/reveal` 限流：30 秒内最多 5 次
- 返回 429 Too Many Requests

---

### 阶段 6：高级功能（延后）

#### 任务 6.1：MCP 端点（7个）
- `GET/POST/DELETE /api/mcp/servers/*`
- `POST /api/mcp/servers/{name}/test`
- `PUT /api/mcp/servers/{name}/enabled`

#### 任务 6.2：OAuth 完整流程
- `/api/auth/providers`
- `/api/auth/ws-ticket`
- `/login`, `/auth/callback`
- Cookie 管理

#### 任务 6.3：文件管理端点
- `GET/POST/DELETE /api/files/*`

---

## 四、时间线

| 日期 | 阶段 | 完成任务 | Desktop 状态 |
|------|------|----------|-------------|
| Day 1 | 阶段 1 | session.info + env reveal | 侧边栏可刷新 |
| Day 2 | 阶段 1 | 技能/Toolset 持久化 | 设置页可保存 |
| Day 3 | 阶段 2 | status.update + tool.progress | 状态栏正常 |
| Day 4 | 阶段 2 | 会话管理端点 | 批量操作可用 |
| Day 5 | 阶段 3 | RPC 补齐 | 补全/热重载可用 |
| Day 6 | 阶段 3 | 交互事件绑定 | 审批弹窗正常 |
| Day 7 | 阶段 4 | STT + TTS | 语音输入可用 |
| Day 8 | 阶段 4 | Voice 列表 + 音频完善 | 语音输出可用 |
| Day 9 | 阶段 5 | 安全加固 | 安全合规 |

**一周后预期**：Desktop 核心功能完整可用，体验与 Python 后端基本一致。

---

## 五、风险与缓解

| 风险 | 概率 | 影响 | 缓解措施 |
|------|------|------|----------|
| `session.info` 格式与 Desktop 预期不符 | 中 | 高 | 参考 Python `_emit("session.info")` 的 payload 结构 |
| `AgentCallbacks` 不支持交互回调 | 中 | 中 | 检查 `hermes-agent` crate，如需修改则提交 PR |
| 音频临时文件清理失败 | 低 | 低 | 使用 `tempfile` crate 自动清理 |
| 限流实现复杂 | 低 | 低 | 简单内存计数器，不需要 Redis |
| 技能 frontmatter 解析失败 | 低 | 中 | 添加错误处理，跳过格式错误的文件 |
| Toolset 配置文件格式设计 | 中 | 中 | 参考 Python toolset 配置格式 |

---

## 六、验收标准

### 阶段 1 验收（Day 2）
- [ ] Desktop 创建会话后侧边栏显示正确标题和消息数
- [ ] 发送新消息后侧边栏消息计数实时 +1
- [ ] 设置页环境变量点击"显示"显示明文值
- [ ] 设置页启用/禁用技能后刷新保持状态
- [ ] 设置页工具集启用状态可保存

### 阶段 2 验收（Day 4）
- [ ] 发送消息后状态栏显示"Thinking..."
- [ ] 工具调用时显示进度动画
- [ ] 侧边栏底部显示会话统计
- [ ] 可清理空会话
- [ ] 可批量删除会话

### 阶段 3 验收（Day 6）
- [ ] 模型控制菜单 slash 命令可用
- [ ] 保存 MCP/Env 配置后无需重启生效
- [ ] 输入框路径/命令补全
- [ ] 审批/澄清弹窗正常触发和响应

### 阶段 4 验收（Day 8）
- [ ] 语音输入可转录为文字
- [ ] 语音输出可播放
- [ ] 语音列表显示 6 个 OpenAI voice

### 阶段 5 验收（Day 9）
- [ ] CORS 仅允许 localhost
- [ ] Host Header 验证生效
- [ ] `/api/env/reveal` 限流生效

---

*文档版本：v1.0*
*最后更新：2026-06-11*
