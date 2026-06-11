use hermes_core::AgentError;

use super::super::traits::UiChromeRuntime;
use super::super::{App, PetSettings};

impl UiChromeRuntime for App {
    fn mouse_enabled(&self) -> bool {
        self.stream.mouse_enabled
    }

    fn set_mouse_enabled(&mut self, enabled: bool) {
        self.stream.mouse_enabled = enabled;
    }

    fn request_theme_change(&mut self, skin: &str) {
        App::request_theme_change(self, skin);
    }

    fn take_pending_theme_change(&mut self) -> Option<String> {
        self.stream.pending_theme.take()
    }

    fn take_pending_input_prefill(&mut self) -> Option<String> {
        self.stream.pending_input_prefill.take()
    }

    fn set_pending_image_hint(&mut self, path: String) {
        self.stream.pending_image_hint = Some(path);
    }

    fn pending_image_hint(&self) -> Option<&str> {
        self.stream.pending_image_hint.as_deref()
    }

    fn clear_pending_image_hint(&mut self) {
        self.stream.pending_image_hint = None;
    }

    fn pet_settings(&self) -> &PetSettings {
        &self.chrome.pet_settings
    }

    fn set_pet_settings(&mut self, settings: PetSettings) -> Result<(), AgentError> {
        App::set_pet_settings(self, settings)
    }
}
