#![cfg(feature = "whatsapp")]

//! WhatsApp connect lifecycle tests.

use hermes_gateway::PlatformAdapter;
use hermes_gateway::platforms::whatsapp::{WhatsAppAdapter, WhatsAppConfig, is_paired};
use tempfile::TempDir;

#[tokio::test]
async fn connect_fails_without_paired_marker() {
    let dir = TempDir::new().unwrap();
    let session = dir.path().join("session");
    std::fs::create_dir_all(&session).unwrap();

    let mut cfg = WhatsAppConfig::default();
    cfg.session_path = Some(session.to_string_lossy().into_owned());
    let adapter = WhatsAppAdapter::new(cfg).unwrap();
    let err = adapter.start().await.expect_err("missing paired marker");
    assert!(
        err.to_string().contains("paired") || err.to_string().contains("not paired"),
        "unexpected error: {err}"
    );
}

#[test]
fn paired_marker_detected() {
    let dir = TempDir::new().unwrap();
    let session = dir.path().join("session");
    assert!(!is_paired(&session));
    hermes_gateway::platforms::whatsapp::mark_paired(&session).unwrap();
    assert!(is_paired(&session));
}
