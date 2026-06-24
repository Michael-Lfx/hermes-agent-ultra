# 生图 & 生视频 API 对接文档

> 基于 `code/backend` 当前实现整理，供外部客户端（非 FlowyClaw Electron）对接云端图片生成与 Seedance 视频生成能力。  
> 登录与 JWT 获取见 [API.md](../API.md) 认证章节，或 [new-client-api-user-activation.md](../../docs/new-client-api-user-activation.md)。

---

## 1. 概述

| 能力 | 模式 | 客户端 Base | 服务端路由 | 响应包装 |
|------|------|-------------|------------|----------|
| **文生图 / 图片编辑** | 同步代理 | `{host}/claw/v1` | `/v1/images/generations`、`/v1/images/edits` | **上游 JSON 直透**（无 `code/msg/data`） |
| **视频生成（Seedance / 方舟）** | 异步任务 | `{host}/claw` | `/api/v1/video/generations/tasks` | **业务 JSON** `{ code, msg, data }` |

**模型分类**（`tb_model.category` / `GET .../model/availableListClaw?category=`）：

| category | 常量 | 用途 |
|----------|------|------|
| `4` | `ModelCategoryVideo` | 视频模型 |
| `6` | `ModelCategoryImage` | 图片模型 |

---

## 2. Base URL 与路径映射

生产环境示例（国内）：

| 用途 | 客户端 Base URL |
|------|-----------------|
| 业务 API（模型列表、视频任务、积分等） | `https://server.flowyaipc.cn/claw` |
| LLM / 生图 API | `https://server.flowyaipc.cn/claw/v1` |

网关将客户端路径映射到 Go 服务（与 OAuth2 等接口规则一致）：

| 客户端请求 | 服务端实际路由 |
|------------|----------------|
| `GET {业务根}/model/availableListClaw` | `GET /api/v1/model/availableListClaw` |
| `POST {LLM根}/images/generations` | `POST /v1/images/generations` |
| `POST {LLM根}/images/edits` | `POST /v1/images/edits` |
| `POST {业务根}/video/generations/tasks` | `POST /api/v1/video/generations/tasks` |
| `GET {业务根}/video/generations/tasks` | `GET /api/v1/video/generations/tasks` |
| `GET {业务根}/video/generations/tasks/:id` | `GET /api/v1/video/generations/tasks/:id` |
| `DELETE {业务根}/video/generations/tasks/:id` | `DELETE /api/v1/video/generations/tasks/:id` |

测试环境可将 host 换为 `test.flowyaipc.cn`（与现有客户端 `shared/flowy-server.ts` 一致）。

---

## 3. 认证

### 3.1 凭证类型

| 接口 | JWT（`Bearer eyJ...`） | 用户 API Key（`Bearer flowy-...`） |
|------|------------------------|-------------------------------------|
| 生图 `/v1/images/*` | ✅ | ✅ |
| 视频 `/video/generations/*` | ✅ | ✅ |
| 模型列表 `/model/availableListClaw` | ✅ | ❌（仅 JWT） |

API Key 在 `POST /api/v1/user/apiKeys` 创建（需 JWT），完整密钥仅创建时返回一次。

### 3.2 推荐请求头

```http
Content-Type: application/json
Authorization: Bearer <token>
token: <token>
```

`token` 与 `Authorization` 携带相同值，与现有 Flowy 客户端保持一致。

---

## 4. 获取可用模型

### 4.1 业务模型列表（推荐）

| 项 | 值 |
|----|-----|
| 客户端路径 | `GET {业务根}/model/availableListClaw` |
| 需登录 | JWT |

**Query**

| 参数 | 必填 | 说明 |
|------|------|------|
| `category` | 否 | 默认 `1`（对话）。**图片传 `6`，视频传 `4`** |

**Success Response**

```json
{
  "code": 200,
  "msg": "操作成功",
  "data": {
    "cloud": [
      {
        "id": "AIPC-z-image-turbo",
        "name": "Z-Image Turbo",
        "extra": "{\"input\":[\"text\"]}",
        "endpoint": "https://server.flowyaipc.cn/claw/v1",
        "anthropic_endpoint": "https://server.flowyaipc.cn/claw/anthropic/v1",
        "icon": "https://...",
        "category": 6,
        "created_at": "2026-01-01T00:00:00Z"
      }
    ]
  }
}
```

**调用生图 / 生视频时的 `model` 字段：**

- 列表返回的 `id` 形如 `AIPC-<tb_model.name>`，可直接用于请求体 `model`。
- 亦支持 `flowy/<tb_model.name>`（服务端 `StripModelPrefix` 识别 `AIPC-` 与 `flowy/` 两种前缀）。
- 视频任务在无法识别前缀时，会回退将整段 `model` 当作 `tb_model.name` 匹配（兼容旧客户端）；**生图接口无此前缀则直接报错** `error.invalid_param`（field: model）。

---

## 5. 生图 API（同步）

### 5.1 文生图

| 项 | 值 |
|----|-----|
| 客户端路径 | `POST {LLM根}/images/generations` |
| 服务端路径 | `POST /v1/images/generations` |
| Content-Type | `application/json` |

### 5.2 图片编辑

| 项 | 值 |
|----|-----|
| 客户端路径 | `POST {LLM根}/images/edits` |
| 服务端路径 | `POST /v1/images/edits` |

> 当前实现中，`generations` 与 `edits` 均代理至上游同一路径 `/services/aigc/multimodal-generation/generation`，区别仅在于路由与计费归类；请求体格式由上游渠道决定。

### 5.3 请求体

服务端行为：

1. 校验 `model` 前缀（`AIPC-` 或 `flowy/`），按 `tb_model.name` 匹配 **category=6** 的渠道模型。
2. 将请求体中的 `model` **替换**为渠道配置的 upstream model id（`tb_channel_model.model`）。
3. 原样转发 JSON 至 `{channel.base_url}/services/aigc/multimodal-generation/generation`。
4. 可选：调用前建议 `POST {LLM根}/chat/session` 上报 `sessionId`，用于调用记录归因（失败不阻塞生图）。

**最小示例（单元测试用例形态）：**

```json
{
  "model": "flowy/z-image-turbo",
  "prompt": "a cat"
}
```

实际字段以当前环境所接上游（如阿里云 DashScope 多模态生成）文档为准；服务端**不做** OpenAI DALL·E 字段到 DashScope 的自动转换，客户端应按上游要求组织 body（常见为 `input` / `parameters` 或渠道文档规定的结构）。

### 5.4 成功响应

- **HTTP 200**
- Body 为**上游原始 JSON**，**不**包装 `{ code, msg, data }`。
- 响应头由上游透传（`Content-Length`、`Content-Encoding` 等部分头会被过滤）。

计费：`usage.image_count` 存在时按 `image_count × 1_000_000` 作为 completion tokens 计价；否则回退 `completion_tokens` / `output_tokens`。

### 5.5 积分与限流

| 规则 | 说明 |
|------|------|
| 最低余额 | 若开启积分消费且配置了 `model_proxy.image_min_credit`（默认 **500**），可用积分低于该值则拒绝（402） |
| 扣费时机 | 上游返回 200 后按用量扣积分 |
| 限流 | 与其他 `/v1/*` 代理共用每用户 QPS（配置项 `model_proxy.user_qps`，默认 5） |
| 缓存 | 生图**不走**对话缓存 |

### 5.6 错误响应

失败时返回业务 JSON（与 LLM 代理一致）：

```json
{
  "code": 402,
  "msg": "积分不足"
}
```

| HTTP | code | errorKey | 说明 |
|------|------|----------|------|
| 400 | 400 | `error.invalid_param` | model 无效或无 `AIPC-` / `flowy/` 前缀 |
| 402 | 402 | `error.insufficient_credit` | 积分不足 |
| 429 | 429 | `error.rate_limited` | 触发限流 |
| 500 | 500 | `error.all_channel_models_failed` | 全部渠道失败 |
| 500 | 500 | `error.internal` | 内部错误 |

---

## 6. 生视频 API（Seedance 异步任务）

基于火山方舟（Ark）`POST /contents/generations/tasks` 的异步任务；Flowy 服务端负责鉴权、模型路由、落库、回调更新与扣费。

### 6.1 推荐对接流程

```
GET /model/availableListClaw?category=4   # 获取视频模型
    ↓
POST /video/generations/tasks             # 创建任务，得到本地 id
    ↓
轮询 GET /video/generations/tasks/:id     # 直至 status=4 或 5/6
    ↓
从 data.result.content.video_url 取成片 URL
```

服务端配置 `seedance.callback_url` 时，方舟回调会更新任务状态；**客户端仍应轮询**查询接口（仅读库，不每次打方舟）。

### 6.2 创建任务

| 项 | 值 |
|----|-----|
| 客户端路径 | `POST {业务根}/video/generations/tasks` |
| 需登录 | JWT 或 API Key |

**请求体**：与方舟「创建视频生成任务」JSON **一致**。服务端：

- 将 `model` 解析为 `model_category=4` 的渠道模型（建议 `flowy/<name>` 或与模型列表一致的 `AIPC-<name>`）。
- 将 `model` 替换为渠道 upstream model id 后转发至 `{base_url}/contents/generations/tasks`。
- 若配置了 `seedance.callback_url`，**覆盖**请求体中的 `callback_url`。

**文生视频最小示例：**

```json
{
  "model": "flowy/doubao-seedance-1-0-pro-250528",
  "content": [
    {
      "type": "text",
      "text": "写实风格，晴朗蓝天下的白色雏菊花田，镜头推近至一朵带露珠的特写"
    }
  ],
  "ratio": "16:9",
  "duration": 5,
  "watermark": false
}
```

**多模态 `content` 项类型**（有序数组）：

| type | 说明 | role（可选） |
|------|------|--------------|
| `text` | 提示词 | — |
| `image_url` | 参考图 / 首尾帧 | `reference_image`、`first_frame`、`last_frame` |
| `video_url` | 参考视频 | `reference_video` |
| `audio_url` | 参考音频 | `reference_audio` |

**常用顶层字段**：`generate_audio`、`ratio`、`duration`、`watermark`、`seed`、`resolution`、`service_tier`、`negative_prompt` 等（透传方舟）。

**Success Response**

```json
{
  "code": 200,
  "msg": "操作成功",
  "data": {
    "id": 12345
  }
}
```

| 字段 | 说明 |
|------|------|
| `data.id` | 本地 `tb_video_task.id`（**后续查询/取消均用此 id**） |

**创建前积分预检**：若请求体含 `duration`（或 `length` 等别名字段），且开启积分消费，则要求可用积分 ≥ `duration（秒，向上取整）× 1000`。

### 6.3 查询任务

| 项 | 值 |
|----|-----|
| 客户端路径 | `GET {业务根}/video/generations/tasks/:id` |
| 路径参数 `id` | 本地 `tb_video_task.id`（创建返回的 `data.id`） |

**Success Response**

```json
{
  "code": 200,
  "msg": "操作成功",
  "data": {
    "id": 12345,
    "task_id": "cgt-20260413143821-pkkmj",
    "status": 4,
    "result": {
      "id": "cgt-20260413143821-pkkmj",
      "model": "doubao-seedance-2-0-fast-260128",
      "status": "succeeded",
      "content": {
        "video_url": "https://example.com/video.mp4"
      },
      "duration": 10,
      "ratio": "16:9",
      "resolution": "720p",
      "usage": {
        "completion_tokens": 216900,
        "total_tokens": 216900
      },
      "created_at": 1776062301,
      "updated_at": 1776062458
    },
    "created_at": "2026-04-13T06:38:21Z",
    "updated_at": "2026-04-13T06:40:58Z"
  }
}
```

| 字段 | 说明 |
|------|------|
| `task_id` | 方舟侧任务 ID |
| `status` | 本地状态码（见下表） |
| `result` | 上游任务 JSON 快照（已剥离内部 `_flowy` 元数据） |

**本地 `status` 枚举**

| 值 | 含义 | 对应上游 status 示例 |
|----|------|----------------------|
| `1` | 排队 | `queued`、`pending` |
| `2` | 生成中 | `running`、`processing` |
| `3` | 已取消 | `cancelled` |
| `4` | 成功 | `succeeded`、`completed` |
| `5` | 失败 | `failed` |
| `6` | 过期 | `expired` |

**轮询建议**：任务提交后每 3–10 秒 GET 一次，直到 `status` 为终态（3/4/5/6）。成片地址：`data.result.content.video_url`。

### 6.4 任务列表

| 项 | 值 |
|----|-----|
| 客户端路径 | `GET {业务根}/video/generations/tasks` |
| Query | `page`（默认 1）、`pageSize`（默认 10，最大 200） |

**Success Response**

```json
{
  "code": 200,
  "msg": "操作成功",
  "data": {
    "list": [ /* 元素结构同单条查询 data */ ],
    "total": 42
  }
}
```

按本地 `id` 倒序，仅返回当前用户任务。

### 6.5 取消 / 删除任务

| 项 | 值 |
|----|-----|
| 客户端路径 | `DELETE {业务根}/video/generations/tasks/:id` |
| 路径参数 `id` | 本地 `tb_video_task.id` |

- 服务端调用方舟 `DELETE .../contents/generations/tasks/{task_id}`，并将本地状态置为 `3`（已取消）。
- 已取消任务再次删除：**幂等成功**（200）。

| HTTP | code | errorKey | 说明 |
|------|------|----------|------|
| 404 | 404 | `error.video_task_not_found` | 任务不存在或非本人 |
| 409 | 409 | `error.video_task.delete_running` | 任务仍在生成，上游拒绝删除 |
| 409 | 409 | `error.video_task.delete_conflict` | 上游冲突 |
| 502 | 502 | `error.seedance_upstream_failed` | 上游调用失败 |

### 6.6 方舟回调（运维 / 服务端）

| 项 | 值 |
|----|-----|
| 路径 | `POST /api/v1/video/generations/callback` |
| 认证 | **无需登录**（由方舟调用） |
| 响应 | HTTP **200**（无业务 JSON 要求） |

客户端**无需**对接；配置项 `seedance.callback_url` 指向该路径完整 URL。

### 6.7 视频扣费

| 阶段 | 行为 |
|------|------|
| 创建前 | `duration` 有效时预检：可用积分 ≥ 秒数 × 1000 |
| 回调成功 | 按 callback 中 `usage`（`prompt_tokens`/`completion_tokens`/`total_tokens` 等）扣费；无 token 时按渠道单价回退规则计价 |

---

## 7. 辅助接口（可选）

| 功能 | 客户端路径 | 说明 |
|------|-----------|------|
| 积分余额 | `GET {业务根}/credits/balance` | 生图/生视频前检查 |
| 调用记录 | `GET {业务根}/model/calls` | 含 `credit_consumed` |
| Session 上报 | `POST {LLM根}/chat/session` | 生图调用记录归因 |

---

## 8. 对接示例（TypeScript）

```typescript
const BUSINESS_BASE = 'https://server.flowyaipc.cn/claw';
const LLM_BASE = 'https://server.flowyaipc.cn/claw/v1';

function headers(token: string) {
  return {
    'Content-Type': 'application/json',
    Authorization: `Bearer ${token}`,
    token,
  };
}

// 获取图片模型
async function listImageModels(token: string) {
  const res = await fetch(`${BUSINESS_BASE}/model/availableListClaw?category=6`, {
    headers: headers(token),
  });
  const json = await res.json();
  if (json.code !== 200) throw new Error(json.msg);
  return json.data.cloud as Array<{ id: string; name: string }>;
}

// 文生图（响应为上游 JSON，非包装格式）
async function generateImage(token: string, model: string, body: Record<string, unknown>) {
  const res = await fetch(`${LLM_BASE}/images/generations`, {
    method: 'POST',
    headers: headers(token),
    body: JSON.stringify({ model, ...body }),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.msg ?? `HTTP ${res.status}`);
  }
  return res.json();
}

// 创建视频任务并轮询
async function generateVideo(token: string, createBody: Record<string, unknown>) {
  const createRes = await fetch(`${BUSINESS_BASE}/video/generations/tasks`, {
    method: 'POST',
    headers: headers(token),
    body: JSON.stringify(createBody),
  });
  const created = await createRes.json();
  if (created.code !== 200) throw new Error(created.msg);

  const localId = created.data.id as number;
  for (;;) {
    await new Promise((r) => setTimeout(r, 5000));
    const pollRes = await fetch(`${BUSINESS_BASE}/video/generations/tasks/${localId}`, {
      headers: headers(token),
    });
    const polled = await pollRes.json();
    if (polled.code !== 200) throw new Error(polled.msg);

    const { status, result } = polled.data;
    if (status === 4) return result?.content?.video_url as string;
    if (status === 5 || status === 6) throw new Error('Video generation failed or expired');
    if (status === 3) throw new Error('Video task cancelled');
  }
}
```

---

## 9. 接口速查

| 功能 | Method | 客户端路径 | Base | JWT | API Key |
|------|--------|-----------|------|-----|---------|
| 图片模型列表 | GET | `/model/availableListClaw?category=6` | 业务 | ✅ | ❌ |
| 视频模型列表 | GET | `/model/availableListClaw?category=4` | 业务 | ✅ | ❌ |
| 文生图 | POST | `/images/generations` | LLM `/claw/v1` | ✅ | ✅ |
| 图片编辑 | POST | `/images/edits` | LLM `/claw/v1` | ✅ | ✅ |
| 创建视频任务 | POST | `/video/generations/tasks` | 业务 | ✅ | ✅ |
| 视频任务列表 | GET | `/video/generations/tasks` | 业务 | ✅ | ✅ |
| 查询视频任务 | GET | `/video/generations/tasks/:id` | 业务 | ✅ | ✅ |
| 取消视频任务 | DELETE | `/video/generations/tasks/:id` | 业务 | ✅ | ✅ |

---

## 10. 源码索引

| 模块 | 路径 |
|------|------|
| 路由注册 | `internal/routes/routes.go` |
| 生图 Handler | `internal/handlers/model.go` → `ImagesGenerations` / `ImagesEdits` |
| 生图代理 | `internal/services/model_proxy/proxy.go` |
| 视频 Handler | `internal/handlers/seedance_video.go` |
| 视频服务 | `internal/services/model_proxy/seedance_video.go` |
| 模型分类常量 | `internal/constants/constants.go` |
| 模型前缀 | `internal/constants/prefix.go` |
| 客户端视频契约参考 | `electron/workbench/video/provider/flowy-seedance-contract.ts` |
| 方舟请求示例 | `electron/workbench/video/provider/ark-create-request-examples.md` |
| 服务端 API 总览 | `../API.md` |

---

## 11. 对接检查清单

- [ ] 完成登录，取得 JWT 或创建 `flowy-` API Key
- [ ] 区分业务 Base（`/claw`）与 LLM Base（`/claw/v1`）
- [ ] 生图：解析**上游直透 JSON**；错误时解析 `{ code, msg }`
- [ ] 生图：`model` 使用 `AIPC-...` 或 `flowy/...` 前缀
- [ ] 生视频：创建返回的 `data.id` 作为轮询与删除的路径 id
- [ ] 生视频：请求体对齐方舟 `content` + 顶层控制字段
- [ ] 积分：生图注意 `image_min_credit`；生视频注意 `duration × 1000` 预检
- [ ] 参考资源 URL 建议使用 **HTTPS**（部分上游要求）
