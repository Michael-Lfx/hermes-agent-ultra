# SOP: `send_message`

| 字段 | 值 |
|------|-----|
| registry `id` | `send_message` |
| Python | `research/hermes-agent/tools/send_message_tool.py` |
| Rust | `hermes_tools::extract_media`, `hermes_config::{resolve_agent_path, resolve_outbound_media_path}`, `hermes_gateway::tool_backends::GatewayMessagingBackend` |
| Fixtures | `crates/hermes-parity-tests/fixtures/send_message/*.json` |

## 验证

```bash
cargo build -p hermes-tools -p hermes-gateway -p hermes-cli
cargo test -p hermes-parity-tests send_message
cargo test -p hermes-parity-tests
```

## Windows 路径

- `/tmp/...` → `{HERMES_HOME}/cache/terminal/...` via `resolve_agent_path`
- Live gateway: `send_message` `file` → `Gateway::send_file`
- Message body: `MEDIA:/path/to/file.ext` via `extract_media`

## 提交

```
parity(send_message): Windows /tmp mapping, send_file wiring, MEDIA extract
```
