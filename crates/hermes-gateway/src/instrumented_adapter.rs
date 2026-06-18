//! Transparent decorator that adds elapsed-time logging to every outbound
//! `PlatformAdapter` call.
//!
//! Mounted in `Gateway::register_adapter` so all callers benefit automatically.
//! Concrete `platforms/*` implementations are unchanged.

use std::sync::Arc;

use async_trait::async_trait;

use hermes_core::errors::GatewayError;
use hermes_core::traits::{ParseMode, PlatformAdapter, SendMessageOptions};

/// Transparent adapter decorator: adds debug-level timing metrics to outbound
/// calls without changing any platform behaviour.
pub(crate) struct InstrumentedAdapter {
    inner: Arc<dyn PlatformAdapter>,
    #[allow(dead_code)] // reserved for outbound timing logs when re-enabled
    platform: String,
}

impl InstrumentedAdapter {
    pub(crate) fn new(platform: impl Into<String>, adapter: Arc<dyn PlatformAdapter>) -> Self {
        Self {
            inner: adapter,
            platform: platform.into(),
        }
    }
}

#[async_trait]
impl PlatformAdapter for InstrumentedAdapter {
    async fn start(&self) -> Result<(), GatewayError> {
        self.inner.start().await
    }

    async fn stop(&self) -> Result<(), GatewayError> {
        self.inner.stop().await
    }

    async fn send_message(
        &self,
        chat_id: &str,
        text: &str,
        parse_mode: Option<ParseMode>,
    ) -> Result<(), GatewayError> {
        let result = self.inner.send_message(chat_id, text, parse_mode).await;
        // debug!(
        //     platform = %self.platform,
        //     elapsed_ms = t.elapsed().as_millis(),
        //     ok = result.is_ok(),
        //     "send_message"
        // );
        result
    }

    async fn send_message_with_id(
        &self,
        chat_id: &str,
        text: &str,
        parse_mode: Option<ParseMode>,
    ) -> Result<Option<String>, GatewayError> {
        let result = self
            .inner
            .send_message_with_id(chat_id, text, parse_mode)
            .await;
        // debug!(
        //     platform = %self.platform,
        //     elapsed_ms = t.elapsed().as_millis(),
        //     ok = result.is_ok(),
        //     "send_message_with_id"
        // );
        result
    }

    async fn send_message_replying(
        &self,
        chat_id: &str,
        text: &str,
        parse_mode: Option<ParseMode>,
        reply_to_message_id: Option<&str>,
    ) -> Result<Option<String>, GatewayError> {
        self.inner
            .send_message_replying(chat_id, text, parse_mode, reply_to_message_id)
            .await
    }

    async fn send_message_in_thread(
        &self,
        chat_id: &str,
        text: &str,
        parse_mode: Option<ParseMode>,
        reply_to_message_id: Option<&str>,
        message_thread_id: Option<&str>,
    ) -> Result<Option<String>, GatewayError> {
        self.inner
            .send_message_in_thread(
                chat_id,
                text,
                parse_mode,
                reply_to_message_id,
                message_thread_id,
            )
            .await
    }

    async fn send_message_threaded(
        &self,
        chat_id: &str,
        text: &str,
        parse_mode: Option<ParseMode>,
        thread_id: Option<&str>,
    ) -> Result<(), GatewayError> {
        self.inner
            .send_message_threaded(chat_id, text, parse_mode, thread_id)
            .await
    }

    async fn send_message_with_options(
        &self,
        chat_id: &str,
        text: &str,
        parse_mode: Option<ParseMode>,
        options: SendMessageOptions,
    ) -> Result<(), GatewayError> {
        self.inner
            .send_message_with_options(chat_id, text, parse_mode, options)
            .await
    }

    async fn send_or_update_status(
        &self,
        chat_id: &str,
        status_key: &str,
        text: &str,
        parse_mode: Option<ParseMode>,
    ) -> Result<(), GatewayError> {
        self.inner
            .send_or_update_status(chat_id, status_key, text, parse_mode)
            .await
    }

    async fn edit_message(
        &self,
        chat_id: &str,
        message_id: &str,
        text: &str,
    ) -> Result<(), GatewayError> {
        self.inner.edit_message(chat_id, message_id, text).await
    }

    async fn send_file(
        &self,
        chat_id: &str,
        file_path: &str,
        caption: Option<&str>,
    ) -> Result<(), GatewayError> {
        let result = self.inner.send_file(chat_id, file_path, caption).await;
        // debug!(
        //     platform = %self.platform,
        //     elapsed_ms = t.elapsed().as_millis(),
        //     ok = result.is_ok(),
        //     "send_file"
        // );
        result
    }

    async fn send_file_with_options(
        &self,
        chat_id: &str,
        file_path: &str,
        caption: Option<&str>,
        options: SendMessageOptions,
    ) -> Result<(), GatewayError> {
        self.inner
            .send_file_with_options(chat_id, file_path, caption, options)
            .await
    }

    async fn send_image_url(
        &self,
        chat_id: &str,
        image_url: &str,
        caption: Option<&str>,
    ) -> Result<(), GatewayError> {
        let result = self.inner.send_image_url(chat_id, image_url, caption).await;
        // debug!(
        //     platform = %self.platform,
        //     elapsed_ms = t.elapsed().as_millis(),
        //     ok = result.is_ok(),
        //     "send_image_url"
        // );
        result
    }

    async fn delete_message(&self, chat_id: &str, message_id: &str) -> Result<bool, GatewayError> {
        self.inner.delete_message(chat_id, message_id).await
    }

    async fn add_reaction(
        &self,
        chat_id: &str,
        message_id: &str,
        emoji: &str,
    ) -> Result<(), GatewayError> {
        let result = self.inner.add_reaction(chat_id, message_id, emoji).await;
        // debug!(
        //     platform = %self.platform,
        //     elapsed_ms = t.elapsed().as_millis(),
        //     ok = result.is_ok(),
        //     "add_reaction"
        // );
        result
    }

    async fn remove_reaction(
        &self,
        chat_id: &str,
        message_id: &str,
        emoji: &str,
    ) -> Result<(), GatewayError> {
        self.inner.remove_reaction(chat_id, message_id, emoji).await
    }

    fn reactions_enabled(&self) -> bool {
        self.inner.reactions_enabled()
    }

    async fn trigger_typing(&self, chat_id: &str) -> Result<(), GatewayError> {
        let result = self.inner.trigger_typing(chat_id).await;
        // debug!(
        //     platform = %self.platform,
        //     elapsed_ms = t.elapsed().as_millis(),
        //     ok = result.is_ok(),
        //     "trigger_typing"
        // );
        result
    }

    async fn stop_typing(&self, chat_id: &str) -> Result<(), GatewayError> {
        self.inner.stop_typing(chat_id).await
    }

    async fn respond_interaction(
        &self,
        interaction_id: &str,
        interaction_token: &str,
        content: &str,
    ) -> Result<(), GatewayError> {
        self.inner
            .respond_interaction(interaction_id, interaction_token, content)
            .await
    }

    fn supports_native_streaming(&self) -> bool {
        self.inner.supports_native_streaming()
    }

    async fn start_native_stream(
        &self,
        chat_id: &str,
        reply_to: Option<&str>,
        initial_content: Option<&str>,
    ) -> Result<Option<String>, GatewayError> {
        self.inner
            .start_native_stream(chat_id, reply_to, initial_content)
            .await
    }

    async fn send_native_stream_chunk(
        &self,
        chat_id: &str,
        stream_id: &str,
        content: &str,
        finish: bool,
    ) -> Result<(), GatewayError> {
        self.inner
            .send_native_stream_chunk(chat_id, stream_id, content, finish)
            .await
    }

    fn is_running(&self) -> bool {
        self.inner.is_running()
    }

    fn platform_name(&self) -> &str {
        self.inner.platform_name()
    }
}
