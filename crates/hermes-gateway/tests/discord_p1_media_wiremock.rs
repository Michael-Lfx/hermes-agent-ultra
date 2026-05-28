//! P1.2 Discord attachment download (wiremock).

#![cfg(feature = "discord")]

use hermes_core::errors::GatewayError;
use hermes_gateway::platforms::discord::download_attachment_bytes;

fn allow_wiremock_private_urls() {
    // SAFETY: integration tests run serially per binary; wiremock binds 127.0.0.1.
    unsafe {
        std::env::set_var("HERMES_ALLOW_PRIVATE_URLS", "true");
    }
}

#[tokio::test]
async fn m01_download_attachment_success() {
    allow_wiremock_private_urls();
    let mock_server = wiremock::MockServer::start().await;
    let body = b"hello attachment";
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/attachments/1/2/photo.png"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_bytes(body))
        .mount(&mock_server)
        .await;

    let url = format!("{}/attachments/1/2/photo.png", mock_server.uri());
    let client = reqwest::Client::new();
    let bytes = download_attachment_bytes(&client, "Bot test-token", &url, 1024)
        .await
        .expect("download ok");
    assert_eq!(bytes, body);
}

#[tokio::test]
async fn m02_download_attachment_401() {
    allow_wiremock_private_urls();
    let mock_server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/attachments/x"))
        .respond_with(wiremock::ResponseTemplate::new(401).set_body_string("Unauthorized"))
        .mount(&mock_server)
        .await;

    let url = format!("{}/attachments/x", mock_server.uri());
    let client = reqwest::Client::new();
    let err = download_attachment_bytes(&client, "Bot bad", &url, 1024)
        .await
        .expect_err("401 should fail");
    match err {
        GatewayError::ConnectionFailed(msg) => assert!(msg.contains("401")),
        other => panic!("unexpected error: {other:?}"),
    }
}

#[tokio::test]
async fn m03_download_attachment_exceeds_max_size() {
    allow_wiremock_private_urls();
    let mock_server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/attachments/big"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_bytes(vec![0u8; 64]))
        .mount(&mock_server)
        .await;

    let url = format!("{}/attachments/big", mock_server.uri());
    let client = reqwest::Client::new();
    let err = download_attachment_bytes(&client, "Bot test", &url, 16)
        .await
        .expect_err("oversized body");
    match err {
        GatewayError::ConnectionFailed(msg) => assert!(msg.contains("exceeds max size")),
        other => panic!("unexpected error: {other:?}"),
    }
}
