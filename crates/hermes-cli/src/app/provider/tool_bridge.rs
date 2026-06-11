use std::sync::Arc;

use serde_json::Value;

use hermes_agent::agent_loop::ToolRegistry as AgentToolRegistry;
use hermes_tools::ToolRegistry;

/// Build async tool dispatch for gateway agents (uses `dispatch_async`, no `block_in_place`).
pub fn async_tool_dispatch_for(tools: Arc<ToolRegistry>) -> hermes_agent::AsyncToolDispatch {
    Arc::new(move |name, params| {
        let tools = tools.clone();
        Box::pin(async move {
            let output = tools.dispatch_async(&name, params).await;
            hermes_tools_dispatch_output(output)
        })
    })
}

fn hermes_tools_dispatch_output(output: String) -> Result<String, hermes_core::ToolError> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&output) {
        if let Some(err) = value.get("error").and_then(|v| v.as_str()) {
            return Err(hermes_core::ToolError::ExecutionFailed(err.to_string()));
        }
    }
    Ok(output)
}

pub fn bridge_tool_registry(tools: &ToolRegistry) -> AgentToolRegistry {
    let mut agent_registry = AgentToolRegistry::new();
    for schema in tools.get_definitions() {
        let name = schema.name.clone();
        let tools_clone = tools.clone();
        agent_registry.register(
            name.clone(),
            schema,
            Arc::new(
                move |params: Value| -> Result<String, hermes_core::ToolError> {
                    Ok(tools_clone.dispatch(&name, params))
                },
            ),
        );
    }
    agent_registry
}
