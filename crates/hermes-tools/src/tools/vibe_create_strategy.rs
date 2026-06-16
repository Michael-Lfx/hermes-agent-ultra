//! create_strategy tool: Create a new declarative backtest strategy from indicators and rules.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use indexmap::IndexMap;
use serde_json::{Value, json};
use tokio::sync::Mutex;

use hermes_core::{JsonSchema, ToolError, ToolHandler, ToolSchema, tool_schema};

pub struct CreateStrategyHandler {
    strategies_dir: PathBuf,
    registry: Arc<Mutex<hermes_strategies::StrategyRegistry>>,
}

impl CreateStrategyHandler {
    pub fn new(
        strategies_dir: PathBuf,
        registry: Arc<Mutex<hermes_strategies::StrategyRegistry>>,
    ) -> Self {
        Self {
            strategies_dir,
            registry,
        }
    }
}

#[async_trait]
impl ToolHandler for CreateStrategyHandler {
    async fn execute(&self, params: Value) -> Result<String, ToolError> {
        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("Missing 'name' parameter".into()))?;

        let description = params
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let indicators = params
            .get("indicators")
            .ok_or_else(|| ToolError::InvalidParams("Missing 'indicators' parameter".into()))?;

        let rules = params
            .get("rules")
            .ok_or_else(|| ToolError::InvalidParams("Missing 'rules' parameter".into()))?;

        let default_params = params.get("default_params").cloned().unwrap_or(json!({}));
        let position_sizing = params
            .get("position_sizing")
            .and_then(|v| v.as_str())
            .unwrap_or("full");

        // Build DeclarativeStrategyDef.
        let def = hermes_strategies::DeclarativeStrategyDef {
            name: name.to_string(),
            description: description.to_string(),
            version: 1,
            author: "user".to_string(),
            indicators: serde_json::from_value(indicators.clone()).map_err(|e| {
                ToolError::InvalidParams(format!("Invalid indicators format: {e}"))
            })?,
            rules: serde_json::from_value(rules.clone()).map_err(|e| {
                ToolError::InvalidParams(format!("Invalid rules format: {e}"))
            })?,
            default_params,
            position_sizing: position_sizing.to_string(),
            market_rules: vec![],
        };

        // Validate.
        def.validate().map_err(|e| {
            ToolError::InvalidParams(format!("Strategy validation failed: {e}"))
        })?;

        // Check name uniqueness.
        {
            let reg = self.registry.lock().await;
            if reg.contains(name) {
                return Err(ToolError::InvalidParams(format!(
                    "Strategy '{name}' already exists"
                )));
            }
        }

        // Serialize and write to disk (atomic write).
        let json = serde_json::to_string_pretty(&def).map_err(|e| {
            ToolError::ExecutionFailed(format!("Serialization error: {e}"))
        })?;

        tokio::fs::create_dir_all(&self.strategies_dir).await.map_err(|e| {
            ToolError::ExecutionFailed(format!("Failed to create strategies directory: {e}"))
        })?;

        let path = self.strategies_dir.join(format!("{name}.json"));
        let tmp = self.strategies_dir.join(format!("{name}.json.tmp"));

        tokio::fs::write(&tmp, &json).await.map_err(|e| {
            ToolError::ExecutionFailed(format!("Failed to write strategy: {e}"))
        })?;

        tokio::fs::rename(&tmp, &path).await.map_err(|e| {
            ToolError::ExecutionFailed(format!("Failed to rename strategy file: {e}"))
        })?;

        // Compile and register.
        let strategy =
            hermes_strategies::DeclarativeStrategy::from_def(def.clone()).map_err(|e| {
                ToolError::ExecutionFailed(format!("Strategy compilation failed: {e}"))
            })?;

        let info = hermes_strategies::StrategyInfo {
            name: def.name.clone(),
            description: def.description.clone(),
            author: def.author.clone(),
            default_params: def.default_params.clone(),
        };

        {
            let mut reg = self.registry.lock().await;
            reg.register(Arc::new(strategy), info);
        }

        Ok(format!(
            "Strategy '{name}' created successfully and saved to {}",
            path.display()
        ))
    }

    fn schema(&self) -> ToolSchema {
        let mut props = IndexMap::new();
        props.insert(
            "name".into(),
            json!({
                "type": "string",
                "description": "Strategy name (lowercase letters, digits, underscores; must start with a letter)"
            }),
        );
        props.insert(
            "description".into(),
            json!({
                "type": "string",
                "description": "Human-readable description of the strategy"
            }),
        );
        props.insert(
            "indicators".into(),
            json!({
                "type": "array",
                "description": "List of indicator definitions. Each has: id, type (sma/ema/rsi/macd/bollinger), params, source (default: 'close')",
                "items": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "string", "description": "Indicator alias for use in rules"},
                        "type": {"type": "string", "enum": ["sma", "ema", "rsi", "macd", "bollinger"]},
                        "params": {"type": "object", "description": "Indicator parameters (e.g. {\"period\": 20})"},
                        "source": {"type": "string", "description": "Input source: 'close' (default) or another indicator id"}
                    },
                    "required": ["id", "type", "params"]
                }
            }),
        );
        props.insert(
            "rules".into(),
            json!({
                "type": "object",
                "description": "Buy/sell rules. Each rule is '<indicator_id> <operator> <indicator_id|number>'. Operators: crosses_above, crosses_below, above, below",
                "properties": {
                    "buy": {"type": "string", "description": "Buy rule expression, e.g. 'rsi_val crosses_above 30'"},
                    "sell": {"type": "string", "description": "Sell rule expression, e.g. 'rsi_val crosses_below 70'"}
                }
            }),
        );
        props.insert(
            "default_params".into(),
            json!({
                "type": "object",
                "description": "Default strategy parameters for run_backtest"
            }),
        );
        props.insert(
            "position_sizing".into(),
            json!({
                "type": "string",
                "description": "Position sizing: 'full', 'half', or 'quarter' (default: full)",
                "enum": ["full", "half", "quarter"]
            }),
        );

        tool_schema(
            "create_strategy",
            "Create a new declarative backtest strategy from indicators and rules. \
             The strategy is saved to disk and immediately available for run_backtest. \
             Example: create a MACD crossover strategy with indicators [{id:'macd_line',type:'macd',params:{fast:12,slow:26}}] \
             and rules {buy:'macd_line crosses_above signal_line', sell:'macd_line crosses_below signal_line'}.",
            JsonSchema::object(props, vec!["name".into(), "indicators".into(), "rules".into()]),
        )
    }
}
