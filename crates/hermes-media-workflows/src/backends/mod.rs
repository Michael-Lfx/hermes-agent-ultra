//! Shared Flowy media service handle.

use std::sync::Arc;

use hermes_config::{GatewayConfig, MediaGenConfig};
use hermes_server_client::{FlowyApiClient, ServerSession};

/// Runtime handle for Flowy image/video APIs (login token + config).
#[derive(Clone)]
pub struct FlowyMediaServices {
    pub api: Arc<FlowyApiClient>,
    pub session: ServerSession,
    pub media: MediaGenConfig,
}

impl FlowyMediaServices {
    pub fn try_new(config: &GatewayConfig, hermes_home: &std::path::Path) -> Option<Self> {
        if !config.media.uses_flowy() || !config.server.api_ready() {
            return None;
        }
        let api = FlowyApiClient::new(&config.server).ok()?;
        let session = ServerSession::from_config(&config.server, hermes_home);
        Some(Self {
            api: Arc::new(api),
            session,
            media: config.media.clone(),
        })
    }

    pub async fn is_authenticated(&self) -> bool {
        self.session
            .access_token()
            .await
            .ok()
            .flatten()
            .is_some_and(|t| !t.trim().is_empty())
    }

    pub async fn default_image_model(&self) -> Result<String, hermes_core::ToolError> {
        let configured = self.media.image.model.trim();
        if !configured.is_empty() {
            return Ok(configured.to_string());
        }
        let token = self.require_token().await?;
        let _ = token;
        let models = self
            .api
            .get_available_models_claw(
                &self.session,
                Some(hermes_server_client::flowy::MODEL_CATEGORY_IMAGE),
            )
            .await
            .map_err(map_server_err)?;
        models.cloud.first().map(|m| m.id.clone()).ok_or_else(|| {
            hermes_core::ToolError::ExecutionFailed(
                "no image models available — check login and credits".into(),
            )
        })
    }

    pub async fn default_video_model(&self) -> Result<String, hermes_core::ToolError> {
        let configured = self.media.video.model.trim();
        if !configured.is_empty() {
            return Ok(configured.to_string());
        }
        let token = self.require_token().await?;
        let _ = token;
        let models = self
            .api
            .get_available_models_claw(
                &self.session,
                Some(hermes_server_client::flowy::MODEL_CATEGORY_VIDEO),
            )
            .await
            .map_err(map_server_err)?;
        models.cloud.first().map(|m| m.id.clone()).ok_or_else(|| {
            hermes_core::ToolError::ExecutionFailed(
                "no video models available — check login and credits".into(),
            )
        })
    }

    pub async fn require_token(&self) -> Result<String, hermes_core::ToolError> {
        self.session
            .access_token()
            .await
            .map_err(|e| hermes_core::ToolError::ExecutionFailed(e.to_string()))?
            .filter(|t| !t.trim().is_empty())
            .ok_or_else(|| {
                hermes_core::ToolError::ExecutionFailed(
                    "not logged in — run `hermes server login` first".into(),
                )
            })
    }
}

pub fn map_server_err(err: hermes_server_client::ServerClientError) -> hermes_core::ToolError {
    hermes_core::ToolError::ExecutionFailed(err.to_string())
}

pub mod flowy_image;
pub mod flowy_video;
