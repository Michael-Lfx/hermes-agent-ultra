use hermes_core::AgentError;

use super::super::App;
use super::super::traits::SessionSnapshotRuntime;

impl SessionSnapshotRuntime for App {
    fn session_info(&self) -> super::super::SessionInfo {
        App::session_info(self)
    }

    fn persist_session_snapshot(
        &mut self,
        name: Option<&str>,
    ) -> Result<std::path::PathBuf, AgentError> {
        App::persist_session_snapshot(self, name)
    }

    fn apply_agent_result_and_persist(
        &mut self,
        result: hermes_core::AgentResult,
    ) -> Result<(), hermes_core::AgentError> {
        App::apply_agent_result_and_persist(self, result)
    }

    fn flush_session_teardown(&self, interrupted: bool) {
        App::flush_session_teardown(self, interrupted);
    }

    fn running_background_job_count(&self) -> usize {
        App::running_background_job_count(self)
    }
}
