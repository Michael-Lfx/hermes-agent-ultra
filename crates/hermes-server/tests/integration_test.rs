//! Integration tests for hermes-server
//!
//! Tests the full HTTP + WebSocket stack with a real server instance.

use std::time::Duration;

use futures_util::SinkExt;
use tokio_stream::StreamExt;

/// Create a test AppState with default config
fn test_state() -> hermes_server::state::AppState {
    let config = hermes_config::GatewayConfig::default();
    let hermes_home = std::env::temp_dir().join("hermes-server-test");
    std::fs::create_dir_all(&hermes_home).unwrap();
    hermes_server::state::AppState::new(config, hermes_home)
}

#[tokio::test]
async fn test_api_status() {
    let state = test_state();
    let app = hermes_server::server::router(state);
    
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    
    let client = reqwest::Client::new();
    let response = client
        .get(format!("http://{}/api/status", addr))
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert!(body.get("version").is_some() || body.get("status").is_some());
}

#[tokio::test]
async fn test_sessions_list() {
    let state = test_state();
    let app = hermes_server::server::router(state.clone());
    
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    
    let client = reqwest::Client::new();
    let token = state.session_token();
    
    // List sessions (should return empty array or existing sessions)
    let list_response = client
        .get(format!("http://{}/api/sessions", addr))
        .header("X-Hermes-Session-Token", token)
        .send()
        .await
        .unwrap();
    
    assert_eq!(list_response.status(), 200);
    let list_body: serde_json::Value = list_response.json().await.unwrap();
    // Default format is array (Desktop expects this)
    assert!(list_body.is_array());
    let sessions = list_body.as_array().unwrap();
    // May be empty if no sessions exist
    assert!(sessions.is_empty() || sessions[0].get("id").is_some());
}

#[tokio::test]
async fn test_websocket_jsonrpc() {
    use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};
    
    let state = test_state();
    let token = state.session_token().to_string();
    let app = hermes_server::server::router(state);
    
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    
    // Give server time to start
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    let ws_url = format!("ws://{}/api/ws?token={}", addr, token);
    let (mut ws_stream, _) = connect_async(ws_url).await.unwrap();
    
    // Read gateway.ready
    let msg = ws_stream.next().await.unwrap().unwrap();
    if let WsMessage::Text(text) = msg {
        let event: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(event["method"], "event");
        assert_eq!(event["params"]["type"], "gateway.ready");
    }
    
    // Send session.create
    let create_req = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "session.create",
        "params": {
            "cols": 120,
            "cwd": "/tmp"
        },
        "id": "test-1"
    });
    ws_stream.send(WsMessage::Text(create_req.to_string())).await.unwrap();
    
    // Read response
    let msg = ws_stream.next().await.unwrap().unwrap();
    if let WsMessage::Text(text) = msg {
        let response: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!(response.get("result").is_some());
        assert!(response["result"]["session_id"].is_string());
    }
    
    ws_stream.close(None).await.unwrap();
}

#[tokio::test]
async fn test_gateway_control() {
    let state = test_state();
    let token = state.session_token().to_string();
    let app = hermes_server::server::router(state);
    
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    
    let client = reqwest::Client::new();
    
    // Test /api/gateway/start
    let response = client
        .post(format!("http://{}/api/gateway/start", addr))
        .header("X-Hermes-Session-Token", &token)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert!(body.get("status").is_some());
    
    // Test /api/gateway/stop
    let response = client
        .post(format!("http://{}/api/gateway/stop", addr))
        .header("X-Hermes-Session-Token", &token)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["status"], "stopped");
    
    // Test /api/gateway/restart
    let response = client
        .post(format!("http://{}/api/gateway/restart", addr))
        .header("X-Hermes-Session-Token", &token)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["status"], "restart_requested");
}
