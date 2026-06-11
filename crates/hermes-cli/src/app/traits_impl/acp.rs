use std::sync::Arc;

use hermes_acp_server::server::AcpPipeServer;

use super::super::App;
use super::super::traits::AcpServerRuntime;

impl AcpServerRuntime for App {
    fn acp_server(&self) -> Option<&Arc<AcpPipeServer>> {
        self.acp.server.as_ref()
    }

    fn acp_server_mut(&mut self) -> &mut Option<Arc<AcpPipeServer>> {
        &mut self.acp.server
    }

    fn acp_event_buffer(&self) -> Option<&Arc<std::sync::Mutex<Vec<String>>>> {
        self.acp.event_buffer.as_ref()
    }

    fn acp_event_buffer_mut(&mut self) -> &mut Option<Arc<std::sync::Mutex<Vec<String>>>> {
        &mut self.acp.event_buffer
    }
}
