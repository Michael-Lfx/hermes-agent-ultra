# SOP: `run_conversation`

| 字段 | 值 |
|------|-----|
| registry `id` | `run_conversation` |
| Python @ 1335ce | `run_agent.py::AIAgent.run_conversation` |
| Rust | `hermes_agent::conversation_loop` + `AgentLoop::run_prepared` / `run_stream_prepared` |
| Crate | `hermes-agent` |
| Contract tests | `crates/hermes-agent/tests/run_conversation_*.rs` |
| Fixtures | 可选 `fixtures/conversation_loop/`（不阻塞 merge） |

## 语义对齐原则

- **Adopt**：Python 用户/插件可感知行为更完整 → Rust 同等语义（见下表）。
- **Document**：Rust 结构或等价路径更优 → 登记 `docs/parity/intentional-divergence.json`，不字面 port。

## 必须语义对齐（Adopt）

| 行为 | Rust 位置 | 验证 |
|------|-----------|------|
| Turn 前奏：sanitize、@file、restore primary | `prepare_turn` / `apply_turn_message_prelude` | `cargo test -p hermes-agent preprocess_user_message` / `restore_primary` |
| Hooks：`on_session_end`、`pre_api_request` | `session_end_hooks` / `invoke_pre_api_request_hook` | `cargo test -p hermes-agent --test run_conversation_hooks` |
| Steer pre-API drain | `steer.rs` | `cargo test -p hermes-agent steer` |
| Truncated tool-call retry | `agent_loop.rs` | `run_truncated_tool_call_retries` |
| 编排 API | `run_conversation` | `cargo test -p hermes-agent --test run_conversation_contracts` |
| Gateway/HTTP 主路径 | `hermes-cli/main.rs`, `hermes-http` | 集成；`task_id` = `session_key` |

## Rust 优势（Document — 非欠账）

| 项 | 收益 |
|----|------|
| `conversation_loop` + `run_prepared` | 可测 B/E；`skip_message_prelude` 避免双重 prelude |
| Gateway 在 cli/http 装配 | 平台 crate 与 loop 解耦 |
| 无 Python runtime vendoring | 单二进制、`cargo test` 主门禁 |

完整 divergence id 见 `intentional-divergence.json` 中 `run-conversation-*` 条目。

## 验证（每 PR）

```bash
cargo build -p hermes-agent
cargo test -p hermes-agent --test run_conversation_hooks --test run_conversation_contracts
cargo clippy -p hermes-agent -- -D warnings
```

## 参考

- [`crates/hermes-agent/src/conversation_loop.rs`](../../crates/hermes-agent/src/conversation_loop.rs)
- [`crates/hermes-agent/src/python_alignment.rs`](../../crates/hermes-agent/src/python_alignment.rs) Phase A 列表
