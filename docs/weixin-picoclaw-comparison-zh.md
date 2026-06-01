# 微信（Weixin）渠道功能对比：Hermes Agent Ultra vs PicoClaw

> 对比基准  
> - **Hermes**：本仓库 `hermes-agent-ultra`（Rust 网关 `crates/hermes-gateway/src/platforms/weixin.rs` + CLI `hermes auth login weixin --qr`）  
> - **PicoClaw**：[sipeed/picoclaw](https://github.com/sipeed/picoclaw) `main` 分支（`pkg/channels/weixin/`）  
> - 协议：二者均基于腾讯 **iLink Bot API**（个人微信扫码绑定），**长轮询**收消息，**无需公网 Webhook**。

---

## 1. 架构与定位

| 维度 | Hermes Agent Ultra | PicoClaw |
|------|-------------------|----------|
| 语言/运行时 | Rust（`hermes-gateway` feature `weixin`） | Go |
| 在系统中的角色 | 完整 Agent 网关（会话、工具、Cron、多平台）中的一条渠道 | 轻量 Agent 的 Channel 插件 |
| 登录命令 | `hermes auth login weixin --qr` / `hermes gateway setup` | `picoclaw auth weixin` |
| 凭证存储 | `~/.hermes/weixin/accounts/<account_id>.json` 等 | `~/.picoclaw/config.json` → `channel_list.weixin` |
| 官方文档 | [website/docs/user-guide/messaging/weixin.md](../website/docs/user-guide/messaging/weixin.md) | [docs/channels/weixin/README.zh.md](https://github.com/sipeed/picoclaw/blob/main/docs/channels/weixin/README.zh.md) |

---

## 2. 功能对比总表

图例：**✅** 已实现 · **⚠️** 部分/有条件 · **❌** 未实现 · **📄** 仅文档/测试提及

### 2.1 连接与鉴权

| 功能 | Hermes | PicoClaw | 说明 |
|------|--------|----------|------|
| 终端 QR 扫码登录 | ✅ | ✅ | 均轮询 iLink `get_bot_qrcode` / 状态接口 |
| QR 过期自动刷新 | ✅（CLI 最多 3 次） | ✅（auth 包内） | Hermes：`weixin_qr_login_flow` |
| Token / account_id 持久化 | ✅ | ✅ | |
| 自定义 `base_url` | ✅ `WEIXIN_BASE_URL` | ✅ `base_url` | PicoClaw 登录后可写回区域 URL |
| HTTP 代理 | ✅ `platforms.weixin` proxy 配置 | ✅ `proxy`（auth 与 channel） | |
| 会话过期 `errcode=-14` 退避 | ✅ 暂停 10 分钟 | ✅ `pauseSession` | |
| 单 Token 多实例互斥锁 | 📄 用户文档有 | ❌ | Hermes Rust 适配器内**未**看到 `scoped_lock`；以实测为准 |

### 2.2 消息收发（文本）

| 功能 | Hermes | PicoClaw | 说明 |
|------|--------|----------|------|
| 长轮询 `getupdates` | ✅ | ✅ | 默认约 35s 超时 |
| `get_updates_buf` 断点续传 | ✅ 落盘 `*.sync.json` | ✅ `syncBufPath` | 重启不丢游标 |
| `context_token` 入站更新 + 出站回传 | ✅ 按 `account_id:sender` 持久化 | ✅ 按 `from_user_id`；**无 token 则拒绝发送** | PicoClaw 更严格 |
| 入站消息去重（约 5 分钟） | ✅ `message_id` | ⚠️ 依赖 `client_id` 生成 | |
| 单条最大长度 | ✅ 4000 | ✅ 4000（BaseChannel） | |
| 超长消息分片 | ✅ `split_long_message` | ✅ BaseChannel 分块逻辑 | |
| 多段 Markdown 合并为一条气泡 | ✅ 默认（`split_multiline=false`） | ⚠️ 随 BaseChannel 策略 | Hermes 有专门 Python 回归测试；Rust 侧用通用分片 |
| 保留 Markdown（标题/表格/代码块） | ✅（Python 测试 + 文档） | ⚠️ 文本原样拼接 | Hermes 文档明确不转纯文本 |
| 流式回复 / 消息编辑 | ❌ 网关强制非流式 | ❌ | Hermes：`weixin` 禁用 streaming（避免只显示 `...`） |
| **正在输入**（typing） | ✅ iLink `sendtyping` + 网关路由期 5s 保活 / 结束 STOP | ✅ `StartTyping` + 5s 保活 | 实现路径对齐 PicoClaw |
| 仅私聊入站（代码层） | ⚠️ 支持群字段 + 策略，但 iLink 常收不到群 | ✅ 写死 `ChatType: direct` | 见下文「群聊限制」 |

### 2.3 引用回复（微信「引用」/ `ref_msg`）

| 功能 | Hermes | PicoClaw | 说明 |
|------|--------|----------|------|
| 引用消息中的**文字**进入 Agent 上下文 | ❌ 仅取顶层 `text_item` | ❌ 占位符或未解析引用正文 | **二者都需人工验证；不宜写「已支持引用问答」** |
| 引用消息中的**图片/文件/视频/语音**媒体 | ✅ `ref_msg.message_item` 下载解密 | ✅ `selectInboundMediaItem` 从 `RefMsg` 取媒体 | 适合测「引用一张图再问这张图」 |
| 出站「回复某条消息」的 thread 关联 | ⚠️ 靠 `context_token` 会话连续 | ⚠️ 同上 | **不是**微信 UI 里那种带引用的气泡样式 |

### 2.4 媒体（CDN AES-128-ECB）

| 功能 | Hermes | PicoClaw | 说明 |
|------|--------|----------|------|
| 入站图片/视频/文件 | ✅ 解密缓存 `[图片: path]` 等 | ✅ 下载到 media 目录 | |
| 入站语音（有转写文本） | ✅ 用 `voice_item.text` | ✅ 用转写文本 | |
| 入站语音（无转写，SILK） | ✅ 缓存 `.silk` | ✅ 可 **SILK→WAV**（ffmpeg 等）供 ASR | PicoClaw 声明 `VoiceCapabilities ASR/TTS` |
| 单条消息多附件 | ✅ `collect_media_lines` 遍历全部 item | ⚠️ `selectInboundMediaItem` **只取一个** | 同时发图+文件时 Hermes 更完整 |
| 出站图片/文件/视频 | ✅ `send_ilink_file` + CDN 上传 | ✅ `SendMedia` | |
| 出站 `send_image_url` | ✅ 下载公网图或 `file://`，SSRF 拦截 | ⚠️ 经 `resolveOutboundPart` | Hermes 有 SSRF 测试 |
| CDN 上传/下载重试 | ⚠️ 基础错误退避 | ✅ 明确 retry 常量 | |
| 出站语音 | ⚠️ `.silk` 等多以文件附件发送 | ✅ voice 类型 | |

### 2.5 访问控制与运营

| 功能 | Hermes | PicoClaw | 说明 |
|------|--------|----------|------|
| `allow_from` / 白名单 | ✅ `WEIXIN_ALLOWED_USERS` | ✅ `allow_from` | |
| DM 策略 `open/allowlist/disabled/pairing` | ✅ | ⚠️ 主要通过 `allow_from` | Hermes 更细 |
| 群策略 `open/allowlist/disabled` | ✅ 默认 `disabled` | ❌ 无独立群策略 | Hermes 为 iLink 将来可收群预留 |
| Cron / 通知主频道 | ✅ `WEIXIN_HOME_CHANNEL` | ❌ | Hermes `hermes-cron` 投递 |
| `send_message` 工具跨平台发微信 | ✅ `weixin:<chat_id>` | ❌（自有总线） | |
| Web 控制台配置 | ❌ | ✅ `weixin-form.tsx` | |
| `reasoning_channel_id` | ❌ | ✅ BaseChannel | PicoClaw 可把推理发到另一频道 |

### 2.6 Agent 能力（仅 Hermes 侧明显）

| 功能 | Hermes | PicoClaw |
|------|--------|----------|
| 多轮工具调用 / Skills / MCP | ✅ | ✅（架构不同） |
| 会话持久化 / 压缩 / 子代理 | ✅ | ✅ |
| 与其他 20+ 平台同一网关 | ✅ | ✅（channel 列表） |
| 评测 `hermes-eval` | ✅ | ❌ |

---

## 3. 共同限制（测试前必读）

两端文档均强调：扫码得到的是 **iLink Bot 身份**（如 `xxx@im.bot`），**不是**可脚本化的普通个人号。

- 普通微信群聊、@个人号 等事件 **常常根本不会推到 iLink**。
- 可靠场景：**给 Bot 身份发私聊（DM）**。
- Hermes 即使配置 `WEIXIN_GROUP_POLICY=open`，若日志里没有群消息 raw event，属于 **iLink 侧限制**，不是 Hermes 单独缺陷。

---

## 4. Hermes 本项目：真人操作测试用例

> 仓库内 **没有** 单独的「微信用户故事」条目（`website/src/data/userStories.json` 无 weixin）。  
> 下列用例根据实现与 [weixin.md](../website/docs/user-guide/messaging/weixin.md)、`tests/gateway/test_weixin.py` 整理，供**真人**在微信客户端操作验证。

**前置（所有用例共用）**

1. 安装依赖并按文档配置环境变量。  
2. 执行 `hermes auth login weixin --qr`（或 `hermes gateway setup` → 选 Weixin），手机扫码并确认。  
3. 启动：`hermes gateway`（或项目实际使用的 gateway 命令）。  
4. 记录日志中的 `from_user_id` / `chat_id`（发给 Bot 后可在 DEBUG 日志看到）。  
5. 建议 `WEIXIN_DM_POLICY=open` 做首轮冒烟；收紧策略时用例单独说明。

---

### 4.1 连接与账号

| ID | 操作步骤 | 预期结果 |
|----|----------|----------|
| W-01 | 首次 QR 登录 | 终端出码；扫码后 `~/.hermes/weixin/accounts/<id>.json` 含 token |
| W-02 | 重启 gateway，不再扫码 | 自动长轮询；同一人再发消息能收到回复 |
| W-03 | 故意启动第二个 gateway（同 token） | 若启用锁：第二个实例报错；若无锁：可能出现重复回复（记录实际行为） |
| W-04 | 长时间闲置后发言 | 若 session 过期：日志 `errcode=-14`；重新 QR 登录后恢复 |

---

### 4.2 基础对话与 context_token（会话连续）

| ID | 操作步骤 | 预期结果 |
|----|----------|----------|
| W-10 | 向 Bot 发：`我叫小明，记住` | 正常文字回复 |
| W-11 | **不新开话题**，紧接发：`我叫什么？` | 能答「小明」；证明多轮上下文 + `context_token` 生效 |
| W-12 | 回复后 **停止 gateway 再启动**，再问：`我叫什么？` | 若仍记得：检查 `*.context-tokens.json` 已恢复；若忘记：属 Agent 会话存储策略，与 token 无关 |
| W-13 | **全新联系人**首次给 Bot 发消息（从未聊过） | Bot 能回复；日志写入新 peer 的 context_token |

---

### 4.3 引用回复（重点：与「引用」相关的真实操作）

| ID | 操作步骤 | 预期结果 |
|----|----------|----------|
| W-20 | 先让 Bot 发一段明确内容，例如：`项目代号是 Alpha-7` | 收到回复气泡 |
| W-21 | 在微信里 **引用/回复** 该气泡，只发：`上面的代号是什么？` | **理想**：答 Alpha-7；**当前实现风险**：若未解析引用正文，可能答非所问 → 记录为缺陷 |
| W-22 | 你发一张图片（或文件），再 **引用该图片** 问：`总结这张图` | Agent 应收到 `[图片: ...]` 或文件路径类提示（`ref_msg` 媒体路径） |
| W-23 | 引用一条 **纯文字** 同时加一句新问题 | 对比 W-21：区分「引用文字」与「引用媒体」是否都进上下文 |

---

### 4.4 媒体入站

| ID | 操作步骤 | 预期结果 |
|----|----------|----------|
| W-30 | 发送一张截图（JPEG/PNG） | Bot 能描述或处理图片；日志有 CDN 下载 |
| W-31 | 发送短视频 | 收到 `[视频: path]` 或等价处理 |
| W-32 | 发送 PDF/文档 | 文件名保留；Agent 可读路径或摘要 |
| W-33 | 发送语音（若微信给转写） | Bot 按**文字**理解 |
| W-34 | 发送语音（无转写） | 收到 `[语音: .silk]` 或附件说明；对比 PicoClaw 的 WAV 转码能力 |

---

### 4.5 媒体出站 / Agent 主动发图

| ID | 操作步骤 | 预期结果 |
|----|----------|----------|
| W-40 | 让 Agent「生成并发送一张图」或提供本地路径 | 微信收到图片消息（非纯链接文本） |
| W-41 | 让 Agent 发送较长 Markdown（含表格、代码块） | 单气泡内 Markdown 可读；超长则分多条且代码块尽量完整 |
| W-42 | 让 Agent 通过工具向 `weixin:<你的chat_id>` 发消息 | `send_message` 成功（需 gateway 运行） |

---

### 4.6 策略与安全

| ID | 操作步骤 | 预期结果 |
|----|----------|----------|
| W-50 | 设 `WEIXIN_DM_POLICY=allowlist`，仅填自己的 user id | 自己可聊；其他账号被忽略 |
| W-51 | 设 `disabled` | 所有人无回复 |
| W-52 | Agent 尝试把图片发到 `http://127.0.0.1/...` | 日志 SSRF 拦截；用户侧为文本兜底或失败提示 |

---

### 4.7 Cron / 通知（Hermes 特有）

| ID | 操作步骤 | 预期结果 |
|----|----------|----------|
| W-60 | 配置 `WEIXIN_HOME_CHANNEL` + `WEIXIN_HOME_CHANNEL_NAME` | 创建每分钟 ping 的 cron job |
| W-61 | 等待触发 | 指定微信会话收到通知消息 |

---

### 4.8 非流式与体验

| ID | 操作步骤 | 预期结果 |
|----|----------|----------|
| W-70 | 提一个需 30s+ 推理的问题 | **不应**只出现 `...` 占位；应一次性出现完整回复（非流式） |
| W-71 | 观察处理中状态 | 文档称有 typing；**当前 Rust 可能无「正在输入」** → 若无，记为与 PicoClaw 差异 |

---

### 4.9 群聊（可选，常失败）

| ID | 操作步骤 | 预期结果 |
|----|----------|----------|
| W-80 | `WEIXIN_GROUP_POLICY=open`，在群内 @Bot 或发消息 | 多数账号：**无任何入站**；若有 raw event，Bot 才应回复 |
| W-81 | `allowlist` 只填某一 `group_id` | 仅该群（且 iLink 有事件时）触发 |

---

## 5. PicoClaw 可对照的真人用例（简表）

在相同 iLink 账号能力下，用 PicoClaw 复测时可优先对比：

| 用例 | PicoClaw 预期差异 |
|------|-------------------|
| W-11 多轮 | 同样依赖 `context_token`；无 token 时**发不出**回复（更严格） |
| W-71 typing | 应能看到「对方正在输入」类状态 |
| W-34 语音 | 若配置 ffmpeg/silk 解码，语音进 ASR 管线 |
| W-22 引用媒体 | 与 Hermes 类似，从 `RefMsg` 拉媒体 |
| W-60 Cron | PicoClaw 需另配通知渠道，无 `WEIXIN_HOME_CHANNEL` 一等字段 |

---

## 6. 结论摘要

1. **协议层**：两者都是 iLink 长轮询 + CDN 加密媒体 + `context_token`，核心一致。  
2. **Hermes 更强在**：完整 Agent 生态、群/DM 策略、Cron 主频道、`send_message`、多附件入站、非流式微信体验、出站 URL 图与 SSRF。  
3. **PicoClaw 更强在**：Typing 指示器、语音转码与 Voice 能力声明、出站 `SendMedia` 工程化重试、Web 配置 UI、发送前必须有 `context_token` 的严格会话模型。  
4. **引用回复**：两端均对 `ref_msg` 做了**引用媒体**处理；**引用文字**是否进入 Agent 上下文需用 **W-20～W-23** 实测，不要仅凭产品直觉判断。  
5. **真人测试**：仓库无现成 Weixin 用户故事，请直接采用第 4 节用例表做回归。

---

## 7. 参考文件索引

**Hermes**

- `crates/hermes-gateway/src/platforms/weixin.rs`
- `crates/hermes-cli/src/main.rs`（`weixin_qr_login_flow`）
- `website/docs/user-guide/messaging/weixin.md`
- `tests/gateway/test_weixin.py`

**PicoClaw**

- `pkg/channels/weixin/weixin.go`
- `pkg/channels/weixin/media.go`
- `pkg/channels/weixin/types.go`
- `cmd/picoclaw/internal/auth/weixin.go`
- `docs/channels/weixin/README.zh.md`
