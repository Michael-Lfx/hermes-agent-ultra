# SOP: `model_metadata`

| 字段 | 值 |
|------|-----|
| registry `id` | `model_metadata` |
| Python | `research/hermes-agent/agent/model_metadata.py` |
| Rust | `crates/hermes-intelligence/src/model_metadata.rs` |
| Crate | `hermes-intelligence` |
| Fixtures | `crates/hermes-parity-tests/fixtures/model_metadata/*.json` |

## 验证

```bash
cargo build -p hermes-intelligence
cargo test -p hermes-parity-tests model_metadata
cargo clippy -p hermes-intelligence -- -D warnings
```

## 提交

```
parity(model_metadata): port from python v2026.4.13
```
