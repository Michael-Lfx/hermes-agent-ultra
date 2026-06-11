use super::super::App;
use super::super::traits::TranscriptRuntime;

impl TranscriptRuntime for App {
    fn stream_attached(&self) -> bool {
        self.stream.stream_attached()
    }

    fn stream_handle(&self) -> Option<&crate::tui::StreamHandle> {
        self.stream.stream_handle.as_ref()
    }

    fn set_stream_handle(&mut self, handle: Option<crate::tui::StreamHandle>) {
        App::set_stream_handle(self, handle);
    }

    fn push_ui_message(&mut self, message: hermes_core::Message) {
        App::push_ui_message(self, message);
    }

    fn push_ui_user(&mut self, text: String) {
        App::push_ui_user(self, text);
    }

    fn push_ui_assistant(&mut self, text: String) {
        App::push_ui_assistant(self, text);
    }

    fn transcript_messages(&self) -> Vec<hermes_core::Message> {
        App::transcript_messages(self)
    }

    fn prepare_user_message(&mut self, raw: &str) -> String {
        App::prepare_user_message(self, raw)
    }
}
