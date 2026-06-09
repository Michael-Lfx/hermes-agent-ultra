//! ACP slash command handler and HermesExecutor bridge.
//!
//! Integrates the standalone ACP Pipe Server into the CLI by providing
//! a HermesExecutor that delegates prompt execution to the agent loop.

use std::sync::Arc;

use hermes_acp_server::{
    AcpPipeServer, AcpServerConfig, AgentInfo, ConnectionInfo, PromptExecutor, PromptResult,
    StreamContent, StreamEvent,
};
use hermes_acp::protocol::{StopReason, Usage};
use hermes_agent::{AgentLoop, RunConversationParams};
use hermes_core::{AgentError, Message, ToolSchema};

use crate::app::App;
use crate::commands::{emit_command_output, CommandResult};

// ---------------------------------------------------------------------------
// HermesExecutor -- bridges ACP prompts to the agent loop
// ---------------------------------------------------------------------------

/// Bridge between the ACP PromptExecutor trait and hermes-agent's agent loop.
///
/// Each `/acp_server start` creates one HermesExecutor shared across all ACP connections.
/// It holds Arc references to the same agent loop and provider that the CLI uses,
/// so Cherry prompts go through the full agent pipeline (tools, session, streaming).
pub(crate) struct HermesExecutor {
    pub agent: Arc<AgentLoop>,
    pub tool_schemas: Vec<ToolSchema>,
}

fn convert_acp_history_to_messages(history: &[serde_json::Value]) -> Vec<Message> {
    let mut messages = Vec::with_capacity(history.len());
    for msg in history {
        let role = msg
            .get("role")
            .and_then(|v| v.as_str())
            .unwrap_or("user");
        let content = msg.get("content").and_then(|v| v.as_str()).map(|s| s.to_string());

        let role_enum = match role {
            "user" => hermes_core::MessageRole::User,
            "assistant" => hermes_core::MessageRole::Assistant,
            "system" => hermes_core::MessageRole::System,
            _ => continue,
        };

        messages.push(Message {
            role: role_enum,
            content,
            tool_calls: None,
            tool_call_id: None,
            name: None,
            reasoning_content: None,
            cache_control: None,
        });
    }
    messages
}

#[async_trait::async_trait]
impl PromptExecutor for HermesExecutor {
    async fn execute(
        &self,
        _session: &hermes_acp_server::PipeSession,
        prompt_text: &str,
        history: &[serde_json::Value],
        event_tx: tokio::sync::mpsc::Sender<StreamEvent>,
    ) -> Result<PromptResult, String> {
        let conversation_history = convert_acp_history_to_messages(history);

        let tx = event_tx.clone();
        let stream_cb: Option<Box<dyn Fn(hermes_core::StreamChunk) + Send + Sync>> =
           Some(Box::new(move |chunk: hermes_core::StreamChunk| {
                if let Some(content) = chunk.delta.as_ref().and_then(|d| d.content.as_ref()) {
                        let evt = StreamEvent::AgentMessageChunk {
                            content: StreamContent::Text {
                                text: content.clone(),
                            },
                        };
                        // Best-effort send; drop if receiver closed.
                        let _ = tx.try_send(evt);
                    }
           }));

        let params = RunConversationParams {
            user_message: prompt_text.to_string(),
            conversation_history,
            task_id: None,
            stream_callback: stream_cb,
            persist_user_message: None,
            tools: Some(self.tool_schemas.clone()),
            persist_session: false,
        };

        match self.agent.run_conversation(params).await {
            Ok(result) => {
                let stop_reason = if result.loop_result.interrupted {
                    StopReason::Cancelled
                } else if result.loop_result.failed {
                    StopReason::Error
                } else {
                    StopReason::EndTurn
                };

                let usage = result.loop_result.usage.map(|u| Usage {
                    input_tokens: u.prompt_tokens,
                    output_tokens: u.completion_tokens,
                    total_tokens: u.total_tokens,
                    thought_tokens: None,
                    cached_read_tokens: None,
                });

                Ok(PromptResult {
                    stop_reason,
                    usage,
                })
            }
            Err(AgentError::Interrupted { .. }) => Ok(PromptResult {
                stop_reason: StopReason::Cancelled,
                usage: None,
            }),
            Err(e) => Err(format!("agent error: {}", e)),
        }
    }
}

// ---------------------------------------------------------------------------
// /acp_server slash command handler
// ---------------------------------------------------------------------------

pub(crate) async fn handle_acp_command(
    app: &mut App,
    args: &[&str],
) -> Result<CommandResult, AgentError> {
    let action = args.first().copied().unwrap_or("auto");
    match action {
        "start" => start_acp_server(app),
        "stop" => stop_acp_server(app),
        "status" => {
            show_acp_status(app);
            Ok(CommandResult::Handled)
        }
        "restart" => {
            stop_acp_server(app)?;
            start_acp_server(app)
        }
        "connections" => {
            show_acp_connections(app);
            Ok(CommandResult::Handled)
        }
        "auto" => {
            // Smart default: start if not running, otherwise show status.
            if app.acp_server.is_none() {
                start_acp_server(app)
            } else {
                show_acp_status(app);
                Ok(CommandResult::Handled)
            }
        }
        _ => {
            emit_command_output(
                app,
                "Usage: /acp_server [start|stop|status|restart|connections]",
            );
            Ok(CommandResult::Handled)
        }
    }
}

fn start_acp_server(app: &mut App) -> Result<CommandResult, AgentError> {
    if app.acp_server.is_some() {
        emit_command_output(app, "[ACP server already running]");
        show_acp_status(app);
        return Ok(CommandResult::Handled);
    }

    let executor = Arc::new(HermesExecutor {
        agent: app.agent.clone(),
        tool_schemas: app.tool_schemas.clone(),
    });

    let pipe_path = hermes_acp_server::default_pipe_path();
    let agent_info = AgentInfo {
        name: "hermes-agent".to_string(),
        title: "Hermes Agent Ultra".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };

    let config = AcpServerConfig {
        pipe_path: pipe_path.to_string(),
        max_connections: 5,
        prompt_timeout_secs: 300,
        agent_info,
        executor,
    };

    let server = AcpPipeServer::new(config)
        .map_err(|e| AgentError::Config(format!("ACP server init: {}", e)))?;

    let server_arc = Arc::new(server);
    let runner = Arc::clone(&server_arc);
    tokio::spawn(async move {
        if let Err(e) = runner.run().await {
            tracing::error!(error = %e, "ACP server accept loop error");
        }
    });

    app.acp_server = Some(server_arc);

    emit_command_output(
        app,
        format!("[ACP server started on {}]", pipe_path),
    );
    Ok(CommandResult::Handled)
}

fn stop_acp_server(app: &mut App) -> Result<CommandResult, AgentError> {
    match app.acp_server.take() {
        Some(server) => {
            server.shutdown();
            emit_command_output(app, "[ACP server stopped]");
            Ok(CommandResult::Handled)
        }
        None => {
            emit_command_output(app, "[ACP server not running]");
            Ok(CommandResult::Handled)
        }
    }
}

fn show_acp_status(app: &mut App) {
    match &app.acp_server {
        Some(server) => {
            let conns = server.connection_count();
            let max = server.max_connections();
            let cherry = server.has_cherry_client();
            let endpoint = server.endpoint();
            let lines = [
                "ACP Server: running".to_string(),
                format!("Endpoint: {}", endpoint),
                format!("Connections: {}/{}", conns, max),
                format!("Cherry client: {}", if cherry { "online" } else { "none" }),
            ];
            emit_command_output(app, lines.join("\n"));
        }
        None => {
            emit_command_output(
                app,
                "ACP Server: stopped\nUse /acp_server to start.",
            );
        }
    }
}

fn show_acp_connections(app: &mut App) {
    match &app.acp_server {
        Some(server) => {
            let conns: Vec<ConnectionInfo> = server.connections();
            if conns.is_empty() {
                emit_command_output(app, "[No active connections]");
                return;
            }
            let mut lines = Vec::new();
            for (i, c) in conns.iter().enumerate() {
                let client = c
                    .client_title
                    .as_deref()
                    .or(c.client_name.as_deref())
                    .unwrap_or("unknown");
                lines.push(format!(
                    "  [{}] {}  session: {}",
                    i + 1,
                    client,
                    c.session_id.as_deref().unwrap_or("-"),
                ));
            }
            emit_command_output(app, lines.join("\n"));
        }
        None => {
            emit_command_output(app, "[ACP server not running]");
        }
    }
}
