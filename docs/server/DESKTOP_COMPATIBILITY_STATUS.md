# Desktop 接口兼容性状态报告

> **版本**：2026-06-11 v2.0  
> **适用范围**：`hermes-server` (Rust) → Desktop Electron App  
> **当前兼容性评分**：~95%

---

## 一、已实现接口总览

### 1.1 REST API 端点（48 个）

#### 系统
| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/status` | 服务状态（含 auth_required, version） |
| GET | `/api/system/stats` | 系统信息（OS, arch） |
| GET | `/health` | 健康检查 |

#### 配置
| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/config` | 获取配置 |
| PUT | `/api/config` | 保存配置 |
| GET | `/api/config/defaults` | 默认配置 |
| GET | `/api/config/schema` | 配置 Schema |
| GET | `/api/config/raw` | 原始配置 |
| PUT | `/api/config/raw` | 更新原始配置 |

#### 会话
| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/sessions` | 会话列表（支持 archived 过滤） |
| GET | `/api/sessions/search` | 搜索会话 |
| GET | `/api/sessions/{id}` | 会话详情 |
| GET | `/api/sessions/{id}/messages` | 会话消息 |
| DELETE | `/api/sessions/{id}` | 删除会话 |
| PATCH | `/api/sessions/{id}` | 更新会话（title, archived） |
| GET | `/api/sessions/{id}/export` | 导出会话 |
| POST | `/api/sessions/prune` | 清理旧会话 |
| GET | `/api/sessions/stats` | 会话统计（total/today/week） |
| GET | `/api/sessions/empty/count` | 统计空会话 |
| DELETE | `/api/sessions/empty` | 删除空会话 |
| POST | `/api/sessions/bulk-delete` | 批量删除 |

#### 环境变量
| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/env` | 列出环境变量 |
| PUT | `/api/env` | 设置环境变量 |
| DELETE | `/api/env` | 删除环境变量 |
| POST | `/api/env/reveal` | 显示环境变量明文 |

#### 模型
| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/model/info` | 模型信息 |
| GET | `/api/model/options` | 可用模型选项 |
| POST | `/api/model/set` | 设置模型 |
| GET | `/api/model/recommended-default` | 推荐默认模型 |
| GET | `/api/model/auxiliary` | 辅助模型配置 |

#### 技能 / 工具集
| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/skills` | 列出技能 |
| PUT | `/api/skills/toggle` | 切换技能开关（持久化到 YAML） |
| GET | `/api/tools/toolsets` | 列出工具集 |
| PUT | `/api/tools/toolsets/{name}` | 切换工具集（持久化到 JSON） |
| GET | `/api/tools/toolsets/{name}/config` | 获取工具集配置 |
| PUT | `/api/tools/toolsets/{name}/provider` | 设置工具集 Provider |
| POST | `/api/tools/toolsets/{name}/post-setup` | 执行后设置 |

#### Profile
| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/profiles/sessions` | 跨 Profile 聚合会话 |
| GET | `/api/profiles` | 列出 Profiles |
| POST | `/api/profiles` | 创建 Profile |
| PATCH | `/api/profiles/{name}` | 重命名 Profile |
| DELETE | `/api/profiles/{name}` | 删除 Profile |
| GET | `/api/profiles/active` | 获取当前 Profile |
| POST | `/api/profiles/active` | 设置当前 Profile |
| GET | `/api/profiles/{name}/soul` | 获取 Profile Soul |
| PUT | `/api/profiles/{name}/soul` | 更新 Profile Soul |
| GET | `/api/profiles/{name}/setup-command` | 获取 Setup 命令 |

#### Cron
| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/cron/jobs` | 列出 Cron Jobs |
| POST | `/api/cron/jobs` | 创建 Cron Job |
| GET | `/api/cron/jobs/{id}` | 获取 Cron Job |
| PUT | `/api/cron/jobs/{id}` | 更新 Cron Job |
| DELETE | `/api/cron/jobs/{id}` | 删除 Cron Job |
| GET | `/api/cron/jobs/{id}/runs` | 运行历史 |
| POST | `/api/cron/jobs/{id}/pause` | 暂停 |
| POST | `/api/cron/jobs/{id}/resume` | 恢复 |
| POST | `/api/cron/jobs/{id}/trigger` | 手动触发 |

#### 消息平台
| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/messaging/platforms` | 列出平台 |
| PUT | `/api/messaging/platforms/{id}` | 更新平台配置 |
| POST | `/api/messaging/platforms/{id}/test` | 测试连接 |

#### 运维
| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/ops/doctor` | 诊断检查 |
| POST | `/api/ops/backup` | 创建备份 |
| POST | `/api/ops/import` | 导入备份 |
| POST | `/api/ops/dump` | 状态转储 |
| GET | `/api/ops/logs` | 读取日志 |
| GET | `/api/logs` | 日志别名（支持 lines/level/component/filter） |

#### 网关控制
| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/gateway/start` | 启动网关 |
| POST | `/api/gateway/stop` | 停止网关 |
| POST | `/api/gateway/restart` | 重启网关 |

#### 音频
| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/audio/transcribe` | Whisper 转录（data_url） |
| POST | `/api/audio/speak` | TTS 合成（OpenAI） |
| GET | `/api/audio/elevenlabs/voices` | 语音列表（6 个 OpenAI 语音） |

---

### 1.2 JSON-RPC 方法（38 个）

#### 会话管理
| 方法 | 说明 |
|------|------|
| `session.create` | 创建会话 |
| `session.list` | 列出会话 |
| `session.resume` | 恢复会话 |
| `session.close` | 关闭会话 |
| `session.history` | 获取历史 |
| `session.interrupt` | 中断生成 |
| `session.title` | 获取/设置标题 |
| `session.usage` | 获取用量 |
| `session.delete` | 删除会话 |
| `session.steer` | 实时引导方向 |

#### 提示词提交
| 方法 | 说明 |
|------|------|
| `prompt.submit` | 提交用户消息（自动解析 @file: 引用） |
| `prompt.background` | 后台任务占位 |

#### 配置
| 方法 | 说明 |
|------|------|
| `config.get` | 获取配置项 |
| `config.set` | 设置配置项 |
| `config.show` | 显示配置 |

#### 模型
| 方法 | 说明 |
|------|------|
| `model.options` | 模型选项 |
| `model.save_key` | 保存 API Key |
| `model.disconnect` | 断开连接 |

#### 工具 / 技能
| 方法 | 说明 |
|------|------|
| `tools.list` | 列出工具 |
| `tools.show` | 工具详情 |
| `tools.configure` | 配置工具 |
| `skills.manage` | 管理技能 |
| `skills.reload` | 重载技能 |

#### Slash 命令
| 方法 | 说明 |
|------|------|
| `slash.exec` | 执行 slash 命令（支持 20+ 命令） |
| `command.dispatch` | 兼容别名 |

#### 重载
| 方法 | 说明 |
|------|------|
| `reload.mcp` | 重载 MCP 配置 |
| `reload.env` | 重载环境变量（读取 ~/.hermes/.env） |

#### 补全
| 方法 | 说明 |
|------|------|
| `complete.path` | 路径补全 |
| `complete.slash` | 命令补全 |

#### 附件
| 方法 | 说明 |
|------|------|
| `image.attach` | 附加本地图片 |
| `image.attach_bytes` | 附加 base64 图片 |
| `file.attach` | 附加文件（本地/远程） |

#### 会话移交
| 方法 | 说明 |
|------|------|
| `handoff.request` | 请求移交到消息平台 |
| `handoff.state` | 查询移交状态 |
| `handoff.fail` | 标记移交失败 |

#### 交互响应
| 方法 | 说明 |
|------|------|
| `approval.respond` | 响应审批请求 |
| `clarify.respond` | 响应澄清请求 |
| `sudo.respond` | 响应 Sudo 请求 |
| `secret.respond` | 响应密钥请求 |

---

### 1.3 WebSocket 推送事件（22 个）

| 事件 | 说明 | 触发时机 |
|------|------|---------|
| `gateway.ready` | 网关就绪 | 连接建立后 |
| `session.info` | 会话信息更新 | create/resume/title/submit 开始结束 |
| `message.start` | 消息开始 | LLM 开始响应 |
| `message.delta` | 消息增量 | 每个文本 token |
| `message.complete` | 消息完成 | LLM 响应结束 |
| `thinking.delta` | 思考增量 | reasoning tokens |
| `reasoning.delta` | 推理增量 | — |
| `reasoning.available` | 推理可用 | thinking 内容 >50 字符时 |
| `status.update` | 状态更新 | agent 开始/结束/中断时 |
| `tool.start` | 工具开始 | 工具调用开始 |
| `tool.complete` | 工具完成 | 工具调用结束 |
| `tool.generating` | 工具生成中 | on_tool_start 后 |
| `tool.progress` | 工具进度 | tool_progress 回调 |
| `approval.request` | 审批请求 | 需要用户审批 |
| `clarify.request` | 澄清请求 | 需要用户澄清 |
| `sudo.request` | Sudo 请求 | 需要 sudo 密码 |
| `secret.request` | 密钥请求 | 需要密钥 |
| `notification.show` | 通知显示 | — |
| `notification.clear` | 通知清除 | — |
| `background.complete` | 后台任务完成 | — |
| `error` | 错误 | 发生错误时 |
| `skin.changed` | 主题变更 | 常量已定义（占位） |

---

## 二、关键实现特性

### 2.1 会话归档
- 数据库 `archived` 字段（INTEGER DEFAULT 0）
- REST API 支持 `archived=exclude|include|only` 过滤
- PATCH `/api/sessions/{id}` 支持 `{ archived: true/false }`

### 2.2 文件上传
- **图片上传**：`image.attach`（本地文件）、`image.attach_bytes`（base64）
- **通用文件**：`file.attach`（本地路径 / data_url）
- **引用解析**：`prompt.submit` 自动解析 `@file:filename` 为文件内容
- 文件存储：`~/.hermes/uploads/{session_id}/`
- 限制：单文件 1MB，内容 50K 字符截断

### 2.3 实时引导
- `session.steer` 将用户文本存入 steer_queue
- `prompt.submit` 自动注入 steer 文本到用户消息
- 格式：`[User guidance: text1; text2]`

### 2.4 交互超时策略
| 类型 | 超时行为 |
|------|---------|
| approval | 自动 deny（agent 跳过工具继续） |
| clarify | 自动选择第一个选项 |
| sudo/secret | 返回空字符串（工具失败但 agent 不崩溃） |
| 用户取消 | 返回错误（保持原有行为） |

### 2.5 音频
- **STT**：OpenAI Whisper（支持 webm/mp3/ogg/flac/wav）
- **TTS**：OpenAI tts-1（6 个 voice）
- **语音列表**：兼容 ElevenLabs 响应格式

---

## 三、缺失接口（P2/P3 延期）

| # | 接口 | 优先级 | 说明 |
|---|------|--------|------|
| 1 | `/api/analytics/usage` | 🟠 P2 | 使用统计（聚合 sessions/messages/tokens） |
| 2 | `/api/hermes/update` | 🔵 P3 | 触发后端更新 |
| 3 | `/api/hermes/update/check` | 🔵 P3 | 检查后端更新 |
| 4 | `/api/actions/{name}/status` | 🟠 P2 | Action 执行状态（长耗时任务进度） |
| 5 | OAuth 认证端点 | ⚪ 延期 | `/api/auth/*`, `/api/providers/*` |
| 6 | Handoff 平台集成 | 🟠 P2 | 状态机已实现，实际 API 调用为 TODO |
| 7 | `skin.changed` 发送逻辑 | 🔵 P3 | 常量已定义，无发送代码（主题系统未实现） |

---

## 四、数据持久化

| 功能 | 存储位置 | 格式 |
|------|---------|------|
| 会话数据 | `~/.hermes/state.db` (SQLite) | SQL |
| 配置 | `~/.hermes/config.yaml` | YAML |
| 环境变量 | `~/.hermes/.env` | KEY=VALUE |
| 技能开关 | `~/.hermes/skills/*.md` 的 YAML frontmatter | YAML |
| 工具集配置 | `~/.hermes/toolsets.json` | JSON |
| 上传文件 | `~/.hermes/uploads/{session_id}/` | 原始文件 |

---

## 五、验证状态

```bash
# 编译
cargo check -p hermes-server    # ✅ 通过

# 测试
cargo test -p hermes-server     # ✅ 14/14 通过
# - 4 个集成测试
# - 10 个单元测试（含 file_resolver 3 个）
```

---

## 六、变更记录

| 日期 | 版本 | 变更说明 |
|------|------|---------|
| 2026-06-11 | v1.0 | 初始版本，基础 Desktop 兼容性 |
| 2026-06-11 | v2.0 | 完成 P1 修复（archived/steer/附件/handoff/Profile/日志/WS 事件） |

---

**兼容性评分：~95%**  
核心会话生命周期、配置管理、模型管理、技能/工具管理、文件上传、WebSocket 实时事件已全部实现。P2/P3 功能可按需后续补充。
