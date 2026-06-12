//! External service integration tool registrations.
//!
//! Preconditions:
//! - Spotify: HERMES_SPOTIFY_ACCESS_TOKEN or SPOTIFY_ACCESS_TOKEN or HERMES_AUTH_FILE.
//! - Home Assistant: HASS_URL and HASS_TOKEN.
//! - Feishu: FEISHU_APP_ID and FEISHU_APP_SECRET (skipped when absent).
//! - Messaging: no env requirements (uses signal backend).

use std::sync::Arc;

use super::{RegistryContext, reg};

pub fn register(ctx: &RegistryContext<'_>) {
    register_spotify(ctx);
    register_homeassistant(ctx);
    register_messaging(ctx);
    register_feishu(ctx);
}

fn register_spotify(ctx: &RegistryContext<'_>) {
    let backend: Arc<dyn crate::tools::spotify::SpotifyBackend> =
        match crate::backends::spotify::SpotifyWebApiBackend::from_env_or_auth_store() {
            Ok(backend) => Arc::new(backend),
            Err(_) => Arc::new(crate::backends::spotify::SpotifyWebApiBackend::unconfigured()),
        };
    let deps = vec![
        "HERMES_SPOTIFY_ACCESS_TOKEN".into(),
        "SPOTIFY_ACCESS_TOKEN".into(),
        "HERMES_AUTH_FILE".into(),
    ];
    for (tool, emoji) in [
        (crate::tools::spotify::SpotifyTool::Playback, "🎵"),
        (crate::tools::spotify::SpotifyTool::Devices, "🔈"),
        (crate::tools::spotify::SpotifyTool::Queue, "📻"),
        (crate::tools::spotify::SpotifyTool::Search, "🔎"),
        (crate::tools::spotify::SpotifyTool::Playlists, "📚"),
        (crate::tools::spotify::SpotifyTool::Albums, "💿"),
        (crate::tools::spotify::SpotifyTool::Library, "❤️"),
    ] {
        reg(
            ctx,
            "spotify",
            Arc::new(crate::tools::spotify::SpotifyHandler::new(
                tool,
                backend.clone(),
            )),
            emoji,
            deps.clone(),
        );
    }
}

fn register_homeassistant(ctx: &RegistryContext<'_>) {
    let ha_backend: Arc<dyn crate::tools::homeassistant::HomeAssistantBackend> =
        match crate::backends::homeassistant::HaRestBackend::from_env() {
            Ok(b) => Arc::new(b),
            Err(_) => Arc::new(crate::backends::homeassistant::HaRestBackend::new(
                String::new(),
                String::new(),
            )),
        };
    let deps = vec!["HASS_URL".into(), "HASS_TOKEN".into()];
    reg(
        ctx,
        "homeassistant",
        Arc::new(crate::tools::homeassistant::HaListEntitiesHandler::new(
            ha_backend.clone(),
        )),
        "🏠",
        deps.clone(),
    );
    reg(
        ctx,
        "homeassistant",
        Arc::new(crate::tools::homeassistant::HaGetStateHandler::new(
            ha_backend.clone(),
        )),
        "🏠",
        deps.clone(),
    );
    reg(
        ctx,
        "homeassistant",
        Arc::new(crate::tools::homeassistant::HaListServicesHandler::new(
            ha_backend.clone(),
        )),
        "🏠",
        deps.clone(),
    );
    reg(
        ctx,
        "homeassistant",
        Arc::new(crate::tools::homeassistant::HaCallServiceHandler::new(
            ha_backend,
        )),
        "🏠",
        deps,
    );
}

fn register_messaging(ctx: &RegistryContext<'_>) {
    reg(
        ctx,
        "messaging",
        Arc::new(crate::tools::messaging::SendMessageHandler::new(Arc::new(
            crate::backends::messaging::SignalMessagingBackend::new(),
        ))),
        "💬",
        vec![],
    );
}

fn register_feishu(ctx: &RegistryContext<'_>) {
    if let Some(feishu_client) = crate::tools::feishu::FeishuApiClient::from_env() {
        let feishu = Arc::new(feishu_client);
        let feishu_deps = vec!["FEISHU_APP_ID".into()];
        reg(
            ctx,
            "feishu",
            Arc::new(crate::tools::feishu::FeishuCalendarHandler::new(
                feishu.clone(),
            )),
            "📅",
            feishu_deps.clone(),
        );
        reg(
            ctx,
            "feishu",
            Arc::new(crate::tools::feishu::FeishuDocsHandler::new(feishu.clone())),
            "📄",
            feishu_deps.clone(),
        );
        reg(
            ctx,
            "feishu",
            Arc::new(crate::tools::feishu::FeishuTaskHandler::new(feishu.clone())),
            "✅",
            feishu_deps.clone(),
        );
        reg(
            ctx,
            "feishu",
            Arc::new(crate::tools::feishu::FeishuChatHistoryHandler::new(feishu)),
            "💬",
            feishu_deps,
        );
    } else {
        tracing::debug!("Skipping feishu tools — FEISHU_APP_ID / FEISHU_APP_SECRET not set");
    }
}
