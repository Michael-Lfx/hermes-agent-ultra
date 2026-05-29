//! P2-11 safe slash command sync (wiremock).

#![cfg(feature = "discord")]

use hermes_gateway::platforms::discord::command_sync::CommandSyncSummary;
use hermes_gateway::platforms::discord::{SlashCommand, DiscordAdapter, DiscordConfig};
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn safe_sync_creates_updates_and_deletes() {
    let server = MockServer::start().await;
    let app_id = "999";

    Mock::given(method("GET"))
        .and(path_regex(r"/api/v10/applications/999/commands$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {
                "id": "11",
                "name": "status",
                "description": "Show Hermes session status",
                "type": 1,
                "options": []
            },
            {
                "id": "12",
                "name": "help",
                "description": "Old help text",
                "type": 1,
                "options": []
            },
            {
                "id": "13",
                "name": "old-command",
                "description": "To be deleted",
                "type": 1,
                "options": []
            }
        ])))
        .mount(&server)
        .await;

    Mock::given(method("PATCH"))
        .and(path_regex(r"/api/v10/applications/999/commands/12$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "12",
            "name": "help",
            "description": "Show available commands",
            "type": 1,
            "options": []
        })))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path_regex(r"/api/v10/applications/999/commands$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "20",
            "name": "model",
            "description": "Switch or show the active model",
            "type": 1,
            "options": []
        })))
        .mount(&server)
        .await;

    Mock::given(method("DELETE"))
        .and(path_regex(r"/api/v10/applications/999/commands/13$"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let mut config = DiscordConfig::for_test("test-token");
    config.rest_api_base = format!("{}/api/v10", server.uri());
    config.application_id = Some(app_id.into());
    let adapter = DiscordAdapter::new(config).unwrap();
    let desired = vec![
        SlashCommand {
            name: "status".into(),
            description: "Show Hermes session status".into(),
            options: None,
            command_type: 1,
        },
        SlashCommand {
            name: "help".into(),
            description: "Show available commands".into(),
            options: None,
            command_type: 1,
        },
        SlashCommand {
            name: "model".into(),
            description: "Switch or show the active model".into(),
            options: None,
            command_type: 1,
        },
    ];

    let summary = adapter
        .inner()
        .safe_sync_slash_commands(&desired)
        .await
        .expect("sync");
    assert_eq!(
        summary,
        CommandSyncSummary {
            total: 3,
            unchanged: 1,
            updated: 1,
            recreated: 0,
            created: 1,
            deleted: 1,
            ..Default::default()
        }
    );
}
