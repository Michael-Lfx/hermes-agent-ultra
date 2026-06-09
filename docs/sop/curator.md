# SOP: `curator`

| 字段 | 值 |
|------|-----|
| registry `id` | _未录入（本次 CLI/TUI 命令首次落地）_ |
| Python | `hermes_cli/curator.py`（CLI 入口）、`agent/curator.py`（后台引擎） |
| Rust CLI handler | `crates/hermes-cli/src/commands.rs` → `handle_curator_command()` |
| Rust 后台引擎 | `crates/hermes-skills/src/usage.rs`（使用追踪 + 归档/恢复/pin） |
| Crate | `hermes-cli` (命令处理器), `hermes-skills` (底层函数) |
| Fixtures | _无（尚未纳入 parity 测试）_ |

## 架构概览

### 数据流

```
SLASH_COMMANDS 注册
    │  "/curator" — "Skill curator/control-plane compatibility surface"
    ▼
canonical_command("/curator") → "/curator"  (独立命令，不再 alias 到 /skills)
    │
    ▼
handle_slash_command() match
    │  "/curator" => handle_curator_command(app, args).await
    ▼
handle_curator_command()
    │  解析 args[0] 作为子命令
    │  获取 skills_dir = hermes_config::hermes_home().join("skills")
    ▼
    ├── "status" / ""  → hermes_skills::agent_created_report()
    ├── "pin"          → hermes_skills::set_pinned(dir, name, true)
    ├── "unpin"        → hermes_skills::set_pinned(dir, name, false)
    ├── "archive"      → hermes_skills::archive_skill(dir, name)
    ├── "restore"      → hermes_skills::restore_skill(dir, name)
    ├── "list-archived"→ std::fs::read_dir(dir.join(".archive"))
    ├── "run"          → ⚠️ 占位提示
    ├── "pause"        → ⚠️ 占位提示
    ├── "resume"       → ⚠️ 占位提示
    └── _              → 帮助文本
```

### 已实现 vs 占位

| 子命令 | 状态 | 调用路径 |
|--------|------|---------|
| (无参数) | ✅ 已实现 | `agent_created_report()` → 格式化输出，显示名称/状态/pin/活跃度 |
| `status` | ✅ 已实现 | 同上 |
| `pin <name>` | ✅ 已实现 | `set_pinned(dir, name, true)` → 一行调用 |
| `unpin <name>` | ✅ 已实现 | `set_pinned(dir, name, false)` → 一行调用 |
| `archive <name>` | ✅ 已实现 | `archive_skill(dir, name)` → `Result<(bool, String), SkillError>` |
| `restore <name>` | ✅ 已实现 | `restore_skill(dir, name)` → `Result<(bool, String), SkillError>` |
| `list-archived` | ✅ 已实现 | `fs::read_dir(dir.join(".archive"))` → 遍历子目录 |
| `run` | ⚠️ 占位 | 提示"后台引擎尚未移植" |
| `pause` | ⚠️ 占位 | 同上 |
| `resume` | ⚠️ 占位 | 同上 |

## 已调用的 `hermes_skills::usage` API

所有函数从 `crates/hermes-skills/src/usage.rs` 并通过 `lib.rs` 的 `pub use usage::*` 重新导出。

### `agent_created_report(skills_dir: &Path) -> Vec<SkillUsageReportRow>`

返回所有 `agent_created == true` 技能的报表行列表。

```rust
pub struct SkillUsageReportRow {
    pub name: String,              // 技能名称
    pub use_count: u64,            // 使用次数
    pub view_count: u64,           // 查看次数
    pub patch_count: u64,          // 补丁次数
    pub activity_count: u64,       // use + view + patch
    pub state: String,             // "active" | "stale" | "archived"
    pub pinned: bool,              // 是否被固定（豁免 curator 自动管理）
    pub archived_at: Option<String>, // 归档时间 (ISO 8601)
    pub last_activity_at: Option<String>, // 最后活动时间
}
```

### `set_pinned(skills_dir: &Path, skill_name: &str, pinned: bool) -> Result<(), SkillError>`

固定/取消固定技能。受保护技能（bundled、hub-installed）会被静默忽略。

### `archive_skill(skills_dir: &Path, skill_name: &str) -> Result<(bool, String), SkillError>`

将技能目录移动到 `.archive/` 子目录。返回 `(true, msg)` 表示成功，`(false, msg)` 表示被拒绝（如受保护技能）。

### `restore_skill(skills_dir: &Path, skill_name: &str) -> Result<(bool, String), SkillError>`

从 `.archive/` 恢复技能。会检查是否与已有技能冲突。状态重置为 `active`。

### `is_protected_skill(skills_dir: &Path, skill_name: &str) -> bool`

判断技能是否为受保护（bundled 或 hub-installed），受保护技能会被 `archive_skill` 等操作拒绝。

## `skills_dir` 路径解析

```
hermes_config::hermes_home().join("skills")
```

`hermes_home()` 优先级链（`crates/hermes-config/src/paths.rs`）：
1. 环境变量 `HERMES_HOME`
2. 环境变量 `HERMES_AGENT_ULTRA_HOME`
3. `~/.hermes-agent-ultra`（默认）
4. 向后兼容：若 `~/.hermes-agent-ultra` 不存在但 `~/.hermes` 存在 → 使用 `~/.hermes`

## 子命令路由实现细节

### 输出机制

使用 `emit_command_output(app, text)`（`commands.rs:4848`）统一输出：
- TUI 模式 → `app.push_ui_assistant(rendered)`（渲染到 TUI 面板）
- 非 TUI 模式 → `println!("{}", rendered)`（标准输出）

### 错误处理

- `set_pinned` 失败 → 显示 `Failed: {e}`
- `archive_skill` / `restore_skill` → 区分 `Ok((true, msg))`（✅ 成功）和 `Ok((false, msg))`（❌ 被拒绝）
- `list-archived` 的 `read_dir` 失败 → 显示 I/O 错误信息
- 缺少参数 → 显示用法提示（`Usage: /curator <subcommand> <args>`）

### 子命令自动补全

在 `command_subcommand_overrides()` 中注册：
```rust
"/curator" => &[
    "status", "run", "pause", "resume", "pin", "unpin", "restore",
    "list-archived",
],
```

## 待移植的后台能力清单

### 1. `run` — curator 审查触发

**Python 实现** (`agent/curator.py`)：
- 两阶段运行：① 自动状态迁移（active→stale→archived，确定性规则）；② LLM review（fork AIAgent，最多 8 次迭代）
- 支持 `--background`（后台线程）和 `--dry-run`（仅预览）

**Rust 现状**：
- 辅助任务配置已存在（`hermes-config/src/config.rs:565`，超时 600s）
- `hermes-skills/src/usage.rs` 中的 `set_state()` 可驱动状态迁移
- 缺少：curator 状态机（`load_state()`/`is_enabled()`/运行间隔控制）、LLM review fork 逻辑

### 2. `pause` / `resume` — curator 启停控制

**Python 实现** (`agent/curator.py`)：
- 通过 state JSON 文件的 `paused` 字段跨 session 持久化
- `pause` → 设置 `paused: true`
- `resume` → 设置 `paused: false`

**Rust 现状**：
- 无 curator state 文件基础设施
- 需要新增 `hermes_config::hermes_home().join("curator_state.json")` 的读写逻辑

### 3. Gateway/IM 适配

**Python 行为**：`/curator` 未标记 `cli_only` / `gateway_only`，全平台可用

**Rust 需要做的**：
- 在 `crates/hermes-gateway/src/commands.rs` 的 `handle_command()` 中添加 `CuratorStatus` / `CuratorPin` 等 `GatewayCommandResult` 变体
- Gateway 输出需要 Markdown 格式化适配（微信气泡限制、飞书卡片等）

## 与 Python 端的差异对照

| 项目 | Python | Rust (本次实现) |
|------|--------|----------------|
| 命令定义 | `commands.py: CommandDef("curator", ...)` | `commands.rs: SLASH_COMMANDS` |
| 平台范围 | CLI + TUI + Gateway 全平台 | 仅 CLI/TUI |
| `status` | 含 pinned 列表 + most/least active top 5 | 简化版：显示所有 agent-created 技能 |
| `pin` / `unpin` | 仅允许 agent-created 技能 | 底层 `set_pinned()` 自动过滤受保护技能 |
| `run` | 完整 LLM review 流程 | 占位提示 |
| `pause` / `resume` | state JSON 持久化 | 占位提示 |
| `archive` | 作为 `list-archived` 的补充 | 单独支持（Python 描述中提到但 subcommands 未列出） |

## 测试策略

### 现有测试覆盖

- `canonical_command("/curator")` → `"/curator"`（`commands.rs:26105+`）
- `SLASH_COMMANDS` 包含 `"/curator"` 条目（通过已有的 slash command 完整性测试）
- `command_subcommand_overrides` 包含 curator 子命令

### 待添加的测试

```rust
// 建议在 commands.rs 的 tests 模块中添加：
#[test]
fn curator_subcommand_completions_registered() {
    let subs = command_subcommand_overrides("/curator");
    assert!(subs.contains(&"status"));
    assert!(subs.contains(&"pin"));
    assert!(subs.contains(&"archive"));
    assert!(subs.contains(&"list-archived"));
}
```

## 构建和运行

```bash
# 编译
cargo build -p hermes-cli

# 运行测试
cargo test -p hermes-cli canonical_command

# 风格检查（注意：hermes-core 有预存在的 clippy 错误会阻塞 -D warnings）
cargo clippy -p hermes-cli
```
