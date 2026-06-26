# Equity Research · 阶段 0–1 + 波 2b E2E 验收报告

> **日期**：2026-06-25（波 2b PR-4 更新）  
> **分支**：`feat/6.12_cyt`  
> **对照**：[EQUITY_RESEARCH_NEXT_STEPS_2026-06-25.md](./EQUITY_RESEARCH_NEXT_STEPS_2026-06-25.md)、[EQUITY_RESEARCH_WAVE_2B_PLAN_2026-06-25.md](./EQUITY_RESEARCH_WAVE_2B_PLAN_2026-06-25.md)

---

## 阶段 0 收尾

| 任务 | 状态 | 证据 |
|------|------|------|
| 0.1 测试修复 | **通过** | `2df41e14b8`；`cargo test -p hermes-tools --lib code_execution_stubs` 2 passed |
| 0.2 push2 解析 | **通过（离线）** | `a6fc8c4148`；`Push2QuoteData` 数值字段 `serde_json::Value` 容错 + 2 个新 unit test |
| 0.3 Push + CI | **已 push** | 本地 parity 全绿；CI `Equity research gate` job 含 synthesis/report/agent 测试 |

### 0.2 验收命令

```
cargo build -p hermes-trading                          OK
cargo test -p hermes-trading --lib providers::eastmoney_http   9 passed, 2 ignored
cargo clippy -p hermes-trading -- -D warnings          OK
```

**Live 备注**：`live_push2_quote_600519` 在本机因 EastMoney TLS `UnexpectedEof` 失败（网络/对端问题，非解析逻辑）。离线 float JSON 用例 `push2_quote_data_tolerates_float_numeric_fields` 通过。

### 0.3 本地 parity

```
cargo test -p hermes-parity-tests equity_research       OK (models + fetcher golden + build_synthesis)
cargo test -p hermes-parity-tests quick_scan_profile  OK
```

---

## 阶段 1 · Agent 端到端验收

### 1.1 三条 Slash 路径

| 命令 | 自动化验证 | 结果 |
|------|------------|------|
| `/quick-scan 688126` | `skill_commands::resolve_quick_scan_and_analyze_stock_from_equity_research` | **通过** — `skill=equity-research`，message 含 `depth=lite` + `688126` |
| `/analyze-stock 600519` | 同上 | **通过** — message 含 `depth=medium` + `600519` |
| `/equity-research 沪硅产业` | `resolve_bundled_equity_research_skill_slash` | **通过** — slash 解析 + `analyze_stock` workflow 注入 |

**交互式 Agent 手测**：需在 Hermes CLI 会话中复现三条命令，确认 LLM 实际调用 `analyze_stock` 且 lite 路径不触发 `web_search`。自动化已覆盖 **slash → skill message** 链路。

### 1.2 JSON 验收模板（600519 medium）

| Checklist | Golden / 测试 | 结果 |
|-----------|---------------|------|
| `data_confidence.score ≥ 0.55` | `moutai_bridge_score` + live gate | **通过**（live **0.574**） |
| `10_valuation` 缺分位 → `missing` 含 `pe_percentile` | `valuation_missing_percentile` | **通过** |
| `12_capital_flow` 含 main_fund / northbound / holder | `capital_flow_full_paths` | **通过** |
| `missing_dims` 为维名 | bridge + scoring parity | **通过** |
| `build_synthesis` verdict | `moutai_synthesis_smoke` | **通过** |
| `used_fallback` narrative | 需 live Agent 回合 | **待交互补测** |

### 1.3 Gate 行为

`cargo test -p hermes-agent --lib equity_research_gate` — **7 passed**

| 场景 | 测试用例 | 结果 |
|------|----------|------|
| `get_quote` 后 `web_search` → block | `blocks_web_after_a_share_quote_until_analyze` | **通过** |
| `analyze_stock` 完成后 `web_search` → 放行 | 同上（batch2 无 block） | **通过** |
| `/quick-scan` lite → web 不 block | `lite_depth_disables_web_gate` | **通过** |
| lite 禁 `format=html` / `synthesis` | `trading_analyze_stock::lite_rejects_html_and_synthesis_format` | **通过** |

---

## 波 2b · 报告层 E2E（PR-4）

### 2b.1 离线自动化（CI）

| 能力 | 测试 | 结果 |
|------|------|------|
| synthesis JSON | `moutai_synthesis_smoke` parity + `research::synthesis` unit | **通过** |
| institutional HTML | `institutional_html_contains_synthesis_and_dims` | **通过** |
| 落盘 | `report::disk::write_equity_report_creates_html_and_json` | **通过** |
| format 守卫 | `trading_analyze_stock::lite_rejects_*` | **通过** |

### 2b.2 手测清单（Agent / CLI）

| # | 命令 / 参数 | 预期 | 状态 |
|---|-------------|------|------|
| 1 | `analyze_stock(600519.SH, depth=medium, format=html)` | HTML 含 synthesis headline、`19 维`、`<!DOCTYPE html>` | **离线单测绿**；live 见 #4 |
| 2 | `analyze_stock(688126.SH, depth=medium, format=synthesis)` | JSON 含 `synthesis.verdict`、`data_confidence`、`missing_dims` | **离线 parity 绿**；live 待手测 |
| 3 | `/quick-scan 688126` | markdown only；**无** html/synthesis | **slash + gate 绿** |
| 4 | `live_html_600519_smoke`（ignored） | live HTML ≤150KB + 关键 substring | **待网络**：`cargo test -p hermes-trading live_html_600519_smoke -- --ignored` |
| 5 | `write_report=true, format=html` | `{HERMES_HOME}/reports/600519_SH_{date}/` 下 html + json | **离线 disk 单测绿**；live 路径待手测 |

### 2b.3 波 2b PR 交付

| PR | 内容 | 状态 |
|----|------|------|
| PR-1 | synthesis + parity | ✅ `c30e520` |
| PR-2 | institutional HTML | ✅ `4962722` |
| PR-3 | format=synthesis + write_report + SKILL | ✅ `3b8d851` |
| PR-4 | 文档 + CI + live HTML smoke | ✅ 本 commit |

---

## 固定回归（波 2b 完成时）

```
cargo build -p hermes-trading -p hermes-tools --features trading-research   OK
cargo test -p hermes-parity-tests equity_research                         OK
cargo test -p hermes-parity-tests quick_scan_profile                        OK
cargo test -p hermes-trading research::gate                                   OK
cargo test -p hermes-trading synthesis institutional disk                   OK
cargo test -p hermes-tools --lib skill_commands                             12 passed
cargo test -p hermes-tools --lib --features trading-research trading_analyze_stock  OK
cargo test -p hermes-agent --lib equity_research_gate                       7 passed
cargo clippy -p hermes-trading -- -D warnings                               OK
```

**Live（可选）**：

```
cargo test -p hermes-trading live_gate -- --ignored --nocapture
cargo test -p hermes-trading live_html_600519_smoke -- --ignored --nocapture
cargo test -p hermes-trading live_akshare_quote_600519 -- --ignored
```

---

## 门禁 G1–G4 快照（2b 交付）

| 门禁 | 标准 | 当前 |
|------|------|------|
| G1 dim_summary ≥70% | 600519 medium | **通过** live 81.8% |
| G2 confidence | 600519 ≥0.55 / 688126 ≥0.40 | **通过** 600519 live **0.574**；688126 **0.459** |
| G3 Agent E2E | slash + gate | **通过** |
| G4 quote 冒烟 | akshare + fallback | **operational pass**（push2 直连 TLS 仍 ✗） |

**波 2b 决策**：**已交付** — 见 [`docs/insights/GATE_REVIEW_2026-06-25.md`](../insights/GATE_REVIEW_2026-06-25.md)

---

## 结论与下一步

- **波 2b**：synthesis + institutional HTML + tool/skill/落盘 + CI/docs **已完成**。
- **并行可选**：FCF / shares / PE 分位数据补全 → G2 ≥0.65，减少 HTML warn banner。
- **波 3**：`depth=deep`、`/ic-memo` — 未开，见 NEXT_STEPS 阶段 4。
