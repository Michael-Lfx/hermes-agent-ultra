//! Dedicated tokio task for LLM conversation runs (Agent actor lane).

use std::sync::Arc;
use std::time::Instant;

use hermes_agent::{AgentLoop, RunConversationParams, split_messages_for_run_conversation};
use hermes_core::{AgentError, ToolSchema};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::tui::{Event, StreamHandle};

pub struct StandardAgentRunRequest {
    pub agent: Arc<AgentLoop>,
    pub messages: Vec<hermes_core::Message>,
    pub stream_enabled: bool,
    pub tool_schemas: Vec<ToolSchema>,
    pub stream_handle: Option<StreamHandle>,
    pub session_id: String,
    pub result_tx: mpsc::UnboundedSender<Event>,
}

enum AgentLaneCommand {
    Run(StandardAgentRunRequest),
    Abort,
}

/// Serializes agent conversation runs on a single background task.
#[derive(Clone)]
pub struct AgentLane {
    tx: mpsc::UnboundedSender<AgentLaneCommand>,
}

impl AgentLane {
    pub fn spawn() -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            let mut active: Option<JoinHandle<()>> = None;
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    AgentLaneCommand::Abort => {
                        if let Some(task) = active.take() {
                            task.abort();
                            let _ = task.await;
                        }
                    }
                    AgentLaneCommand::Run(job) => {
                        if let Some(task) = active.take() {
                            task.abort();
                            let _ = task.await;
                        }
                        active = Some(tokio::spawn(run_standard_agent(job)));
                    }
                }
            }
            if let Some(task) = active {
                task.abort();
                let _ = task.await;
            }
        });
        Self { tx }
    }

    pub fn submit(&self, request: StandardAgentRunRequest) {
        let _ = self.tx.send(AgentLaneCommand::Run(request));
    }

    pub fn abort(&self) {
        let _ = self.tx.send(AgentLaneCommand::Abort);
    }
}

async fn run_standard_agent(job: StandardAgentRunRequest) {
    let started = Instant::now();
    let result = async {
        let (history, user_message) = split_messages_for_run_conversation(&job.messages)
            .ok_or_else(|| AgentError::Config("no user message in turn".into()))?;
        let task_id = Some(job.session_id.clone());
        if job.stream_enabled {
            let stream_cb: Option<Box<dyn Fn(hermes_core::StreamChunk) + Send + Sync>> =
                job.stream_handle.map(|h| {
                    Box::new(move |chunk: hermes_core::StreamChunk| {
                        h.send_chunk(chunk);
                    }) as Box<dyn Fn(hermes_core::StreamChunk) + Send + Sync>
                });
            job.agent
                .run_conversation(RunConversationParams {
                    user_message,
                    conversation_history: history,
                    task_id,
                    stream_callback: stream_cb,
                    persist_user_message: None,
                    tools: Some(job.tool_schemas),
                    persist_session: false,
                })
                .await
                .map(|c| c.into_loop_result())
        } else {
            job.agent
                .run_conversation(RunConversationParams {
                    user_message,
                    conversation_history: history,
                    task_id,
                    stream_callback: None,
                    persist_user_message: None,
                    tools: Some(job.tool_schemas),
                    persist_session: false,
                })
                .await
                .map(|c| c.into_loop_result())
        }
    }
    .await
    .map_err(|e| e.to_string());

    let _ = job.result_tx.send(Event::AgentRunComplete {
        result,
        elapsed_secs: started.elapsed().as_secs_f64(),
    });
}
