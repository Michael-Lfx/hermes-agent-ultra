#![cfg(feature = "whatsapp")]

//! WhatsApp session key canonicalization tests.

use hermes_gateway::session::{SessionConfig, SessionManager};

#[test]
fn whatsapp_dm_uses_canonical_identifier() {
    let manager = SessionManager::new(SessionConfig::default());
    let key = manager.compose_session_key_with_dm(
        "whatsapp",
        "15551234567@s.whatsapp.net",
        "15551234567",
        Some(true),
    );
    assert_eq!(key, "whatsapp:15551234567");
}

#[test]
fn whatsapp_group_per_user_canonical_participant() {
    let manager = SessionManager::with_group_isolation(SessionConfig::default(), true);
    let key = manager.compose_session_key_with_dm(
        "whatsapp",
        "120363000000000000@g.us",
        "15551234567@s.whatsapp.net",
        Some(false),
    );
    assert_eq!(key, "whatsapp:120363000000000000@g.us:15551234567");
}
