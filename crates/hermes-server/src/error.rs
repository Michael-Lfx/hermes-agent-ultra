use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use std::fmt;

/// Application-wide error type.
/// Converts into an HTTP response with `{"detail": "..."}` body.
#[derive(Debug)]
pub enum AppError {
    Config(String),
    NotFound(String),
    BadRequest(String),
    Unauthorized(String),
    Internal(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Config(msg) => write!(f, "config error: {}", msg),
            AppError::NotFound(msg) => write!(f, "not found: {}", msg),
            AppError::BadRequest(msg) => write!(f, "bad request: {}", msg),
            AppError::Unauthorized(msg) => write!(f, "unauthorized: {}", msg),
            AppError::Internal(msg) => write!(f, "internal error: {}", msg),
        }
    }
}

impl std::error::Error for AppError {}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::Config(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg.clone()),
            AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
        };

        let body = Json(json!({ "detail": message }));
        (status, body).into_response()
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::Internal(err.to_string())
    }
}

impl From<serde_yaml::Error> for AppError {
    fn from(err: serde_yaml::Error) -> Self {
        AppError::Config(err.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::BadRequest(err.to_string())
    }
}

impl From<hermes_config::ConfigError> for AppError {
    fn from(err: hermes_config::ConfigError) -> Self {
        AppError::Config(err.to_string())
    }
}

impl From<hermes_core::AgentError> for AppError {
    fn from(err: hermes_core::AgentError) -> Self {
        AppError::Internal(err.to_string())
    }
}

/// Helper to create a successful JSON response.
pub fn ok_json<T: serde::Serialize>(data: T) -> Json<serde_json::Value> {
    Json(serde_json::to_value(data).unwrap_or_else(|_| json!({})))
}
