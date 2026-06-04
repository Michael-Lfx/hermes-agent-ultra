//! WhatsApp Business Cloud API adapter (optional enterprise backend).
//!
//! Enabled with the `whatsapp-cloud` feature. Default WhatsApp integration uses
//! Baileys via [`super::whatsapp`].

use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;
use tracing::{debug, info};

use hermes_core::errors::GatewayError;
use hermes_core::traits::{ParseMode, PlatformAdapter};

use crate::adapter::{AdapterProxyConfig, BasePlatformAdapter};

const WHATSAPP_API_BASE: &str = "https://graph.facebook.com/v18.0";

#[derive(Debug, Clone)]
pub struct IncomingWhatsAppCloudMessage {
    pub from: String,
    pub message_id: String,
    pub text: String,
    pub message_type: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhatsAppCloudConfig {
    pub token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phone_number_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub business_account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify_token: Option<String>,
    #[serde(default)]
    pub proxy: AdapterProxyConfig,
}

pub struct WhatsAppCloudAdapter {
    base: BasePlatformAdapter,
    config: WhatsAppCloudConfig,
    client: Client,
    stop_signal: Arc<Notify>,
}

impl WhatsAppCloudAdapter {
    pub fn new(config: WhatsAppCloudConfig) -> Result<Self, GatewayError> {
        let base = BasePlatformAdapter::new(&config.token).with_proxy(config.proxy.clone());
        base.validate_token()?;
        let client = base.build_client()?;
        Ok(Self {
            base,
            config,
            client,
            stop_signal: Arc::new(Notify::new()),
        })
    }

    pub fn verify_webhook(
        mode: &str,
        token: &str,
        challenge: &str,
        verify_token: &str,
    ) -> Option<String> {
        if mode == "subscribe" && token == verify_token {
            Some(challenge.to_string())
        } else {
            None
        }
    }

    pub fn parse_webhook_event(body: &serde_json::Value) -> Vec<IncomingWhatsAppCloudMessage> {
        let mut messages = Vec::new();
        let Some(entries) = body.get("entry").and_then(|v| v.as_array()) else {
            return messages;
        };
        for entry in entries {
            let Some(changes) = entry.get("changes").and_then(|v| v.as_array()) else {
                continue;
            };
            for change in changes {
                let Some(value) = change.get("value") else {
                    continue;
                };
                let Some(msgs) = value.get("messages").and_then(|v| v.as_array()) else {
                    continue;
                };
                for msg in msgs {
                    messages.push(IncomingWhatsAppCloudMessage {
                        from: msg
                            .get("from")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        message_id: msg
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        message_type: msg
                            .get("type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("text")
                            .to_string(),
                        timestamp: msg
                            .get("timestamp")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        text: msg
                            .get("text")
                            .and_then(|t| t.get("body"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                    });
                }
            }
        }
        messages
    }

    async fn send_text(&self, to: &str, text: &str) -> Result<(), GatewayError> {
        let phone_id = self.config.phone_number_id.as_deref().ok_or_else(|| {
            GatewayError::SendFailed("phone_number_id not configured".into())
        })?;
        let url = format!("{}/{}/messages", WHATSAPP_API_BASE, phone_id);
        let body = serde_json::json!({
            "messaging_product": "whatsapp",
            "to": to,
            "type": "text",
            "text": { "body": text }
        });
        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.token))
            .json(&body)
            .send()
            .await
            .map_err(|e| GatewayError::SendFailed(format!("WhatsApp send failed: {e}")))?;
        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(GatewayError::SendFailed(format!("WhatsApp API error: {text}")));
        }
        Ok(())
    }

    async fn send_media(
        &self,
        to: &str,
        media_type: &str,
        media_url: &str,
        caption: Option<&str>,
    ) -> Result<(), GatewayError> {
        let phone_id = self.config.phone_number_id.as_deref().ok_or_else(|| {
            GatewayError::SendFailed("phone_number_id not configured".into())
        })?;
        let url = format!("{}/{}/messages", WHATSAPP_API_BASE, phone_id);
        let body = build_link_media_body(to, media_type, media_url, caption);
        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.token))
            .json(&body)
            .send()
            .await
            .map_err(|e| GatewayError::SendFailed(format!("WhatsApp media send failed: {e}")))?;
        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(GatewayError::SendFailed(format!("WhatsApp API error: {text}")));
        }
        Ok(())
    }
}

pub fn build_link_media_body(
    to: &str,
    media_type: &str,
    media_url: &str,
    caption: Option<&str>,
) -> serde_json::Value {
    let mut media_obj = serde_json::json!({ "link": media_url });
    if let Some(cap) = caption.map(str::trim).filter(|s| !s.is_empty()) {
        media_obj["caption"] = serde_json::Value::String(cap.to_string());
    }
    serde_json::json!({
        "messaging_product": "whatsapp",
        "to": to,
        "type": media_type,
        media_type: media_obj
    })
}

#[cfg(test)]
mod tests {
    use super::build_link_media_body;

    #[test]
    fn build_link_media_body_with_caption() {
        let body = build_link_media_body(
            "15551234567",
            "image",
            "https://example.com/preview.png",
            Some("Status update"),
        );
        assert_eq!(body["messaging_product"], "whatsapp");
        assert_eq!(body["image"]["caption"], "Status update");
    }

    #[test]
    fn build_link_media_body_omits_blank_caption() {
        let body = build_link_media_body(
            "15551234567",
            "image",
            "https://example.com/preview.png",
            Some("   "),
        );
        assert!(body["image"]["caption"].is_null());
    }
}

#[async_trait]
impl PlatformAdapter for WhatsAppCloudAdapter {
    async fn start(&self) -> Result<(), GatewayError> {
        info!("WhatsApp Cloud adapter starting");
        self.base.mark_running();
        Ok(())
    }

    async fn stop(&self) -> Result<(), GatewayError> {
        self.base.mark_stopped();
        self.stop_signal.notify_one();
        Ok(())
    }

    async fn send_message(
        &self,
        chat_id: &str,
        text: &str,
        _parse_mode: Option<ParseMode>,
    ) -> Result<(), GatewayError> {
        self.send_text(chat_id, text).await
    }

    async fn edit_message(
        &self,
        _chat_id: &str,
        _message_id: &str,
        _text: &str,
    ) -> Result<(), GatewayError> {
        debug!("WhatsApp Cloud API does not support message editing");
        Ok(())
    }

    async fn send_file(
        &self,
        chat_id: &str,
        file_path: &str,
        caption: Option<&str>,
    ) -> Result<(), GatewayError> {
        use crate::platforms::helpers::{media_category, mime_from_extension};

        let phone_id = self.config.phone_number_id.as_deref().ok_or_else(|| {
            GatewayError::SendFailed("phone_number_id not configured".into())
        })?;
        let path = std::path::Path::new(file_path);
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let mime = mime_from_extension(ext);
        let file_bytes = tokio::fs::read(file_path)
            .await
            .map_err(|e| GatewayError::SendFailed(format!("Failed to read file: {e}")))?;
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
        let upload_url = format!("{}/{}/media", WHATSAPP_API_BASE, phone_id);
        let part = reqwest::multipart::Part::bytes(file_bytes)
            .file_name(file_name.to_string())
            .mime_str(mime)
            .map_err(|e| GatewayError::SendFailed(format!("MIME error: {e}")))?;
        let form = reqwest::multipart::Form::new()
            .text("messaging_product", "whatsapp")
            .part("file", part);
        let resp = self
            .client
            .post(&upload_url)
            .header("Authorization", format!("Bearer {}", self.config.token))
            .multipart(form)
            .send()
            .await
            .map_err(|e| GatewayError::SendFailed(format!("WhatsApp media upload failed: {e}")))?;
        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(GatewayError::SendFailed(format!(
                "WhatsApp media upload error: {text}"
            )));
        }
        let result: serde_json::Value = resp.json().await.map_err(|e| {
            GatewayError::SendFailed(format!("WhatsApp upload parse failed: {e}"))
        })?;
        let media_id = result.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let media_type = match media_category(ext) {
            "image" => "image",
            "video" => "video",
            "audio" => "audio",
            _ => "document",
        };
        let send_url = format!("{}/{}/messages", WHATSAPP_API_BASE, phone_id);
        let mut media_obj = serde_json::json!({ "id": media_id });
        if let Some(cap) = caption {
            media_obj["caption"] = serde_json::Value::String(cap.to_string());
        }
        if media_type == "document" {
            media_obj["filename"] = serde_json::Value::String(file_name.to_string());
        }
        let body = serde_json::json!({
            "messaging_product": "whatsapp",
            "to": chat_id,
            "type": media_type,
            media_type: media_obj
        });
        let resp = self
            .client
            .post(&send_url)
            .header("Authorization", format!("Bearer {}", self.config.token))
            .json(&body)
            .send()
            .await
            .map_err(|e| GatewayError::SendFailed(format!("WhatsApp media send failed: {e}")))?;
        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(GatewayError::SendFailed(format!(
                "WhatsApp media send error: {text}"
            )));
        }
        Ok(())
    }

    async fn send_image_url(
        &self,
        chat_id: &str,
        image_url: &str,
        caption: Option<&str>,
    ) -> Result<(), GatewayError> {
        self.send_media(chat_id, "image", image_url, caption).await
    }

    fn is_running(&self) -> bool {
        self.base.is_running()
    }

    fn platform_name(&self) -> &str {
        "whatsapp"
    }
}
