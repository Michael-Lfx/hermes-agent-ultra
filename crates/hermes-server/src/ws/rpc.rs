//! JSON-RPC 2.0 protocol types and parsing.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Request
// ---------------------------------------------------------------------------

/// JSON-RPC 2.0 request.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JsonRpcRequest {
    #[serde(rename = "jsonrpc")]
    pub version: Option<String>,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<serde_json::Value>,
}

impl JsonRpcRequest {
    /// Validate the request.
    pub fn validate(&self) -> Result<(), JsonRpcError> {
        if self.method.is_empty() {
            return Err(JsonRpcError::invalid_request(
                "method must be non-empty string".into(),
            ));
        }
        if let Some(ref params) = self.params {
            if !params.is_object() {
                return Err(JsonRpcError::invalid_params(
                    "params must be an object".into(),
                ));
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Response
// ---------------------------------------------------------------------------

/// JSON-RPC 2.0 response (success or error).
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcResponse {
    #[serde(rename = "jsonrpc")]
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    /// Create a success response.
    pub fn ok(id: Option<serde_json::Value>, result: serde_json::Value) -> Self {
        Self {
            version: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// Create an error response.
    pub fn err(id: Option<serde_json::Value>, error: JsonRpcError) -> Self {
        Self {
            version: "2.0".into(),
            id,
            result: None,
            error: Some(error),
        }
    }
}

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

/// JSON-RPC 2.0 error object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl JsonRpcError {
    pub fn parse_error(message: String) -> Self {
        Self {
            code: -32700,
            message,
            data: None,
        }
    }

    pub fn invalid_request(message: String) -> Self {
        Self {
            code: -32600,
            message,
            data: None,
        }
    }

    pub fn method_not_found(message: String) -> Self {
        Self {
            code: -32601,
            message,
            data: None,
        }
    }

    pub fn invalid_params(message: String) -> Self {
        Self {
            code: -32602,
            message,
            data: None,
        }
    }

    pub fn internal_error(message: String) -> Self {
        Self {
            code: -32603,
            message,
            data: None,
        }
    }

    pub fn server_error(code: i32, message: String) -> Self {
        Self {
            code,
            message,
            data: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Event (server push)
// ---------------------------------------------------------------------------

/// JSON-RPC 2.0 event (server push, no id).
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcEvent {
    #[serde(rename = "jsonrpc")]
    pub version: String,
    pub method: String,
    pub params: EventParams,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventParams {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
}

impl JsonRpcEvent {
    pub fn new(event_type: impl Into<String>, session_id: Option<String>, payload: Option<serde_json::Value>) -> Self {
        Self {
            version: "2.0".into(),
            method: "event".into(),
            params: EventParams {
                event_type: event_type.into(),
                session_id,
                payload,
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Framing helpers
// ---------------------------------------------------------------------------

/// Serialize a JSON-RPC message to a newline-delimited string.
pub fn frame_message(msg: &impl Serialize) -> Result<String, serde_json::Error> {
    let json = serde_json::to_string(msg)?;
    Ok(format!("{}\n", json))
}

/// Parse a newline-delimited JSON-RPC request.
pub fn parse_request(line: &str) -> Result<JsonRpcRequest, JsonRpcError> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Err(JsonRpcError::parse_error("empty line".into()));
    }
    
    serde_json::from_str(trimmed)
        .map_err(|e| JsonRpcError::parse_error(format!("invalid json: {}", e)))
}
