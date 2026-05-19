# SOP: `v4a_patch`

| 字段 | 值 |
|------|-----|
| registry `id` | `v4a_patch` |
| Python | `research/hermes-agent/tools/patch_parser.py` |
| Rust | `crates/hermes-tools/src/v4a_patch.rs` |
| Crate | `hermes-tools` |
| Fixtures | `crates/hermes-parity-tests/fixtures/v4a_patch/*.json` |

## 验证

```bash
cargo build -p hermes-tools
cargo test -p hermes-parity-tests v4a_patch
cargo clippy -p hermes-tools -- -D warnings
```

## 提交

```
parity(v4a_patch): port from python v2026.4.13
```
