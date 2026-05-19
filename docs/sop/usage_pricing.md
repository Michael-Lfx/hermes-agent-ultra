# SOP: `usage_pricing`

| 字段 | 值 |
|------|-----|
| registry `id` | `usage_pricing` |
| Python | `research/hermes-agent/agent/usage_pricing.py` |
| Rust | `crates/hermes-intelligence/src/usage_pricing.rs` |
| Crate | `hermes-intelligence` |
| Fixtures | `crates/hermes-parity-tests/fixtures/usage_pricing/*.json` |

## 验证

```bash
cargo build -p hermes-intelligence
cargo test -p hermes-parity-tests usage_pricing
cargo clippy -p hermes-intelligence -- -D warnings
```

## 提交

```
parity(usage_pricing): port from python v2026.4.13
```
