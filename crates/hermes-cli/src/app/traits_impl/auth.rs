use async_trait::async_trait;
use hermes_core::AgentError;

use super::super::App;
use super::super::traits::AuthRuntime;

#[async_trait]
impl AuthRuntime for App {
    async fn verify_runtime_auth(&mut self, force_refresh: bool) -> Result<String, AgentError> {
        App::verify_runtime_auth(self, force_refresh).await
    }
}
