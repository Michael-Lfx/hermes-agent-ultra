# Parity fixtures — machine run log（相对 `PYTHON_BASELINE.txt`）

本文件由 **一次实际 `cargo test` + 对 `fixtures/**/*.json` 的脚本统计** 生成，用于代替主观「对齐」表述。  
**基线**：见同目录 [`PYTHON_BASELINE.txt`](./PYTHON_BASELINE.txt)（Python **v0.11.0** / tag **v2026.4.23**）。

---

## 最近一次执行

| 字段 | 值 |
|------|-----|
| **命令** | `cargo test -p hermes-parity-tests` |
| **完成时间（UTC）** | 2026-04-26T15:14:06Z（以 CI/本机终端记录为准） |
| **结果** | **9 passed, 0 failed, 0 ignored**（lib tests） |
| **Doc-tests** | 0 |

### 各测试目标（Rust）

| 测试名 | 含义 |
|--------|------|
| `parity_anthropic_adapter_fixtures` | `fixtures/anthropic_adapter/*.json` |
| `parity_hermes_core_fixtures` | `fixtures/hermes_core/*.json` |
| `parity_model_metadata_fixtures` | `fixtures/model_metadata/*.json` |
| `parity_usage_pricing_fixtures` | `fixtures/usage_pricing/*.json` |
| `parity_approval_fixtures` | `fixtures/approval/*.json` |
| `parity_v4a_patch_fixtures` | `fixtures/v4a_patch/*.json` |
| `parity_error_classifier_fixtures` | `fixtures/error_classifier/*.json` |
| `parity_all_active_fixtures_recursive` | 除 `pending/`、`registry.json` 外全部 `*.json` |
| `checkpoint_shadow_dir_id_matches_python_sample` | 与 Python `sha256` 前缀样本一致 |

---

## Fixture 文件级：`skip: true` 与执行条数

**说明**：`harness` 对每个 case 若 `skip: true` 则不计入 golden 比对。下表为 **非 `pending/`、非 `registry.json`** 的 JSON 统计。

| 文件（相对 `fixtures/`） | cases 总数 | `skip: true` | 实际执行（比对 expected） |
|---------------------------|------------|--------------|---------------------------|
| `anthropic_adapter/model_tools.json` | 6 | 0 | 6 |
| `anthropic_adapter/oauth_betas.json` | 4 | 0 | 4 |
| `approval/command_safety.json` | 14 | 0 | 14 |
| `checkpoint_manager/shadow_dir_hash.json` | 1 | 0 | 1 |
| `error_classifier/classify_errors.json` | 6 | 0 | 6 |
| `hermes_core/format_tool_calls.json` | 2 | 0 | 2 |
| `model_metadata/context_and_capabilities.json` | 13 | 0 | 13 |
| `usage_pricing/billing_routes.json` | 4 | 0 | 4 |
| `v4a_patch/parse_patch.json` | 5 | 0 | 5 |
| **合计** | **55** | **0** | **55** |

`fixtures/pending/` 整树被 `run_all_active_fixtures` **排除**，不在上表；若未来为某条 Python 行为增加占位 JSON，应显式标 `skip: true` 或放在 `pending/`。

---

## 本 crate **不覆盖**的范围（勿与上表混淆）

- **Python `web_server.py` / Dashboard HTTP**：由 `hermes-server` 单测、smoke、`deploy/PARITY_MODULE_C.md` 等跟踪；**不在** `hermes-parity-tests` golden 树内。
- **OAuth 真实授权 / 换票**：当前 Dashboard 为 stub；无对应 parity JSON。
- **Bedrock 全链路**：见 `deploy/PARITY_MODULE_C.md` C1。

---

## 如何刷新本文件

1. 在仓库根执行：`cargo test -p hermes-parity-tests`
2. 用与本次相同的统计脚本（见 `fixtures/README.md` 或本文件 git 历史中的 `python3` 片段）重新生成上表日期与数字后提交。
