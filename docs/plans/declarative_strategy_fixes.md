# 声明式策略框架修复计划

## Context

代码审查（commit 31c020777）发现 10 个问题，其中 3 个 Critical（行为回归/逻辑错误），5 个 Warning（功能缺失/防御性编程），2 个 Suggestion（性能/可观测性）。本计划按优先级逐一修复。

## 修复清单（按优先级）

### Fix 1: 内置策略运行时参数被静默忽略（Critical — 行为回归）

**问题**：`run_backtest(strategy="sma_cross", params={short_window: 10})` 现在忽略用户 params，使用 builtin.rs 硬编码的 period=20/50。

**根因**：`Strategy::run()` 不接受 params，声明式策略在编译期硬编码参数。

**修复方案**：在 trading_backtest.rs 路由层增加判断——若用户传了 params 且策略为内置策略，走硬编码路径（保持向后兼容）：

**文件**：`crates/hermes-tools/src/tools/trading_backtest.rs` (L70-96)

```rust
let reg = self.strategy_registry.lock().await;
if let Some(strategy) = reg.get(strategy_name) {
    // 用户传了 params → 走硬编码路径以支持参数覆盖
    let has_params = strategy_params.as_object().map_or(false, |o| !o.is_empty());
    if has_params {
        drop(reg);
        return Ok(BacktestEngine::run(&data, strategy_name, &strategy_params)
            .map_err(|e| ToolError::ExecutionFailed(format!("Backtest failed: {e}")))?);
    }
    // 否则走声明式路径
    let decisions = strategy.run(&data).map_err(|e| {
        ToolError::ExecutionFailed(format!("Strategy execution failed: {e}"))
    })?;
    // ... 后续不变
}
```

**复杂度**：低

---

### Fix 2: RSI 算法不一致导致回测结果静默变化（Critical — 行为回归）

**问题**：`hermes_strategies::Rsi` 使用简单滚动平均，`hermes_trading::rsi()` 使用 Wilder's smoothing。同一策略产生不同结果。

**根因**：两个 crate 的 RSI 实现算法不同。

**修复方案**：将 `hermes_strategies::indicators::Rsi` 改为 Wilder's smoothing 算法，与 `hermes_trading::rsi()` 保持一致。

**文件**：`crates/hermes-strategies/src/indicators.rs` (L116-137)

将 `compute()` 和 `compute_series()` 重写为 Wilder's smoothing：

```rust
impl Indicator for Rsi {
    fn compute_series(&self, closes: &[f64]) -> Vec<Option<f64>> {
        let n = closes.len();
        if self.period == 0 || n < self.period + 1 {
            return vec![None; n];
        }
        let mut result = Vec::with_capacity(n);
        for _ in 0..self.period {
            result.push(None);
        }
        let mut avg_gain: f64 = 0.0;
        let mut avg_loss: f64 = 0.0;
        for i in 1..=self.period {
            let delta = closes[i] - closes[i - 1];
            if delta > 0.0 { avg_gain += delta; } else { avg_loss += -delta; }
        }
        avg_gain /= self.period as f64;
        avg_loss /= self.period as f64;
        let rs = if avg_loss == 0.0 { 100.0 } else { avg_gain / avg_loss };
        result.push(Some(if avg_loss == 0.0 { 100.0 } else { 100.0 - 100.0 / (1.0 + rs) }));
        for i in (self.period + 1)..n {
            let delta = closes[i] - closes[i - 1];
            let gain = if delta > 0.0 { delta } else { 0.0 };
            let loss = if delta < 0.0 { -delta } else { 0.0 };
            avg_gain = (avg_gain * (self.period as f64 - 1.0) + gain) / self.period as f64;
            avg_loss = (avg_loss * (self.period as f64 - 1.0) + loss) / self.period as f64;
            let r = if avg_loss == 0.0 { 100.0 } else { 100.0 - 100.0 / (1.0 + avg_gain / avg_loss) };
            result.push(Some(r));
        }
        result
    }
    fn compute(&self, _closes: &[f64], _index: usize) -> Option<f64> {
        // compute_series 已覆盖，保留 compute 作为 fallback
        None
    }
}
```

**新增测试**：在 `hermes-strategies` 中增加测试，对比 `hermes_strategies::Rsi` 和 `hermes_trading::rsi()` 输出一致。

**复杂度**：中

---

### Fix 3: Cross 检测在 bar 0 产生虚假信号（Critical — 逻辑错误）

**问题**：`bar_index == 0` 时 `saturating_sub(1)` 返回 0，`prev == cur` 导致虚假 crossover。

**修复方案**：在 cross 规则评估中要求 `bar_index > 0`。

**文件**：`crates/hermes-strategies/src/declarative.rs` (L244-263)

```rust
RuleExpr::CrossesAbove { left, right } => {
    if bar_index == 0 { return false; }
    let prev = get_value(left, bar_index - 1, series_map);
    let cur = get_value(left, bar_index, series_map);
    let prev_right = get_operand_value(right, bar_index - 1, series_map);
    let cur_right = get_operand_value(right, bar_index, series_map);
    match (prev, cur, prev_right, cur_right) {
        (Some(p), Some(c), Some(pr), Some(cr)) => p <= pr && c > cr,
        _ => false,
    }
}
RuleExpr::CrossesBelow { left, right } => {
    if bar_index == 0 { return false; }
    // ... 同上
}
```

**复杂度**：低

---

### Fix 4: 用户策略跨会话持久化未接通（Warning — 功能缺失）

**问题**：`register()` 只调用 `with_builtins()`，从未调用 `load_from_dir()`。重启后用户策略丢失。

**修复方案**：在 `register()` 中加载用户策略目录。由于 `register()` 是同步函数，使用 `tokio::task::block_in_place`。

**文件**：`crates/hermes-tools/src/register/trading.rs` (L15-21)

```rust
let strategies_dir = hermes_config::hermes_home().join("trading").join("strategies");
let mut registry = hermes_strategies::StrategyRegistry::with_builtins();
tokio::task::block_in_place(|| {
    tokio::runtime::Handle::current().block_on(registry.load_from_dir(&strategies_dir));
});
let strategy_registry = Arc::new(Mutex::new(registry));
```

**复杂度**：低

---

### Fix 5: unregister 内置策略保护可被绕过（Warning — 安全）

**问题**：保护逻辑依赖 `strategies` 和 `info` 同步，可被绕过。

**修复方案**：重构 `unregister()` 为先查 info 再决定是否允许删除。

**文件**：`crates/hermes-strategies/src/registry.rs` (L173-180)

```rust
pub fn unregister(&mut self, name: &str) -> bool {
    if self.info.get(name).map_or(false, |m| m.info.author == "builtin") {
        return false;
    }
    let removed_s = self.strategies.remove(name).is_some();
    let removed_i = self.info.remove(name).is_some();
    removed_s || removed_i
}
```

**复杂度**：低

---

### Fix 6: run_from_signals 在空数据上 panic（Warning — 防御性编程）

**问题**：`data.rows.first().unwrap()` 在空数据上 panic。

**修复方案**：增加防御性检查。

**文件**：`crates/hermes-trading/src/backtest.rs` (L154-155)

```rust
if data.is_empty() {
    return Err(TradingError::Backtest("Cannot run backtest on empty data".into()));
}
```

**复杂度**：低

---

### Fix 7: 指标 period 为 0 时产生 NaN（Warning — 防御性编程）

**问题**：`as_u64()` 对 `0` 返回 `Some(0)`，`Sma::new(0)` 除零产生 NaN。

**修复方案**：在 declarative.rs 中 period 提取后增加 > 0 校验。

**文件**：`crates/hermes-strategies/src/declarative.rs` (L134-158)

在 `instantiate_indicator` 中每个指标的 period 提取后增加：

```rust
let period = def.params.get("period")
    .and_then(|v| v.as_u64())
    .ok_or_else(|| StrategyError::InvalidParams(
        format!("'{}' missing 'period' param", def.id)
    ))? as usize;
if period == 0 {
    return Err(StrategyError::InvalidParams(
        format!("'{}' period must be > 0", def.id)
    ));
}
```

**复杂度**：低

---

### Fix 8: Schema enum 移除导致 LLM 准确性下降（Warning — 用户体验）

**问题**：旧 schema 有 `"enum": ["sma_cross", "rsi_revert"]`，移除后 LLM 可能生成无效策略名。

**修复方案**：在错误返回中增加 "did you mean" 提示。

**文件**：`crates/hermes-tools/src/tools/trading_backtest.rs` (L91-95)

```rust
} else {
    // Fallback failed — provide helpful error with available strategies
    let available = reg.list().into_iter().map(|s| s.name).collect::<Vec<_>>();
    let hint = if available.is_empty() {
        String::new()
    } else {
        format!(" Available strategies: {}.", available.join(", "))
    };
    return Err(ToolError::ExecutionFailed(
        format!("Unsupported strategy '{}'.{}", strategy_name, hint)
    ));
}
```

**复杂度**：低

---

### Fix 9: Registry mutex 在同步计算期间被持有（Suggestion — 性能）

**问题**：`strategy.run()` 耗时较长时阻塞其他工具访问 registry。

**修复方案**：获取 `Arc<dyn Strategy>` clone 后立即释放锁。

**文件**：`crates/hermes-tools/src/tools/trading_backtest.rs` (L72-83)

```rust
let strategy = {
    let reg = self.strategy_registry.lock().await;
    reg.get(strategy_name)
};
if let Some(strategy) = strategy {
    let decisions = strategy.run(&data).map_err(|e| {
        ToolError::ExecutionFailed(format!("Strategy execution failed: {e}"))
    })?;
    // ...
}
```

**复杂度**：低

---

### Fix 10: 链式指标 source 找不到时静默回退（Suggestion — 可观测性）

**问题**：source 不存在时静默回退到 `closes.clone()`。

**修复方案**：增加 `tracing::warn!` 日志。

**文件**：`crates/hermes-strategies/src/declarative.rs` (L80-86)

```rust
series_map
    .get(source)
    .map(|s| s.iter().map(|v| v.unwrap_or(0.0)).collect())
    .unwrap_or_else(|| {
        tracing::warn!(indicator = %id, source = %source, "Chained source not found, falling back to close prices");
        closes.clone()
    })
```

**复杂度**：低

---

## 验证步骤

1. **编译**：`cargo build -p hermes-strategies -p hermes-trading -p hermes-tools --features trading-research`
2. **测试**：`cargo test -p hermes-strategies -p hermes-trading`（现有 20 + 39 测试全绿）
3. **Parity**：`cargo test -p hermes-parity-tests`（fixture 全绿）
4. **Clippy**：`cargo clippy -p hermes-strategies -p hermes-trading -p hermes-tools -- -D warnings`
5. **新增测试**：
   - Fix 2: RSI Wilder's smoothing 对比测试（hermes_strategies vs hermes_trading）
   - Fix 3: bar 0 cross 不触发信号测试
   - Fix 6: 空数据 run_from_signals 返回错误而非 panic
   - Fix 7: period=0 返回 InvalidParams 错误
6. **手动验证**：
   - `run_backtest(strategy="sma_cross", params={short_window: 10})` 使用 period=10（走硬编码路径）
   - `run_backtest(strategy="sma_cross")` 使用 period=20/50（走声明式路径）
   - 创建用户策略 → 重启 → `list_strategies` 仍显示该策略
