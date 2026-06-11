use std::sync::Arc;

use hermes_config::GatewayConfig;

use super::super::App;
use super::super::traits::ModelRuntime;

impl ModelRuntime for App {
    fn config(&self) -> &Arc<GatewayConfig> {
        &self.core.config
    }

    fn set_config(&mut self, config: Arc<GatewayConfig>) {
        self.core.config = config;
    }

    fn current_model(&self) -> &str {
        &self.model.current_model
    }

    fn current_model_mut(&mut self) -> &mut String {
        &mut self.model.current_model
    }

    fn current_personality(&self) -> Option<&str> {
        self.model.current_personality.as_deref()
    }

    fn switch_model(&mut self, provider_model: &str) {
        App::switch_model(self, provider_model);
    }

    fn switch_personality(&mut self, name: &str) {
        App::switch_personality(self, name);
    }

    fn current_runtime_provider(&self) -> String {
        App::current_runtime_provider(self)
    }
}
