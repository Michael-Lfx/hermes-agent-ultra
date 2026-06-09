//! Smoke tests for ACP Pipe Server.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use hermes_acp_server::{
    AcpPipeServer, AcpServerConfig, AgentInfo,
    PipeSession, PromptExecutor, PromptResult, StreamEvent,
};
use hermes_acp::protocol::StopReason;

fn test_pipe(name: &str) -> String {
    format!(r"\\.\pipe\test-acp-{}", name)
}

fn test_config(pipe_path: String) -> AcpServerConfig {
    AcpServerConfig {
        pipe_path,
        max_connections: 2,
        agent_info: AgentInfo {
            name: "test-agent".to_string(),
            title: "Test Agent".to_string(),
            version: "0.0.1-test".to_string(),
        },
        executor: Arc::new(NoopExecutor),
    }
}

struct NoopExecutor;

#[async_trait]
impl PromptExecutor for NoopExecutor {
    async fn execute(
        &self,
        _session: &PipeSession,
        _prompt_text: &str,
        _history: &[Value],
        event_tx: tokio::sync::mpsc::Sender<StreamEvent>,
    ) -> Result<PromptResult, String> {
        let _ = event_tx.send(StreamEvent::AgentMessageChunk {
            content: hermes_acp_server::StreamContent::Text {
                text: "noop".to_string(),
            },
        }).await;
        Ok(PromptResult {
            stop_reason: StopReason::EndTurn,
            usage: None,
        })
    }
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

#[tokio::test]
async fn server_starts_and_stops_cleanly() {
    let pipe = test_pipe("start-stop");
    let config = test_config(pipe.clone());
    let server = AcpPipeServer::new(config).unwrap();

    assert_eq!(server.connection_count(), 0);
    assert!(!server.has_cherry_client());
    assert_eq!(server.endpoint(), pipe);

    let server_arc = Arc::new(server);
    let srv = server_arc.clone();
    let handle = tokio::spawn(async move { srv.run().await.unwrap() });

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    server_arc.shutdown();

    let result = tokio::time::timeout(std::time::Duration::from_secs(3), handle).await;
    assert!(result.is_ok(), "server shutdown timed out");
    assert!(server_arc.is_shutdown());
}

#[tokio::test]
async fn full_handshake_initialize_session_new_ping() {
    let pipe = test_pipe("handshake");
    let config = test_config(pipe.clone());
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
        "params": { "clientInfo": { "name": "test-client", "title": "Test Client", "version": "1.0.0" } }
    })).await;
    assert_eq!(resp["id"], 1);
    assert_eq!(resp["result"]["protocolVersion"], 1);
    assert_eq!(resp["result"]["agentInfo"]["name"], "test-agent");

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_eq!(server_arc.connection_count(), 1);
    let conns = server_arc.connections();
    assert_eq!(conns[0].client_name.as_deref(), Some("test-client"));

    // session/new
    let resp = roundtrip(&mut client, json!({
        "jsonrpc": "2.0", "id": 2, "method": "session/new",
        "params": { "cwd": "C:\\test", "_meta": { "source": "test" } }
    })).await;
    assert_eq!(resp["id"], 2);
    assert!(resp["result"]["sessionId"].as_str().unwrap().starts_with("acp:main:"));

    // ping
    let resp = roundtrip(&mut client, json!({
        "jsonrpc": "2.0", "id": 3, "method": "session/ping"
    })).await;
    assert_eq!(resp["id"], 3);

    // unknown method
    let resp = roundtrip(&mut client, json!({
        "jsonrpc": "2.0", "id": 4, "method": "nonexistent/method"
    })).await;
    assert_eq!(resp["error"]["code"], -32601);

    // authenticate (pipe trust boundary)
    let resp = roundtrip(&mut client, json!({
        "jsonrpc": "2.0", "id": 5, "method": "authenticate", "params": {}
    })).await;
    assert_eq!(resp["error"]["code"], -32601);

    server_arc.shutdown();
    let _ = tokio::time::timeout(std::time::Duration::from_secs(3), handle).await;
}

#[tokio::test]
async fn set_mode_updates_session() {
    let pipe = test_pipe("set-mode");
    let config = test_config(pipe.clone());
    let server = AcpPipeServer::new(config).unwrap();
    let server_arc = Arc::new(server);
    let srv = server_arc.clone();
    let handle = tokio::spawn(async move { srv.run().await.unwrap() });

    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    let mut client = tokio::net::windows::named_pipe::ClientOptions::new()
        .open(&pipe).unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let _ = roundtrip(&mut client, json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "clientInfo": { "name": "test", "title": "T", "version": "1" } }
    })).await;
    let _ = roundtrip(&mut client, json!({
        "jsonrpc": "2.0", "id": 2, "method": "session/new", "params": {}
    })).await;

    let resp = roundtrip(&mut client, json!({
        "jsonrpc": "2.0", "id": 3, "method": "session/set_mode",
        "params": { "modeId": "code-assistant" }
    })).await;
    assert_eq!(resp["id"], 3);
    assert!(resp.get("result").is_some());

    server_arc.shutdown();
    let _ = tokio::time::timeout(std::time::Duration::from_secs(3), handle).await;
}
