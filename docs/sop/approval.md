# SOP: `approval`

| 字段 | 值 |
|------|-----|
| registry `id` | `approval` |
| Python | `research/hermes-agent/tools/approval.py` |
| Rust | `crates/hermes-tools/src/approval.rs` |
| Crate | `hermes-tools` |
| Fixtures | `crates/hermes-parity-tests/fixtures/approval/*.json` |

## 验证

```bash
cargo build -p hermes-tools
cargo test -p hermes-parity-tests approval
cargo clippy -p hermes-tools -- -D warnings
```

## 提交

```
parity(approval): port from python v2026.4.13
```
