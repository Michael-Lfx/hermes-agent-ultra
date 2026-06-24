//! Flowy server media generation wiring (image/video + workflows).
//!
//! Mirrors [`crate::moa_wiring`] — registers real backends after built-in tool catalog.

use std::path::Path;

use hermes_config::GatewayConfig;
use hermes_tools::ToolRegistry;

/// Register Flowy image/video backends and optional workflow tools.
pub fn wire_flowy_media_backends(
    registry: &ToolRegistry,
    config: &GatewayConfig,
    hermes_home: &Path,
) {
    hermes_media_workflows::wire_flowy_media(registry, config, hermes_home);
}

/// Reload `config.yaml` and re-wire Flowy media tools (gateway hot refresh after `hermes media init`).
pub fn refresh_flowy_media_backends(registry: &ToolRegistry, config_dir: Option<&str>) {
    let config = match hermes_config::load_config(config_dir) {
        Ok(cfg) => cfg,
        Err(err) => {
            tracing::debug!(error = %err, "media refresh: config load failed");
            return;
        }
    };
    wire_flowy_media_backends(registry, &config, &hermes_config::hermes_home());
}
