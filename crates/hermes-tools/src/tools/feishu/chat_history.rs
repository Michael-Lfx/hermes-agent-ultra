//! Feishu Chat History tool — get messages, search chats, get chat members.

use std::sync::Arc;

use async_trait::async_trait;
use indexmap::IndexMap;
use serde_json::{Value, json};
use tracing::debug;

use hermes_core::{JsonSchema, ToolError, ToolHandler, ToolSchema, tool_schema};

use super::FeishuApiClient;

/// Handler for the `feishu_chat_history` tool.
pub struct FeishuChatHistoryHandler {
    client: Arc<FeishuApiClient>,
}

impl FeishuChatHistoryHandler {
    pub fn new(client: Arc<FeishuApiClient>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl ToolHandler for FeishuChatHistoryHandler {
    async fn execute(&self, params: Value) -> Result<String, ToolError> {
        let action = params
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("Missing required param: action".into()))?;

        let page_size = params
            .get("page_size")
            .and_then(|v| v.as_u64())
            .unwrap_or(20)
            .min(50);
        let page_size_str = page_size.to_string();

        let data = match action {
            "get_messages" => {
                let chat_id = params
                    .get("chat_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ToolError::InvalidParams("get_messages requires 'chat_id'".into())
                    })?;

                let mut query: Vec<(&str, &str)> = vec![
                    ("container_id_type", "chat"),
                    ("container_id", chat_id),
                    ("page_size", &page_size_str),
                ];

                // Convert ISO 8601 timestamps to Unix timestamps (seconds) for the Feishu API.
                let start_unix;
                let end_unix;
                if let Some(start) = params.get("start_time").and_then(|v| v.as_str()) {
                    start_unix = iso8601_to_unix(start)?;
                    query.push(("start_time", &start_unix));
                }
                if let Some(end) = params.get("end_time").and_then(|v| v.as_str()) {
                    end_unix = iso8601_to_unix(end)?;
                    query.push(("end_time", &end_unix));
                }

                debug!(chat_id, "Fetching Feishu messages");
                self.client.get("/im/v1/messages", &query).await?
            }
            "search_chats" => {
                let mut query: Vec<(&str, &str)> = vec![("page_size", &page_size_str)];

                let query_val;
                if let Some(q) = params.get("query").and_then(|v| v.as_str()) {
                    query_val = q.to_string();
                    query.push(("query", &query_val));
                }

                debug!("Searching Feishu chats");
                self.client.get("/im/v1/chats", &query).await?
            }
            "get_chat_members" => {
                let chat_id = params
                    .get("chat_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ToolError::InvalidParams("get_chat_members requires 'chat_id'".into())
                    })?;

                let query: Vec<(&str, &str)> = vec![("page_size", &page_size_str)];

                let path = format!("/im/v1/chats/{chat_id}/members");
                debug!(chat_id, "Fetching Feishu chat members");
                self.client.get(&path, &query).await?
            }
            other => {
                return Err(ToolError::InvalidParams(format!(
                    "Unknown action '{other}'. Expected: get_messages, search_chats, get_chat_members"
                )));
            }
        };

        serde_json::to_string_pretty(&data)
            .map_err(|e| ToolError::ExecutionFailed(format!("JSON serialize error: {e}")))
    }

    fn schema(&self) -> ToolSchema {
        let mut props = IndexMap::new();
        props.insert(
            "action".into(),
            json!({
                "type": "string",
                "enum": ["get_messages", "search_chats", "get_chat_members"],
                "description": "Chat history operation to perform"
            }),
        );
        props.insert(
            "chat_id".into(),
            json!({
                "type": "string",
                "description": "Chat/group ID (required for get_messages and get_chat_members)"
            }),
        );
        props.insert(
            "start_time".into(),
            json!({
                "type": "string",
                "description": "Start time for messages in ISO 8601 format"
            }),
        );
        props.insert(
            "end_time".into(),
            json!({
                "type": "string",
                "description": "End time for messages in ISO 8601 format"
            }),
        );
        props.insert(
            "query".into(),
            json!({
                "type": "string",
                "description": "Search query for chat name (used with search_chats)"
            }),
        );
        props.insert(
            "page_size".into(),
            json!({
                "type": "integer",
                "description": "Number of results per page (default 20, max 50)"
            }),
        );

        tool_schema(
            "feishu_chat_history",
            concat!(
                "Interact with Feishu/Lark chat history. ",
                "Actions: get_messages (fetch messages in a chat), ",
                "search_chats (search chats by name), ",
                "get_chat_members (list members of a chat)."
            ),
            JsonSchema::object(props, vec!["action".into()]),
        )
    }
}

/// Parse an ISO 8601 timestamp string to a Unix timestamp (seconds) string.
fn iso8601_to_unix(ts: &str) -> Result<String, ToolError> {
    // Try parsing common ISO 8601 formats.
    let dt = chrono::DateTime::parse_from_rfc3339(ts)
        .or_else(|_| chrono::DateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%S"))
        .or_else(|_| chrono::DateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S"))
        .map_err(|e| ToolError::InvalidParams(format!("Invalid timestamp '{ts}': {e}")))?;
    Ok(dt.timestamp().to_string())
}
