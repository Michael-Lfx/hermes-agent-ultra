//! list_strategies tool: List all available backtest strategies.

use std::sync::Arc;

use async_trait::async_trait;
use indexmap::IndexMap;
use serde_json::Value;
use tokio::sync::Mutex;

use hermes_core::{JsonSchema, ToolError, ToolHandler, ToolSchema, tool_schema};

pub struct ListStrategiesHandler {
    strategy_registry: Arc<Mutex<hermes_strategies::StrategyRegistry>>,
}

impl ListStrategiesHandler {
    pub fn new(strategy_registry: Arc<Mutex<hermes_strategies::StrategyRegistry>>) -> Self {
        Self { strategy_registry }
    }
}

#[async_trait]
impl ToolHandler for ListStrategiesHandler {
    async fn execute(&self, _params: Value) -> Result<String, ToolError> {
        let reg = self.strategy_registry.lock().await;
        let strategies = reg.list();
        serde_json::to_string_pretty(&strategies)
            .map_err(|e| ToolError::ExecutionFailed(format!("Serialization error: {e}")))
    }

    fn schema(&self) -> ToolSchema {
        let props = IndexMap::new();

        tool_schema(
            "list_strategies",
            "List all available backtest strategy templates with their descriptions and default parameters. \
             Includes both built-in strategies and user-created strategies. \
             Use this before run_backtest to discover which strategies are supported.",
            JsonSchema::object(props, vec![]),
        )
    }
}
