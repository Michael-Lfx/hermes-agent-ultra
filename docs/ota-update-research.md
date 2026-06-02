# Hermes OTA Update 调研报告

> 需求：`hermes update` 实现单 binary 的 OTA 升级，多源（GitHub + ModelScope），自动选低延迟源，支持 Win/Linux/macOS。

---

## 1. 现状总结

### 1.1 现有 `update` 命令实现（仅版本检查，无 OTA）

核心代码位于 `crates/hermes-cli/src/update.rs`（共 79 行），功能单一：

```rust
pub async fn check_for_updates() -> Result<String, AgentError> {
    // 请求 GitHub API 获取最新 release
    // 比较远端 tag_name 与本地 CARGO_PKG_VERSION
    // 返回文字提示
}
```

**行为**：
- 请求 `https://api.github.com/repos/{repo}/releases/latest`
- 比较远端 `tag_name` 与本地 `CARGO_PKG_VERSION`
- 返回"已是最新"或"有新版本，请去 GitHub 下载"的文字提示
- 支持 `HERMES_UPDATE_REPO` 环境变量覆盖默认 repo（`sheawinkler/hermes-agent-ultra`）

**结论：当前 `update` 只是一个版本检查器，不会下载也不会替换 binary。**

### 1.2 命令入口与调度链

| 入口 | 代码位置 | 行为 |
|------|---------|------|
| `hermes update [--check]` | `main.rs:11781` → `run_update()` | CLI 子命令，打印版本状态后退出 |
| `/update [check]` | `commands.rs:17281` → `handle_update_command()` | REPL 内斜杠命令，同样只打印 |

CLI 参数模型：

```rust
// crates/hermes-cli/src/cli/types.rs
Update {
    check: bool,  // 仅一个 --check flag
}

// crates/hermes-cli/src/cli/commands.rs
struct UpdateArgs {
    #[arg(long)]
    check: bool,
}
```

### 1.3 Release 发布流水线

`.github/workflows/release.yml` 已有完善的多平台 CI/CD：

| 目标平台 | 产物格式 | artifact 命名 |
|----------|----------|--------------|
| Linux x86_64 (gnu) | tar.gz | `hermes-linux-x86_64.tar.gz` |
| Linux aarch64 (gnu) | tar.gz | `hermes-linux-aarch64.tar.gz` |
| Linux x86_64 (musl) | tar.gz | `hermes-linux-x86_64-musl.tar.gz` |
| Windows x86_64 | zip | `hermes-windows-x86_64.zip` |
| macOS aarch64 | tar.gz | `hermes-macos-aarch64.tar.gz` |
| macOS x86_64 | tar.gz | `hermes-macos-x86_64.tar.gz` |

安全特性：
- cosign keyless 签名（`.sig` + `.pem` 附在每个 artifact 旁）
- CycloneDX SBOM（`release-sbom.cdx.json`）
- 密钥扫描 gate（`scripts/release_secret_scan.py`）

安装脚本 `scripts/install.sh` 已支持从 GitHub Releases 下载安装（bash 脚本，非 Windows 原生）。

### 1.4 现有可复用依赖

以下 crate 已在 `hermes-cli` 的依赖树中，可直接用于 OTA 逻辑：

| crate | 版本 | OTA 用途 |
|-------|------|---------|
| `reqwest` | 0.13 | HTTP 下载（支持 stream/rustls） |
| `flate2` | 1 | gzip 解压 |
| `tar` | 0.4 | tar 解包 |
| `zip` | 8 | zip 解压（Windows artifact） |
| `sha2` | 0.11 | 哈希校验 |
| `tokio` | 1 | 异步运行时 |
| `hmac` | 0.13 | HMAC 校验（可选） |

---

## 2. 需求 vs 现状差距分析

| 需求点 | 现状 | 差距等级 |
|--------|------|---------|
| `hermes update` 升级 binary | 只做版本检查，不下载不替换 | **核心缺失** |
| 多源：GitHub + ModelScope | 仅 GitHub API | **ModelScope 源完全缺失** |
| 自动选低延迟源 | 无延迟探测机制 | **缺失** |
| 支持 Win/Linux/macOS | CI 已产出多平台 artifact，但 update 无平台检测和下载逻辑 | **下载+替换逻辑缺失** |
| 签名校验 | CI 已用 cosign 签名，客户端无验签逻辑 | **验签缺失** |
| 回滚保护 | 无 | **缺失** |

---

## 3. 架构设计建议

### 3.1 模块化结构

将现有单文件 `update.rs` 重构为模块目录：

```
crates/hermes-cli/src/update/
├── mod.rs          # 公共 API：run_update() 入口
├── source.rs       # Source trait + GitHub/ModelScope 实现
├── probe.rs        # 延迟探测（并发 ping 多源，取最快）
├── download.rs     # binary 下载 + 进度回调
├── replace.rs      # 自替换（rename 旧 binary → 写入新 → 验证）
├── verify.rs       # cosign 签名验证 / SHA256 checksum
└── platform.rs     # 平台检测 → 选择正确的 artifact 名
```

### 3.2 Source Trait 设计

```rust
#[async_trait]
pub trait ReleaseSource: Send + Sync {
    /// 源名称（用于日志和延迟探测显示）
    fn name(&self) -> &str;

    /// 获取最新版本信息和对应的 artifact 下载 URL
    async fn fetch_latest_release(
        &self,
        platform: &Platform,
    ) -> Result<ReleaseInfo, AgentError>;

    /// 下载 artifact bytes
    async fn download_artifact(
        &self,
        url: &str,
        progress: Option<Box<dyn Fn(u64, u64) + Send>>,
    ) -> Result<Vec<u8>, AgentError>;
}

pub struct ReleaseInfo {
    pub version: String,
    pub tag: String,
    pub artifact_url: String,
    pub checksum_url: Option<String>,
    pub signature_url: Option<String>,
    pub release_notes: Option<String>,
}
```

### 3.3 CLI 参数扩展

```rust
Update {
    check: bool,              // --check 只检查不安装
    yes: bool,                // -y 跳过交互确认
    source: Option<String>,   // --source github|modelscope 强制指定源
    rollback: bool,           // --rollback 回退到上一版本
}
```

---

## 4. 分阶段实施计划

### Phase 1 — GitHub 单源 OTA（最小可用）

目标：`hermes update` 能从 GitHub 下载最新 binary 并替换自身。

1. **平台检测**（`platform.rs`）
   - 用 `std::env::consts::{OS, ARCH}` 映射到 artifact 名
   - 映射表：
     ```
     (linux, x86_64)   → hermes-linux-x86_64.tar.gz
     (linux, aarch64)  → hermes-linux-aarch64.tar.gz
     (windows, x86_64) → hermes-windows-x86_64.zip
     (macos, aarch64)  → hermes-macos-aarch64.tar.gz
     (macos, x86_64)   → hermes-macos-x86_64.tar.gz
     ```

2. **版本检查**（复用现有逻辑）
   - 调用 GitHub Releases API
   - 从 response 的 `assets` 数组中匹配平台对应 artifact 的 `browser_download_url`

3. **下载**（`download.rs`）
   - `reqwest` 下载 tar.gz/zip
   - 解压提取 `hermes` binary（tar.gz 用 `flate2`+`tar`，zip 用 `zip` crate）
   - 可选：显示下载进度条

4. **自替换**（`replace.rs`）
   - 获取当前 binary 路径：`std::env::current_exe()`
   - 将当前 binary rename 为 `hermes.bak`（同目录）
   - 写入新 binary 到原路径
   - Unix 平台设置可执行权限（`chmod +x`）
   - 失败时 rename `.bak` 回来（回滚）

### Phase 2 — ModelScope 源支持

1. **Source 实现**
   - ModelScope 没有标准 "Releases" 概念
   - 建议借用 ModelScope 的 dataset 仓库存 binary releases
   - API: `https://modelscope.cn/api/v1/datasets/{namespace}/{dataset}/repo/tree` 获取文件列表
   - 下载: `https://modelscope.cn/api/v1/datasets/{namespace}/{dataset}/repo?Revision=master&FilePath=xxx`

2. **CI/CD 变更**
   - `release.yml` 增加 ModelScope 上传步骤
   - 需要 ModelScope API token 作为 CI secret
   - 上传与 GitHub 相同的 artifact 文件

3. **配置**
   - 新增环境变量 `HERMES_MODELSCOPE_DATASET` 指定 dataset 路径
   - 默认值硬编码为官方 dataset

### Phase 3 — 智能源选择

1. **延迟探测**（`probe.rs`）
   - 并发向 GitHub 和 ModelScope 发 HEAD 请求
   - 测量 RTT（round-trip time）
   - 选延迟最小的源下载
   - 超时阈值：3 秒（探测阶段）

2. **Fallback 机制**
   - 首选源下载失败时，自动切换到备选源
   - 记录失败原因到日志（`tracing::warn!`）

3. **用户覆盖**
   - `--source github` 或 `--source modelscope` 强制指定
   - 环境变量 `HERMES_UPDATE_SOURCE` 持久化偏好

### Phase 4 — 安全加固

1. **SHA256 校验**
   - Release 中附带 `checksums.txt`（CI 生成）
   - 下载完成后校验 binary 哈希

2. **cosign 验签**
   - 内嵌 cosign 公钥或走 keyless OIDC 验证
   - 验证 `.sig` 文件对应 artifact 的签名

3. **回滚命令**
   - `hermes update --rollback`：将 `hermes.bak` rename 回 `hermes`
   - 保留最近一个版本的备份

---

## 5. 关键技术难点

### 5.1 Windows 自替换

Windows 上运行中的 `.exe` 文件**无法被覆盖**（文件锁）。

**解决方案**：
1. 将当前 `hermes.exe` rename 为 `hermes.exe.old`（Windows 允许 rename 运行中的 exe）
2. 将新 binary 写入 `hermes.exe`（原路径已空出）
3. 新 binary 在下次启动时生效
4. `.old` 文件在下次成功启动后清理

**备选方案**：写一个临时 helper 进程，等主进程退出后执行替换。复杂度高，不推荐初版使用。

### 5.2 Unix 权限保持

替换 binary 后需要保持可执行权限：

```rust
#[cfg(unix)]
{
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::Permissions::from_mode(0o755);
    std::fs::set_permissions(&bin_path, perms)?;
}
```

### 5.3 权限不足场景

如果 binary 安装在 `/usr/local/bin` 等需要 root 权限的目录：
- 检测写入权限
- 提示用户使用 `sudo hermes update` 或 `hermes update --dir ~/.local/bin`

### 5.4 ModelScope API 差异

ModelScope 的 API 与 GitHub 差异较大，需要独立的解析逻辑：
- 没有 "latest release" 概念
- 需要在 dataset 中约定版本元数据格式（如 `latest.json`）
- 或者按文件名模式匹配：`hermes-{version}-{platform}.tar.gz`

---

## 6. 风险评估

| 风险 | 等级 | 缓解措施 |
|------|------|----------|
| 自替换失败导致 binary 损坏 | 高 | 备份 `.bak` + 启动时完整性检查 + `--rollback` |
| Windows 运行中 exe 无法覆盖 | 中 | rename-then-write 策略（允许 rename 运行中 exe） |
| ModelScope API 变更或不稳定 | 中 | Source trait 抽象，容易切换后端 |
| 网络中断导致下载不完整 | 中 | 下载完成后校验 SHA256，通过后再替换 |
| 权限不足（`/usr/local/bin`） | 中 | 检测权限，提示 sudo 或自定义安装目录 |
| GitHub API rate limit | 低 | 未认证请求 60/h，足够；可配 `GITHUB_TOKEN` |
| 用户中断（Ctrl+C）下载中途 | 低 | 下载到临时文件，完成后再替换 |

---

## 7. 依赖方案对比

| 方案 | 优点 | 缺点 | 推荐 |
|------|------|------|------|
| **手动实现** | 完全控制；可用现有依赖；支持 ModelScope | 开发量稍大 | **推荐** |
| `self_update` crate | 开箱即用的 GitHub self-update | 不支持 ModelScope；需要 fork 或额外包装 | 不推荐 |
| `cargo-binstall` 思路 | 复用 cargo 生态 | 需要用户安装 cargo；不适合独立 binary 分发 | 不推荐 |

---

## 8. 涉及文件变更清单（预估）

| 文件 | 变更类型 | 说明 |
|------|---------|------|
| `crates/hermes-cli/src/update.rs` | 重构 | 单文件 → 模块目录 |
| `crates/hermes-cli/src/update/mod.rs` | 新增 | 模块入口 + `run_update()` |
| `crates/hermes-cli/src/update/source.rs` | 新增 | Source trait + GitHub/ModelScope impl |
| `crates/hermes-cli/src/update/probe.rs` | 新增 | 延迟探测 |
| `crates/hermes-cli/src/update/download.rs` | 新增 | 下载 + 解压 |
| `crates/hermes-cli/src/update/replace.rs` | 新增 | 自替换（跨平台） |
| `crates/hermes-cli/src/update/verify.rs` | 新增 | 签名/哈希校验 |
| `crates/hermes-cli/src/update/platform.rs` | 新增 | 平台检测 |
| `crates/hermes-cli/src/cli/types.rs` | 修改 | `Update` 变体增加字段 |
| `crates/hermes-cli/src/cli/commands.rs` | 修改 | `UpdateArgs` 增加参数 |
| `crates/hermes-cli/src/main.rs` | 修改 | `run_update()` 调用新逻辑 |
| `crates/hermes-cli/src/commands.rs` | 修改 | `/update` 斜杠命令适配 |
| `.github/workflows/release.yml` | 修改 | 增加 checksums.txt 生成 + ModelScope 上传 |
| `scripts/install.sh` | 可选修改 | 同步支持 ModelScope 源 |
