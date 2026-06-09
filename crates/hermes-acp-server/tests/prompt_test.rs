//! Integration tests for prompt execution and streaming events.
//!
//! Verifies: mock executor produces correct StreamEvents, event bridge
//! formats them as valid NDJSON session/update notifications, and the
//! full prompt lifecycle (prompt -> streaming chunks -> stopReason) works.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use hermes_acp_server::{
    AcpPipeServer, AcpServerConfig, AgentInfo,
    PipeSession, PromptExecutor, PromptResult, StreamContent, StreamEvent,
};
use hermes_acp::protocol::StopReason;

// ---------------------------------------------------------------------------
// Mock executor
// ---------------------------------------------------------------------------

struct MockExecutor {
    chunks: Vec<String>,
}

#[async_trait]
impl PromptExecutor for MockExecutor {
    async fn execute(
        &self,
        _session: &PipeSession,
        _prompt_text: &str,
        _history: &[Value],
        event_tx: tokio::sync::mpsc::Sender<StreamEvent>,
    ) -> Result<PromptResult, String> {
        for chunk in &self.chunks {
            let _ = event_tx.send(StreamEvent::AgentMessageChunk {
                content: StreamContent::Text {
                    text: chunk.clone(),
                },
            }).await;
        }
        Ok(PromptResult {
            stop_reason: StopReason::EndTurn,
            usage: None,
        })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn test_pipe(name: &str) -> String {
    format!(r"\\.\pipe\test-acp-prompt-{}", name)
}

async fn roundtrip(
    client: &mut tokio::net::windows::named_pipe::NamedPipeClient,
    req: Value,
) -> Value {
    let mut line = serde_json::to_string(&req).unwrap();
    line.push('\n');
    client.write_all(line.as_bytes()).await.unwrap();
    client.flush().await.unwrap();

    let mut buf = vec![0u8; 4096];
    let mut total = String::new();
    loop {
        let n = client.read(&mut buf).await.unwrap();
        if n == 0 { break; }
        total.push_str(&String::from_utf8_lossy(&buf[..n]));
        if total.contains('\n') { break; }
    }
    let line = total.split('\n').next().unwrap_or("");
    serde_json::from_str::<Value>(line).unwrap()
}

async fn send_request(
    client: &mut tokio::net::windows::named_pipe::NamedPipeClient,
    req: Value,
) {
    let mut line = serde_json::to_string(&req).unwrap();
    line.push('\n');
    client.write_all(line.as_bytes()).await.unwrap();
    client.flush().await.unwrap();
}

/// Read NDJSON lines with a timeout. Returns all parsed messages.
/// The server sends: 1) response line, 2) streaming notification lines.
/// We need to read all of them.
async fn read_all_ndjson(
    client: &mut tokio::net::windows::named_pipe::NamedPipeClient,
    timeout: std::time::Duration,
) -> Vec<Value> {
    let start = std::time::Instant::now();
    let mut messages = Vec::new();
    let mut buffer = vec![0u8; 16384];
    let mut leftover = String::new();

    loop {
        if start.elapsed() > timeout {
            break; // timeout, return what we have
        }

        // Non-blocking style: try to read with a short timeout
        match tokio::time::timeout(
            std::time::Duration::from_millis(200),
            client.read(&mut buffer),
        ).await {
            Ok(Ok(n)) if n > 0 => {
                leftover.push_str(&String::from_utf8_lossy(&buffer[..n]));
                // Parse all complete lines
                while let Some(pos) = leftover.find('\n') {
                    let line = leftover[..pos].trim().to_string();
                    leftover = leftover[pos + 1..].to_string();
                    if line.is_empty() { continue; }
                    if let Ok(msg) = serde_json::from_str::<Value>(&line) {
                        messages.push(msg);
                    }
                }
            }
            Ok(Ok(_)) => break, // n == 0, connection closed
            Ok(Err(_)) => break,
            Err(_) => break, // timeout on this read, we've waited enough
        }
    }

    messages
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn prompt_streams_chunks_then_stop_reason() {
    let pipe = test_pipe("stream-chunks");
    let executor = Arc::new(MockExecutor {
        chunks: vec![
            "Hello".to_string(),
            " world".to_string(),
            "!".to_string(),
        ],
    });
    let config = AcpServerConfig {
        pipe_path: pipe.clone(),
        max_connections: 2,
        agent_info: AgentInfo {
            name: "test-agent".to_string(),
            title: "Test".to_string(),
            version: "0.0.1".to_string(),
        },
        executor,
    };

    let server = AcpPipeServer::new(config).unwrap();
    let server_arc = Arc::new(server);
    let srv = server_arc.clone();
    let handle = tokio::spawn(async move { srv.run().await.unwrap() });

    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    let mut client = tokio::net::windows::named_pipe::ClientOptions::new()
        .open(&pipe).unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // initialize
    let resp = roundtrip(&mut client, json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "clientInfo": { "name": "test", "title": "T", "version": "1" } }
    })).await;
    assert_eq!(resp["id"], 1);
    assert!(resp.get("result").is_some());

    // session/new
    let resp = roundtrip(&mut client, json!({
        "jsonrpc": "2.0", "id": 2, "method": "session/new", "params": {}
    })).await;
    assert_eq!(resp["id"], 2);
    assert!(resp["result"]["sessionId"].is_string());

    // session/prompt -- read all responses
    send_request(&mut client, json!({
        "jsonrpc": "2.0", "id": 3, "method": "session/prompt",
        "params": { "prompt": [{ "type": "text", "text": "say hello" }] }
    })).await;

    let messages = read_all_ndjson(&mut client, std::time::Duration::from_secs(3)).await;

    // Separate the final response (has "id") from notifications (has "method")
    let final_resp = messages.iter()
        .find(|m| m.get("id").and_then(|v| v.as_u64()) == Some(3))
        .expect("should have response with id=3")
        .clone();
    let notifications: Vec<&Value> = messages.iter()
        .filter(|m| m.get("method").map(|v| v.as_str() == Some("session/update")).unwrap_or(false))
        .collect();

    // Verify final response
    assert_eq!(final_resp["id"], 3);
    assert_eq!(final_resp["result"]["stopReason"], "end_turn");

    // Verify streaming notifications
    // Note: current architecture sends response first, then streams events.
    // With a 256-buffer channel, all events are already queued, so bridge_events
    // flushes them immediately after the response. Whether notifications arrive
    // before or after the response depends on timing -- both are valid.
    assert!(!notifications.is_empty(), "expected streaming notifications, got {} messages total", messages.len());

    for notif in &notifications {
        assert_eq!((*notif)["jsonrpc"], "2.0");
        assert_eq!((*notif)["method"], "session/update");
        assert!((*notif)["params"]["sessionId"].is_string());

        let update = &(*notif)["params"]["update"];
        let update_type = update["sessionUpdate"].as_str().unwrap();
        assert!(update_type == "agent_message_chunk" || update_type == "agent_thought_chunk");
        assert_eq!(update["content"]["type"], "text");
        assert!(update["content"]["text"].is_string());
    }

    // Verify chunk content
    let texts: Vec<&str> = notifications.iter()
        .filter_map(|n| {
            let u = &(*n)["params"]["update"];
            if u["sessionUpdate"] == "agent_message_chunk" {
                u["content"]["text"].as_str()
            } else {
                None
            }
        })
        .collect();

    assert_eq!(texts.len(), 3);
    assert_eq!(texts.join(""), "Hello world!");

    server_arc.shutdown();
    let _ = tokio::time::timeout(std::time::Duration::from_secs(3), handle).await;
}

#[tokio::test]
async fn prompt_without_session_returns_error() {
    let pipe = test_pipe("no-session");
    let config = AcpServerConfig {
        pipe_path: pipe.clone(),
        max_connections: 2,
        agent_info: AgentInfo {
            name: "test-agent".to_string(),
            title: "Test".to_string(),
            version: "0.0.1".to_string(),
        },
        executor: Arc::new(MockExecutor { chunks: vec![] }),
    };

    let server = AcpPipeServer::new(config).unwrap();
    let server_arc = Arc::new(server);
    let srv = server_arc.clone();
    let handle = tokio::spawn(async move { srv.run().await.unwrap() });

    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    let mut client = tokio::net::windows::named_pipe::ClientOptions::new()
        .open(&pipe).unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // initialize only (skip session/new)
    let _ = roundtrip(&mut client, json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "clientInfo": { "name": "test", "title": "T", "version": "1" } }
    })).await;

    // prompt without session -> error
    let resp = roundtrip(&mut client, json!({
        "jsonrpc": "2.0", "id": 2, "method": "session/prompt",
        "params": { "prompt": [{ "type": "text", "text": "hello" }] }
    })).await;

    assert!(resp.get("error").is_some(), "expected error for prompt without session");

    server_arc.shutdown();
    let _ = tokio::time::timeout(std::time::Duration::from_secs(3), handle).await;
}

#[tokio::test]
async fn empty_prompt_returns_error() {
    let pipe = test_pipe("empty-prompt");
    let config = AcpServerConfig {
        pipe_path: pipe.clone(),
        max_connections: 2,
        agent_info: AgentInfo {
            name: "test-agent".to_string(),
            title: "Test".to_string(),
            version: "0.0.1".to_string(),
        },
        executor: Arc::new(MockExecutor { chunks: vec![] }),
    };

    let server = AcpPipeServer::new(config).unwrap();
    let server_arc = Arc::new(server);
    let srv = server_arc.clone();
    let handle = tokio::spawn(async move { srv.run().await.unwrap() });

    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    let mut client = tokio::net::windows::named_pipe::ClientOptions::new()
        .open(&pipe).unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // initialize + session/new
    let _ = roundtrip(&mut client, json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "clientInfo": { "name": "test", "title": "T", "version": "1" } }
    })).await;
    let _ = roundtrip(&mut client, json!({
        "jsonrpc": "2.0", "id": 2, "method": "session/new", "params": {}
    })).await;

    // Empty prompt
    let resp = roundtrip(&mut client, json!({
        "jsonrpc": "2.0", "id": 3, "method": "session/prompt",
        "params": { "prompt": [] }
    })).await;

    assert!(resp.get("error").is_some());
    assert_eq!(resp["error"]["code"], -32600);

    server_arc.shutdown();
    let _ = tokio::time::timeout(std::time::Duration::from_secs(3), handle).await;
}
