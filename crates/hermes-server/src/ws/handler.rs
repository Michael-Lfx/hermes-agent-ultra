use axum::{
    extract::{Query, State, WebSocketUpgrade},
    response::Response,
};
use axum::extract::ws::{Message as WsMessage, WebSocket};
use serde_json::json;
use std::collections::HashMap;
use tracing::{debug, info, warn};

use crate::{
    rpc,
    state::AppState,
    ws::{
        auth::authenticate_ws,
        rpc::{frame_message, parse_request, JsonRpcEvent, JsonRpcResponse},
    },
};

/// GET /api/ws - WebSocket upgrade endpoint.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let query_pairs: Vec<(String, String)> = params
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    
    if !authenticate_ws(&query_pairs, &state) {
        return Response::builder()
            .status(401)
            .body("Unauthorized".into())
            .unwrap();
    }
    
    ws.on_upgrade(move |socket| handle_ws(socket, state))
}

/// Handle bidirectional WebSocket communication.
async fn handle_ws(mut socket: WebSocket, state: AppState) {
    info!("WebSocket connection established");
    
    // Create channel for server-push events (AgentLoop → WebSocket)
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    
    // Send gateway.ready event
    let ready_event = JsonRpcEvent::new(
        "gateway.ready",
        None,
        Some(json!({
            "skin": {
                "name": "default",
                "colors": {},
            }
        })),
    );
    
    if let Ok(frame) = frame_message(&ready_event) {
        if socket.send(WsMessage::Text(frame.into())).await.is_err() {
            warn!("Failed to send gateway.ready");
            return;
        }
    }
    
    // Main loop: handle requests and push events concurrently
    loop {
        tokio::select! {
            msg = socket.recv() => {
                match msg {
                    Some(Ok(WsMessage::Text(text))) => {
                        let text_str = text.as_str();
                        
                        // Parse JSON-RPC request
                        match parse_request(text_str) {
                            Ok(request) => {
                                let method = request.method.clone();
                                debug!("Received JSON-RPC request: method={}", method);
                                
                                // Dispatch to handler
                                if let Some(response) = rpc::dispatch(request, &state).await {
                                    // Send response back through WebSocket
                                    if let Ok(frame) = frame_message(&response) {
                                        if socket.send(WsMessage::Text(frame.into())).await.is_err() {
                                            break;
                                        }
                                    }
                                    
                                    // If session.create succeeded, bind transport to session
                                    if method == "session.create" {
                                        if let Some(ref result) = response.result {
                                            if let Some(session_id) = result.get("session_id").and_then(|v| v.as_str()) {
                                                let mut sessions = state.sessions.write().await;
                                                if let Some(session) = sessions.get_mut(session_id) {
                                                    session.transport.replace(crate::ws::transport::WsTransport::new(event_tx.clone()));
                                                    info!("Bound WebSocket transport to session {}", session_id);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            Err(err) => {
                                warn!("Failed to parse JSON-RPC request: {}", err.message);
                                let response = JsonRpcResponse::err(None, err);
                                if let Ok(frame) = frame_message(&response) {
                                    let _ = socket.send(WsMessage::Text(frame.into())).await;
                                }
                            }
                        }
                    }
                    Some(Ok(WsMessage::Close(_))) => {
                        info!("WebSocket close frame received");
                        break;
                    }
                    _ => {}
                }
            }
            event = event_rx.recv() => {
                match event {
                    Some(text) => {
                        if socket.send(WsMessage::Text(text.into())).await.is_err() {
                            break;
                        }
                    }
                    None => {
                        info!("Event channel closed");
                        break;
                    }
                }
            }
        }
    }
    
    info!("WebSocket connection closed");
}
