#![cfg(feature = "whatsapp")]

//! WhatsApp display tier tests (TIER_MEDIUM).

use hermes_gateway::display_config::resolve_display_setting;

#[test]
fn whatsapp_tool_progress_is_new() {
    assert_eq!(
        resolve_display_setting(None, "whatsapp", "tool_progress", None),
        Some("new".to_string())
    );
}

#[test]
fn whatsapp_streaming_follows_global() {
    assert_eq!(
        resolve_display_setting(None, "whatsapp", "streaming", None),
        None
    );
}
