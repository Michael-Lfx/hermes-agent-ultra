# SOP: `error_classifier`

| 字段 | 值 |
|------|-----|
| registry `id` | `error_classifier` |
| Python | `research/hermes-agent/agent/error_classifier.py` |
| Rust | `crates/hermes-intelligence/src/error_classifier.rs` |
| Crate | `hermes-intelligence` |
| Fixtures | `crates/hermes-parity-tests/fixtures/error_classifier/*.json` |

## 验证

```bash
cargo build -p hermes-intelligence
cargo test -p hermes-parity-tests error_classifier
cargo clippy -p hermes-intelligence -- -D warnings
```

## 提交

```
parity(error_classifier): port from python v2026.4.13
```
