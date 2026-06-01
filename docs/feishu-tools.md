# 飞书工具配置指南

## 概述

Hermes Agent 通过 4 个原生 Rust 工具对接飞书开放平台 API，覆盖日历、文档、任务和聊天记录四大场景：

| 工具名称 | 功能域 |
|----------|--------|
| `feishu_calendar` | 日历事件管理（查询、创建、忙闲查询） |
| `feishu_docs` | 文档操作（搜索、阅读、创建、追加） |
| `feishu_task` | 任务管理（创建、列表、更新、完成） |
| `feishu_chat_history` | 聊天记录（获取消息、搜索群组、查看成员） |

典型场景：通过微信/IM 向 Agent 发送自然语言指令，Agent 自动调用飞书工具完成日历安排、文档协作、任务分配等操作。

## 前置条件

- 拥有飞书开放平台账号（[https://open.feishu.cn](https://open.feishu.cn)）
- 创建一个**企业自建应用**（非商店应用）
- 应用已发布并处于可用状态
- 如需对接 Lark（海外版），需使用 Lark 域名

## 环境变量配置

Agent 启动前需设置以下环境变量：

| 变量名 | 必填 | 说明 | 示例值 |
|--------|------|------|--------|
| `FEISHU_APP_ID` | 是 | 飞书应用的 App ID | `cli_a5xxxxxxxxxxxxx` |
| `FEISHU_APP_SECRET` | 是 | 飞书应用的 App Secret | `xxxxxxxxxxxxxxxxxxxxxxxx` |
| `FEISHU_DOMAIN` | 否 | 域名选择，默认 `feishu` | `feishu` 或 `lark` |

> 当 `FEISHU_DOMAIN` 设为 `lark` 时，API 基地址切换为 `https://open.larksuite.com/open-apis`；否则使用 `https://open.feishu.cn/open-apis`。

### Shell (bash/zsh)

```bash
export FEISHU_APP_ID="cli_a5xxxxxxxxxxxxx"
export FEISHU_APP_SECRET="xxxxxxxxxxxxxxxxxxxxxxxx"
export FEISHU_DOMAIN="feishu"   # 海外版用 "lark"
```

### PowerShell

```powershell
$env:FEISHU_APP_ID = "cli_a5xxxxxxxxxxxxx"
$env:FEISHU_APP_SECRET = "xxxxxxxxxxxxxxxxxxxxxxxx"
$env:FEISHU_DOMAIN = "feishu"   # 海外版用 "lark"
```

## 飞书应用创建步骤

1. 登录 [飞书开放平台](https://open.feishu.cn)
2. 点击「创建应用」，选择**企业自建应用**
3. 填写应用名称和描述，完成创建
4. 在「凭证与基础信息」页面获取 **App ID** 和 **App Secret**
5. 在「权限管理」中按需申请下方权限清单中的 scope
6. 配置完成后点击「发布」，等待管理员审批通过

## 权限清单

以下为各工具所需的飞书 API 权限 scope，按需申请：

### feishu_calendar

| 权限 Scope | 说明 |
|------------|------|
| `calendar:calendar` | 读写日历 |
| `calendar:calendar:readonly` | 只读日历 |
| `calendar:calendar:freebusy:readonly` | 查询忙闲信息 |

### feishu_docs

| 权限 Scope | 说明 |
|------------|------|
| `docs:doc` | 读写文档 |
| `docs:doc:readonly` | 只读文档 |
| `search:entity` | 搜索文档 |
| `drive:drive` | 云空间文件管理 |

### feishu_task

| 权限 Scope | 说明 |
|------------|------|
| `task:task` | 读写任务 |
| `task:task:readonly` | 只读任务 |

### feishu_chat_history

| 权限 Scope | 说明 |
|------------|------|
| `im:message` | 读写消息 |
| `im:message:readonly` | 只读消息 |
| `im:chat` | 读写群组信息 |
| `im:chat:readonly` | 只读群组信息 |

## 工具详细说明

### feishu_calendar

与飞书日历交互，支持查询事件、创建事件和忙闲查询。

**参数定义：**

| 参数名 | 类型 | 必填 | 描述 |
|--------|------|------|------|
| `action` | string | 是 | 操作类型，可选值：`list_events`、`create_event`、`free_busy` |
| `calendar_id` | string | 否 | 日历 ID，默认 `primary` |
| `start_time` | string | 否 | 开始时间，ISO 8601 格式 |
| `end_time` | string | 否 | 结束时间，ISO 8601 格式 |
| `summary` | string | 条件必填 | 事件标题（`create_event` 时必填） |
| `description` | string | 否 | 事件描述 |
| `attendees` | array[string] | 否 | 参会人邮箱或 open_id 列表 |
| `user_id` | string | 否 | 用户 ID（仅 `free_busy` 查询使用） |

**API 映射：**

| Action | HTTP 方法 | 飞书 API |
|--------|-----------|----------|
| `list_events` | GET | `/calendar/v4/calendars/{calendar_id}/events` |
| `create_event` | POST | `/calendar/v4/calendars/{calendar_id}/events` |
| `free_busy` | POST | `/calendar/v4/freebusy/list` |

**示例（创建日程）：**

```json
{
  "action": "create_event",
  "summary": "产品评审会议",
  "start_time": "2026-06-01T14:00:00+08:00",
  "end_time": "2026-06-01T15:00:00+08:00",
  "description": "Q3 产品路线图评审",
  "attendees": ["alice@company.com", "bob@company.com"]
}
```

### feishu_docs

与飞书文档交互，支持搜索、阅读、创建和追加内容。

**参数定义：**

| 参数名 | 类型 | 必填 | 描述 |
|--------|------|------|------|
| `action` | string | 是 | 操作类型，可选值：`search`、`read`、`create`、`append` |
| `query` | string | 条件必填 | 搜索关键词（`search` 时必填） |
| `document_id` | string | 条件必填 | 文档 ID（`read` 和 `append` 时必填） |
| `title` | string | 条件必填 | 文档标题（`create` 时必填） |
| `content` | string | 条件必填 | 写入的文本内容（`append` 时必填） |
| `folder_token` | string | 否 | 目标文件夹 token（仅 `create` 使用） |
| `doc_type` | string | 否 | 按文档类型过滤，可选值：`doc`、`sheet`、`bitable`（仅 `search` 使用） |

**API 映射：**

| Action | HTTP 方法 | 飞书 API |
|--------|-----------|----------|
| `search` | POST | `/suite/docs-api/search/object` |
| `read` | GET | `/docx/v1/documents/{document_id}/blocks` |
| `create` | POST | `/docx/v1/documents` |
| `append` | POST | `/docx/v1/documents/{document_id}/blocks/{block_id}/children` |

> `append` 操作默认将内容追加到文档根 block（`block_id` 等于 `document_id`），以文本段落形式插入。

**示例（搜索文档）：**

```json
{
  "action": "search",
  "query": "季度报告",
  "doc_type": "doc"
}
```

### feishu_task

与飞书任务交互，支持创建、列表、更新和完成任务。

**参数定义：**

| 参数名 | 类型 | 必填 | 描述 |
|--------|------|------|------|
| `action` | string | 是 | 操作类型，可选值：`create`、`list`、`update`、`complete` |
| `summary` | string | 条件必填 | 任务标题（`create` 时必填；`update` 时可选） |
| `due` | string | 否 | 截止时间，ISO 8601 格式 |
| `description` | string | 否 | 任务描述 |
| `task_id` | string | 条件必填 | 任务 ID（`update` 和 `complete` 时必填） |
| `assignees` | array[string] | 否 | 负责人用户 ID 列表（仅 `create` 使用） |

**API 映射：**

| Action | HTTP 方法 | 飞书 API |
|--------|-----------|----------|
| `create` | POST | `/task/v2/tasks` |
| `list` | GET | `/task/v2/tasks` |
| `update` | PATCH | `/task/v2/tasks/{task_id}` |
| `complete` | POST | `/task/v2/tasks/{task_id}/complete` |

**示例（创建任务）：**

```json
{
  "action": "create",
  "summary": "完成 API 对接文档",
  "due": "2026-06-05T18:00:00+08:00",
  "description": "编写飞书工具的对外 API 文档",
  "assignees": ["ou_xxxxxxxxxxxxx"]
}
```

### feishu_chat_history

与飞书聊天记录交互，支持获取消息、搜索群组和查看群成员。

**参数定义：**

| 参数名 | 类型 | 必填 | 描述 |
|--------|------|------|------|
| `action` | string | 是 | 操作类型，可选值：`get_messages`、`search_chats`、`get_chat_members` |
| `chat_id` | string | 条件必填 | 群组 ID（`get_messages` 和 `get_chat_members` 时必填） |
| `start_time` | string | 否 | 消息起始时间，ISO 8601 格式 |
| `end_time` | string | 否 | 消息截止时间，ISO 8601 格式 |
| `query` | string | 否 | 群名称搜索关键词（仅 `search_chats` 使用） |
| `page_size` | integer | 否 | 每页结果数，默认 20，最大 50 |

**API 映射：**

| Action | HTTP 方法 | 飞书 API |
|--------|-----------|----------|
| `get_messages` | GET | `/im/v1/messages` |
| `search_chats` | GET | `/im/v1/chats` |
| `get_chat_members` | GET | `/im/v1/chats/{chat_id}/members` |

> 时间参数会自动从 ISO 8601 转换为 Unix 时间戳（秒）传递给飞书 API。支持 RFC 3339、`%Y-%m-%dT%H:%M:%S` 和 `%Y-%m-%d %H:%M:%S` 三种格式。

**示例（获取群消息）：**

```json
{
  "action": "get_messages",
  "chat_id": "oc_xxxxxxxxxxxxx",
  "start_time": "2026-05-29T00:00:00+08:00",
  "end_time": "2026-05-29T23:59:59+08:00",
  "page_size": 30
}
```

## 使用示例

以下是用户通过自然语言与 Agent 交互的典型场景：

| 用户输入 | Agent 调用 |
|----------|-----------|
| "帮我看看今天有什么日程" | `feishu_calendar` → `action: list_events`, `start_time: 2026-05-29T00:00:00+08:00` |
| "明天下午3点到4点安排一个产品评审会，拉上 alice 和 bob" | `feishu_calendar` → `action: create_event`, `summary: 产品评审会` |
| "搜一下飞书里有没有'季度报告'相关的文档" | `feishu_docs` → `action: search`, `query: 季度报告` |
| "创建一个新文档叫'会议纪要'" | `feishu_docs` → `action: create`, `title: 会议纪要` |
| "把刚才讨论的结论追加到那个文档里" | `feishu_docs` → `action: append`, `document_id: ...`, `content: ...` |
| "创建一个任务：完成 API 文档，截止下周五" | `feishu_task` → `action: create`, `summary: 完成 API 文档`, `due: ...` |
| "我有哪些未完成的任务？" | `feishu_task` → `action: list` |
| "把那个任务标记为已完成" | `feishu_task` → `action: complete`, `task_id: ...` |
| "看看产品群最近一小时的消息" | `feishu_chat_history` → `action: search_chats` → `action: get_messages` |
| "这个群里都有谁？" | `feishu_chat_history` → `action: get_chat_members`, `chat_id: ...` |

## 常见问题

### Token 管理

Agent 内置 tenant_access_token 自动管理机制：
- Token 有效期为 2 小时，到期前 5 分钟自动刷新
- 采用读写锁双重检查，并发安全
- 如果 Token 获取失败，Agent 会返回明确的错误信息，检查 App ID / App Secret 是否正确

### 权限不足

调用 API 返回 `code != 0` 时，通常原因：
- 应用未申请对应权限 scope
- 权限已申请但管理员尚未审批
- 应用可见范围未包含目标用户/群组

**解决**：在飞书开放平台「权限管理」中确认 scope 已开通且已审批通过。

### 频率限制

飞书 API 有调用频率限制（通常为 100 次/分钟，具体因接口而异）。如遇限流：
- Agent 会返回包含错误码的 `ToolError`
- 建议适当降低调用频率或增加调用间隔

### 域名切换（飞书 vs Lark）

- 国内用户使用默认的 `feishu` 域名即可
- 海外用户需将 `FEISHU_DOMAIN` 设为 `lark`
- 两者使用不同的 API 基地址和应用凭证，不可混用

### 时间格式

所有时间参数使用 ISO 8601 格式，推荐使用带时区偏移的写法：

```
2026-06-01T14:00:00+08:00
```

聊天记录工具（`feishu_chat_history`）额外支持以下格式：
- `2026-06-01T14:00:00`（无时区，按本地时间处理）
- `2026-06-01 14:00:00`（空格分隔，按本地时间处理）
