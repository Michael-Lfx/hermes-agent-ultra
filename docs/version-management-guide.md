# 版本管理与 OTA 更新维护指南

## 概述

Hermes Agent Ultra 采用多源（GitHub + ModelScope）、多渠道（stable/beta/rc/nightly）的 OTA 自更新架构。本文档面向日常版本维护工作，涵盖发版流程、渠道管理、manifest 配置等操作要点。

---

## 一、设计理念

### 1.1 核心原则

- **语义化版本**：所有版本号遵循 SemVer 2.0.0（MAJOR.MINOR.PATCH[-prerelease]），版本比较由 `semver` crate 提供编译时类型安全保证
- **策略与数据分离**：版本比较逻辑通过 `VersionPolicy` trait 抽象，更新元数据（channel/forced/min_version）与策略判断解耦
- **向后兼容**：manifest 格式设计确保旧客户端能安全降级解析，新增字段使用 `Option` + 默认值
- **多源冗余**：GitHub 和 ModelScope 双源，自动延迟探测选最快响应

### 1.2 架构概览

```
用户执行 hermes update
        │
        ▼
┌─── 源选择（probe）────┐
│  并发探测延迟          │
│  GitHub vs ModelScope  │
└───────┬───────────────┘
        ▼
┌─── 获取 manifest ────┐
│  ReleaseManifest     │
│  (version/channel/   │
│   forced/platforms)  │
└───────┬──────────────┘
        ▼
┌─── 版本策略判断 ─────┐
│  SemverPolicy /      │
│  ChannelPolicy       │
│  → UpdateDecision    │
└───────┬──────────────┘
        ▼
  下载 → 校验 → 替换
```

### 1.3 为什么用 Strategy 模式

版本判断不是简单的"大于就更新"。不同场景需要不同策略：
- 生产环境只推 stable，不接受 pre-release
- 测试团队需要 beta/rc 渠道
- 安全漏洞需要跨版本强制更新
- 某些发行版可能锁定渠道

Strategy 模式让这些需求通过替换 Policy 实现，而非修改核心流程。

---

## 二、日常发版流程

### 2.1 发布 stable 版本

1. **打 tag**：
   ```bash
   git tag v1.2.0
   git push origin v1.2.0
   ```

2. **CI 自动完成**：
   - 安全扫描 → 多平台编译 → cosign 签名 → GitHub Release 发布
   - ModelScope 上传：artifacts + manifest 写入 `hermes-agent-ultra/channels/stable.json` 和 `hermes-agent-ultra/latest.json`

3. **验证**：
   ```bash
   hermes update --check
   # 应显示 "New version available: v1.2.0"
   ```

### 2.2 发布 beta/rc 版本

tag 名称带 pre-release 后缀，系统自动识别渠道：

```bash
# Beta
git tag v1.3.0-beta.1
git push origin v1.3.0-beta.1

# Release Candidate
git tag v1.3.0-rc.1
git push origin v1.3.0-rc.1
```

CI 会将 manifest 写入对应渠道路径（如 `hermes-agent-ultra/channels/beta.json`）。

### 2.3 用户侧切换渠道

```bash
# 检查 beta 渠道
hermes update --check --channel beta

# 更新到 beta 最新
hermes update --channel beta

# 回到 stable
hermes update --channel stable
```

### 2.4 强制指定更新源

```bash
hermes update --source github
hermes update --source modelscope
```

或通过环境变量持久化：
```bash
export HERMES_UPDATE_SOURCE=modelscope
```

---

## 三、Manifest 格式规范

### 3.1 完整格式

```json
{
  "version": "1.2.0",
  "channel": "stable",
  "pub_date": "2026-06-03T12:00:00Z",
  "forced": false,
  "min_version": "0.10.0",
  "notes": "What's new in this release...",
  "platforms": {
    "linux-x86_64": {
      "url": "https://..../hermes-linux-x86_64.tar.gz",
      "sha256": "abcd1234...",
      "size": 12345678
    },
    "windows-x86_64": {
      "url": "https://..../hermes-windows-x86_64.zip",
      "sha256": "efgh5678...",
      "size": 9876543
    },
    "macos-aarch64": { ... },
    "macos-x86_64": { ... },
    "linux-aarch64": { ... }
  },
  "artifacts": ["hermes-linux-x86_64.tar.gz", "hermes-windows-x86_64.zip", ...]
}
```

### 3.2 字段说明

| 字段 | 必填 | 说明 |
|------|------|------|
| `version` | 是 | SemVer 版本号（不带 v 前缀） |
| `channel` | 否 | 默认 "stable"。从 version pre-release 自动推导 |
| `pub_date` | 否 | ISO 8601 发布时间 |
| `forced` | 否 | 设为 true 时客户端强制更新（用于安全补丁） |
| `min_version` | 否 | 低于此版本的客户端会被强制更新 |
| `notes` | 否 | Release notes |
| `platforms` | 是(新格式) | 按平台的下载信息，含 URL/sha256/size |
| `artifacts` | 否 | 文件名列表（向后兼容旧客户端） |

### 3.3 平台 key 映射

| 平台 | Key |
|------|-----|
| Linux x86_64 | `linux-x86_64` |
| Linux ARM64 | `linux-aarch64` |
| Linux x86_64 (musl) | `linux-x86_64-musl` |
| Windows x86_64 | `windows-x86_64` |
| macOS ARM64 | `macos-aarch64` |
| macOS x86_64 | `macos-x86_64` |

### 3.4 向后兼容

旧客户端（未升级版本管理模块的版本）：
- 只读取 `version` 和 `artifacts` 字段
- 忽略不认识的字段（`platforms`/`forced`/`min_version` 等）
- 仍然可以正常检测更新和下载

---

## 四、版本策略配置

### 4.1 更新决策逻辑

```
available > current → 建议更新
available == current → 已是最新
available < current → 不更新（除非 forced=true）
current < min_version → 强制更新
available 在 deprecated 列表中 → 不更新
```

### 4.2 强制更新场景

当发现安全漏洞需要所有用户紧急升级时：

1. 在 manifest 中设置 `"forced": true`
2. 或设置 `"min_version": "1.1.5"`（低于此版本的客户端自动强制更新）

客户端行为：
- 显示 `[FORCED UPDATE REQUIRED]` 提示
- 仍然会询问确认（除非带 `-y`），但跳过版本比较

### 4.3 渠道过滤规则

| 用户渠道 | 能收到的更新 |
|----------|-------------|
| stable | 仅 stable 正式版 |
| rc | stable + rc |
| beta | stable + rc + beta |
| nightly | 所有版本 |

---

## 五、ModelScope 仓库结构

```
flowy2025/agent (dataset)
└── hermes-agent-ultra/
    ├── latest.json                          ← stable 最新版 manifest（别名）
    ├── channels/
    │   ├── stable.json                      ← stable 渠道 manifest
    │   ├── beta.json                        ← beta 渠道 manifest
    │   └── nightly.json                     ← nightly 渠道 manifest
    ├── v1.2.0/
    │   ├── hermes-linux-x86_64.tar.gz
    │   ├── hermes-linux-aarch64.tar.gz
    │   ├── hermes-windows-x86_64.zip
    │   ├── hermes-macos-aarch64.tar.gz
    │   ├── hermes-macos-x86_64.tar.gz
    │   └── checksums.sha256
    └── v1.3.0-beta.1/
        └── ...
```

---

## 六、环境变量参考

| 变量 | 作用 | 默认值 |
|------|------|--------|
| `HERMES_UPDATE_SOURCE` | 强制指定源 (github/modelscope) | 自动探测 |
| `HERMES_UPDATE_REPO` | GitHub 仓库地址 | Michael-Lfx/hermes-agent-ultra |
| `HERMES_MODELSCOPE_REPO` | ModelScope dataset 地址 | flowy2025/agent |
| `GITHUB_TOKEN` | GitHub API 认证（私有仓库） | 无 |

---

## 七、扩展方向

### 7.1 发行版渠道锁定

某些场景下希望分发的 binary 只能使用特定渠道（如 OEM 版本锁定 stable）：

- 通过 Cargo feature flag 在编译时注入锁定渠道
- 运行时优先级：编译锁定 > 服务端下发 > 用户参数
- 当前 Strategy 模式天然支持，新增一个 `LockedChannelPolicy` 包装即可

### 7.2 渐进式发布 (Progressive Rollout)

逐步扩大推送范围，降低风险：

- manifest 新增 `rollout_percentage` 字段（如 10% → 50% → 100%）
- 客户端根据设备 ID hash 决定是否命中灰度
- 可通过 `metadata` 扩展字段承载，不破坏现有格式

### 7.3 增量更新 (Delta Update)

当前为全量替换 binary。未来可优化：

- 使用 bsdiff/bspatch 生成差分包
- manifest 的 `platforms` 中新增 `delta_url` + `delta_from_version` 字段
- 客户端判断是否有匹配的 delta，fallback 为全量

### 7.4 自动回滚

更新后如果新版本启动崩溃：

- 当前已支持 `hermes update --rollback` 手动回滚
- 可扩展为：新版本首次启动写入 "pending" 标记，稳定运行 N 秒后确认；崩溃时自动恢复 `.bak`

### 7.5 版本回溯审计

记录每次更新的历史：

- 本地存储 `~/.hermes/update-history.json`
- 包含时间戳、from/to 版本、来源、成功/失败
- 用于诊断和 telemetry

### 7.6 多产品共享基础设施

当前 ModelScope 仓库已设计为多项目前缀（`hermes-agent-ultra/`）。未来其他产品可复用：

- 相同的 manifest 格式
- 相同的 CI 上传脚本（改 `--prefix` 参数即可）
- 客户端共享 `version.rs` 和 `manifest.rs` 模块

---

## 八、故障排查

### 常见问题

| 症状 | 可能原因 | 解决方法 |
|------|---------|---------|
| "No artifact for platform" | manifest 缺少当前平台 | 检查 CI 是否成功构建该平台 |
| 版本检查超时 | 网络问题 | 用 `--source` 指定可达的源 |
| "Already up to date" 但实际有新版 | 旧 manifest 缓存 | ModelScope CDN 可能有缓存延迟 |
| 强制更新不生效 | manifest.forced=false | 检查上传脚本生成的 manifest |
| channel 参数被忽略 | 编译时渠道锁定 | 检查是否有 feature flag 限制 |

### 调试方法

```bash
# 查看详细日志
RUST_LOG=debug hermes update --check

# 强制指定源排查
hermes update --check --source modelscope
hermes update --check --source github

# 查看当前版本信息
hermes --version
```
