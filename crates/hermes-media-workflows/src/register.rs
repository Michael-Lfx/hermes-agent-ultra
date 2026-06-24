//! Register Flowy media backends and workflow tools into the tool registry.

use std::sync::Arc;

use hermes_config::{GatewayConfig, flowy_media_exposed};
use hermes_core::ToolHandler;
use hermes_tools::ToolRegistry;
use hermes_tools::{ImageGenerateHandler, VideoGenerateHandler};

use crate::backends::FlowyMediaServices;
use crate::backends::flowy_image::FlowyImageGenBackend;
use crate::backends::flowy_video::FlowyVideoGenBackend;
use crate::tools::{MediaWorkflowPlanHandler, MediaWorkflowRunHandler, MediaWorkflowStatusHandler};
use crate::workflows::store::WorkflowRunStore;

fn flowy_media_check_fn() -> Arc<dyn Fn() -> bool + Send + Sync> {
    Arc::new(|| hermes_config::flowy_media_exposed_from_disk())
}

/// Wire Flowy image/video backends and workflow tools when server login is available.
pub fn wire_flowy_media(
    registry: &ToolRegistry,
    config: &GatewayConfig,
    hermes_home: &std::path::Path,
) {
    if !flowy_media_exposed(config) {
        tracing::debug!(
            provider = %config.media.provider,
            server_base_url = %config.server.base_url,
            "Flowy media wiring skipped (provider != flowy or server.base_url missing)"
        );
        return;
    }

    let Some(services) = FlowyMediaServices::try_new(config, hermes_home) else {
        tracing::warn!("Flowy media services could not be initialized");
        return;
    };

    let check = flowy_media_check_fn();
    register_overwrite(
        registry,
        "image_gen",
        Arc::new(ImageGenerateHandler::new(Arc::new(
            FlowyImageGenBackend::new(services.clone()),
        ))),
        "🎨",
        Arc::clone(&check),
    );

    register_overwrite(
        registry,
        "video_gen",
        Arc::new(VideoGenerateHandler::new(Arc::new(
            FlowyVideoGenBackend::new(services.clone()),
        ))),
        "🎞️",
        Arc::clone(&check),
    );

    if !config.media.workflows.enabled {
        tracing::info!("Flowy image/video backends registered (workflows disabled)");
        return;
    }

    let store = Arc::new(WorkflowRunStore::new());
    register_overwrite(
        registry,
        "media_workflow",
        Arc::new(MediaWorkflowPlanHandler::new(config.media.clone())),
        "🎬",
        Arc::clone(&check),
    );
    register_overwrite(
        registry,
        "media_workflow",
        Arc::new(MediaWorkflowRunHandler::new(services, store.clone())),
        "🎬",
        Arc::clone(&check),
    );
    register_overwrite(
        registry,
        "media_workflow",
        Arc::new(MediaWorkflowStatusHandler::new(store)),
        "🎬",
        Arc::clone(&check),
    );

    tracing::info!("Flowy media backends and workflow tools registered");
}

fn register_overwrite(
    registry: &ToolRegistry,
    toolset: &str,
    handler: Arc<dyn ToolHandler>,
    emoji: &str,
    check_fn: Arc<dyn Fn() -> bool + Send + Sync>,
) {
    let schema = handler.schema();
    let name = schema.name.clone();
    let desc = schema.description.clone();
    registry.register(
        name,
        toolset,
        schema,
        handler,
        check_fn,
        vec![],
        true,
        desc,
        emoji,
        None,
    );
}
