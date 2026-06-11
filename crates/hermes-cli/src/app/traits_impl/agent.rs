use std::sync::Arc;

use async_trait::async_trait;
use hermes_agent::{AgentLoop, InterruptController};
use hermes_core::{AgentError, ToolSchema};
use hermes_tools::ToolRegistry;

use super::super::App;
use super::super::traits::{AgentCoordinator, AgentDriver};

impl AgentCoordinator for App {
    fn agent(&self) -> &Arc<AgentLoop> {
        &self.core.agent
    }

    fn tool_registry(&self) -> &Arc<ToolRegistry> {
        &self.core.tool_registry
    }

    fn tool_schemas(&self) -> &[ToolSchema] {
        &self.core.tool_schemas
    }

    fn interrupt_controller(&self) -> &InterruptController {
        &self.core.interrupt_controller
    }

    fn interrupt_controller_mut(&mut self) -> &mut InterruptController {
        &mut self.core.interrupt_controller
    }

    fn running(&self) -> bool {
        self.runtime.running()
    }

    fn set_running(&mut self, running: bool) {
        self.runtime.set_running(running);
    }

    fn quorum_armed_once(&self) -> bool {
        self.runtime.quorum_armed_once()
    }

    fn set_quorum_armed_once(&mut self, armed: bool) {
        self.runtime.set_quorum_armed_once(armed);
    }
}

#[async_trait]
impl AgentDriver for App {
    async fn run_agent_turn(&mut self) -> Result<(), AgentError> {
        App::run_agent(self).await
    }
}
