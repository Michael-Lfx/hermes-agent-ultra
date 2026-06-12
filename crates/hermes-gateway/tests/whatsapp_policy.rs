#![cfg(feature = "whatsapp")]

//! WhatsApp inbound policy tests (subset of upstream group gating).

use hermes_gateway::platforms::whatsapp::{WhatsAppConfig, WhatsAppPolicy};
use serde_json::json;

fn policy_with(mut f: impl FnOnce(&mut WhatsAppConfig)) -> WhatsAppPolicy {
    let mut cfg = WhatsAppConfig::default();
    f(&mut cfg);
    WhatsAppPolicy::from_config(&cfg)
}

#[test]
fn dm_disabled() {
    let p = policy_with(|c| c.dm_policy = "disabled".into());
    let data = json!({"chatId": "1@s.whatsapp.net", "senderId": "1@s.whatsapp.net", "isGroup": false, "body": "hi"});
    assert!(!p.should_process_message(&data));
}

#[test]
fn dm_allowlist() {
    let p = policy_with(|c| {
        c.dm_policy = "allowlist".into();
        c.allow_from = vec!["15551234567".into()];
    });
    let ok = json!({"chatId": "15551234567@s.whatsapp.net", "senderId": "15551234567@s.whatsapp.net", "isGroup": false, "body": "hi"});
    let bad = json!({"chatId": "999@s.whatsapp.net", "senderId": "999@s.whatsapp.net", "isGroup": false, "body": "hi"});
    assert!(p.should_process_message(&ok));
    assert!(!p.should_process_message(&bad));
}

#[test]
fn drops_status_broadcast() {
    let p = policy_with(|_| {});
    let data =
        json!({"chatId": "status@broadcast", "senderId": "x", "isGroup": false, "body": "x"});
    assert!(!p.should_process_message(&data));
}
