//! P2-9 Forum routing and outbound media filenames.

#![cfg(feature = "discord")]

use hermes_gateway::platforms::discord::{
    is_forum_channel_type, outbound_upload_name, DiscordAdapter, DiscordConfig,
};
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[test]
fn outbound_upload_name_gif_uses_animation_filename() {
    let (name, mime) = outbound_upload_name("/tmp/photo.GIF");
    assert_eq!(name, "animation.gif");
    assert_eq!(mime, Some("image/gif"));
}

#[test]
fn outbound_upload_name_video_keeps_extension() {
    let (name, mime) = outbound_upload_name("clip.mp4");
    assert_eq!(name, "clip.mp4");
    assert_eq!(mime, Some("video/mp4"));
}

#[test]
fn is_forum_channel_type_detects_15() {
    assert!(is_forum_channel_type(Some(15)));
    assert!(!is_forum_channel_type(Some(0)));
}

#[tokio::test]
async fn forum_send_creates_thread_then_posts_remainder() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path_regex(r"/api/v10/channels/forum1$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "forum1",
            "type": 15
        })))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path_regex(r"/api/v10/channels/forum1/threads$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "thread1",
            "message": { "id": "500" }
        })))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path_regex(r"/api/v10/channels/thread1/messages$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "501",
            "channel_id": "thread1"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let mut config = DiscordConfig::for_test("test-token");
    config.rest_api_base = format!("{}/api/v10", server.uri());
    config.text_batch_split_delay_seconds = 0.0;
    let adapter = DiscordAdapter::new(config).unwrap();
    let content = format!("{} {}", "a".repeat(2000), "tail");
    adapter
        .send_text_with_reply("forum1", &content, None)
        .await
        .expect("forum send");
}
