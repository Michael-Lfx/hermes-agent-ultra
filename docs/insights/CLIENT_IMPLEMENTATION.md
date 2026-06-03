# Insights Contribution — 客户端实现说明

> **服务端规格**：见 [SERVER_IMPLEMENTATION.md](./SERVER_IMPLEMENTATION.md)（ingest REST 已上线，**Bearer 鉴权必填**）  
> **可读 Payload v2（运营）**：见 [SERVER_READABLE_PAYLOAD.md](./SERVER_READABLE_PAYLOAD.md) — POI/Skill **脱敏明文**，非 `interest:<hex>`  
> **运营平台 UI**：见 [OPS_UI.md](./OPS_UI.md)  
> **本仓库客户端**：`crates/hermes-insights` + CLI + Agent 钩子

---

## 实现状态（已完成）

- [x] `hermes-insights` crate：脱敏、outbox、REST client
- [x] `hermes-config`：`insights` / `interest` 配置块
- [x] `hermes config set/get` 支持 `interest.*`、`insights.contribution.*`
- [x] 环境变量：`HERMES_INSIGHTS_ENDPOINT`、`HERMES_INSIGHTS_TOKEN`
- [x] Agent：session-end 入队 + skill 变更 debounce + background review 成熟度
- [x] CLI：`hermes interest enable|disable`、`hermes contribute *`
- [x] Golden tests：`crates/hermes-insights/tests/`

---

## POI 与个性化 Skills 是否必须一起上传？

**不必须。** 二者是同一 REST batch 里的**两种独立 contribution 类型**，由不同开关控制：

| 开关 | 配置 | CLI |
|------|------|-----|
| POI 脱敏上传 | `insights.contribution.upload_interests` | `hermes contribute enable --poi-only` |
| Skills 模式上传 | `insights.contribution.upload_skills` | `hermes contribute enable --skills-only` |
| 总开关 | `insights.contribution.enabled` | `hermes contribute enable` |

**推荐（运营价值最大）**：两类都开启。Session 结束时客户端可在**同一 batch** 中发送 `interest_snapshot` + 若干 `skill_pattern`；`skill_pattern.linked_interest_keys` 携带当次 top POI，便于服务端做「兴趣 × 工作流」关联分析。

**可单独开启的场景**：

- 只要行业兴趣 cohort → 仅 `--poi-only`
- 只要标准化 skill 聚类 → 仅 `--skills-only`
- 本地 POI 学习但不上传 → `hermes contribute disable`，保留 `interest.enabled`

本地 POI 抽取（`interest.enabled`）与上传无关，由 `hermes interest enable|disable` 控制。

**POI 管道（生产默认）**：Extract → Compare → Update（`crates/hermes-agent/src/user_interest/pipeline.rs`）

- 每轮用户消息：仅写入**会话内存 buffer**（`per_turn_buffer: true`），不落库
- Session 结束：规则抽取 + 可选 auxiliary LLM → 与已有 topic 比对（合并/强化/晋升）→ 一次写入 `interest.db`
- `keyword:` / `path:` 类信号**不持久化**（与 `hermes-insights` 上传门控一致）
- 高质量模式：`hermes interest enable --mode hybrid --llm-on-session-end`

---

## 上传内容（可读 v2）

运营侧 **无法** 解析客户端本地 id，因此上传的是 **脱敏后的明文**：

| 类型 | 上传什么 | 不上传什么 |
|------|----------|------------|
| **POI** | `label_redacted`、`summary_redacted`；`topic_key` 为 `lang:rust` / `topic:<slug>` | `interest:0062d40f…` 等本地 hash id |
| **co_topics** | 脱敏 label 列表 | topic id |
| **Skill** | `display_name`、`description_redacted`、默认 **`redacted_body`**（去 References） | 仅 `pattern_id` 作展示 |

`pattern_id` / `content_hash` 仍用于服务端去重。  
关闭 skill 正文：`hermes config set insights.contribution.redacted_body false`（默认 **true**）。

服务端同步见 [SERVER_READABLE_PAYLOAD.md](./SERVER_READABLE_PAYLOAD.md)。

---

## 鉴权（必填）

服务端所有 Insights 客户端接口要求：

```http
Authorization: Bearer <用户 JWT 或 flowy- API Key>
X-Installation-Id: <客户端安装 UUID，自动生成>
```

Hermes 在 `POST /v1/insights/batch` 与 `DELETE /v1/installations/{id}` 时自动附带上述 Header（见 `crates/hermes-insights/src/client.rs`）。

| 配置方式 | 优先级 | 说明 |
|----------|--------|------|
| 环境变量 `HERMES_INSIGHTS_TOKEN` | 最高 | 适合 CI / 临时联调 |
| `insights.contribution.auth_token` | 次之 | **推荐**；JWT 可先写死在 `config.yaml` |
| `insights.contribution.installation_token` | 同上 | 历史 yaml 字段名，读取时映射到 `auth_token` |

`upload_ready` = `enabled` + `endpoint` + **已配置 Bearer**。缺 token 时 `flush` 不会上传，outbox 保留。

---

## 配置（无需手改 yaml，也可用 yaml）

### 推荐：CLI + config set

```bash
# 服务端 URL
hermes config set insights.contribution.endpoint https://ops.example.com/v1/insights/batch

# Bearer：用户 JWT 或 flowy- API Key（可先写死在 config，见下方 yaml）
hermes config set insights.contribution.auth_token "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...."

# 或环境变量（覆盖 yaml，适合 CI）
export HERMES_INSIGHTS_ENDPOINT=https://ops.example.com/v1/insights/batch
export HERMES_INSIGHTS_TOKEN="flowy-sk-xxxx"

# 开启上传并确认就绪
hermes contribute enable
hermes contribute status   # 应看到 Upload ready: true

# 仅 POI / 仅 Skills
hermes contribute enable --poi-only
hermes contribute enable --skills-only

# 本地 POI 学习（与上传无关）
hermes interest enable
```

### 可选：直接编辑 config.yaml

```yaml
interest:
  enabled: true

insights:
  contribution:
    enabled: true
    endpoint: "https://ops.example.com/v1/insights/batch"
    upload_interests: true
    upload_skills: true
    on_session_end: true
    skill_min_age_hours: 24
    redacted_body: false
    # 用户 JWT 或 flowy- API Key（可先写死；勿提交到公开仓库）
    auth_token: "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...."
```

`config.yaml` 来源：`hermes setup` 写入、各 CLI 子命令 `save_config_yaml`、或不存在时使用代码默认值。

写入后 yaml 里键名为 **`auth_token`**（不是 `installation_token`）。若 `config set` 报 unknown key，请先 `cargo build -p hermes-cli` 使用含 Insights 改动的二进制。

---

## CLI 速查

| 命令 | 作用 |
|------|------|
| `hermes interest enable` / `disable` | 本地 POI 抽取 |
| `hermes contribute status` | 贡献与 outbox 状态 |
| `hermes contribute enable` / `disable` | 上传总开关 |
| `hermes contribute enable --poi-only` | 仅 POI |
| `hermes contribute enable --skills-only` | 仅 Skills |
| `hermes contribute preview` | 预览脱敏 JSON |
| `hermes contribute flush` | 上传 outbox |
| `hermes contribute reset` | 将 `sent`/`failed` 改回 `pending`（本地重传测试） |
| `hermes contribute reset --clear` | 清空 outbox 全部行 |
| `hermes contribute revoke` | DELETE installation |

> `hermes insights` = Usage 统计（token/会话），与贡献管道无关。

---

## `hermes config` 支持的键

- `interest.enabled`、`interest.extract_mode`、`interest.max_topics`、`interest.llm_on_session_end`
- `insights.contribution.enabled`
- `insights.contribution.endpoint`
- `insights.contribution.upload_interests` / `upload_skills`
- `insights.contribution.on_session_end`
- `insights.contribution.skill_min_age_hours`
- `insights.contribution.redacted_body`
- `insights.contribution.auth_token` / `installation_token`（Bearer，二选一字段名）

---

## 验证

```bash
cargo test -p hermes-insights
cargo build -p hermes-cli
hermes contribute status
hermes contribute preview
```
