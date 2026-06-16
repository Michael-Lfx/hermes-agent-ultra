//! Built-in strategy definitions (sma_cross, rsi_revert).
//!
//! These are the same strategies that were hardcoded in `hermes-vibe/backtest.rs`,
//! now expressed as declarative definitions that go through the same execution
//! path as user-defined strategies.

use crate::dsl::{DeclarativeStrategyDef, IndicatorDef, RulesDef};

/// Return the built-in SMA crossover strategy definition.
pub fn sma_cross_def() -> DeclarativeStrategyDef {
    DeclarativeStrategyDef {
        name: "sma_cross".into(),
        description: "SMA crossover: buy on golden cross, sell on death cross".into(),
        version: 1,
        author: "builtin".into(),
        indicators: vec![
            IndicatorDef {
                id: "sma_short".into(),
                indicator_type: "sma".into(),
                params: serde_json::json!({"period": 20}),
                source: "close".into(),
            },
            IndicatorDef {
                id: "sma_long".into(),
                indicator_type: "sma".into(),
                params: serde_json::json!({"period": 50}),
                source: "close".into(),
            },
        ],
        rules: RulesDef {
            buy: Some("sma_short crosses_above sma_long".into()),
            sell: Some("sma_short crosses_below sma_long".into()),
        },
        default_params: serde_json::json!({"short_window": 20, "long_window": 50}),
        position_sizing: "full".into(),
        market_rules: vec![],
    }
}

/// Return the built-in RSI mean-reversion strategy definition.
pub fn rsi_revert_def() -> DeclarativeStrategyDef {
    DeclarativeStrategyDef {
        name: "rsi_revert".into(),
        description: "RSI mean reversion: buy when RSI crosses above oversold, sell when crosses below overbought".into(),
        version: 1,
        author: "builtin".into(),
        indicators: vec![IndicatorDef {
            id: "rsi_val".into(),
            indicator_type: "rsi".into(),
            params: serde_json::json!({"period": 14}),
            source: "close".into(),
        }],
        rules: RulesDef {
            buy: Some("rsi_val crosses_above 30".into()),
            sell: Some("rsi_val crosses_below 70".into()),
        },
        default_params: serde_json::json!({"rsi_period": 14, "oversold": 30, "overbought": 70}),
        position_sizing: "full".into(),
        market_rules: vec![],
    }
}

/// Return all built-in strategy definitions.
pub fn all_builtin_defs() -> Vec<DeclarativeStrategyDef> {
    vec![sma_cross_def(), rsi_revert_def()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_defs_are_valid() {
        for def in all_builtin_defs() {
            assert!(def.validate().is_ok(), "Built-in '{}' failed validation", def.name);
        }
    }
}
