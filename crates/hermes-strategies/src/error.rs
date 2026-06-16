//! Error types for the hermes-strategies crate.

use thiserror::Error;

/// Errors that can occur in the strategy engine.
#[derive(Debug, Error)]
pub enum StrategyError {
    #[error("Insufficient data: need at least {needed} rows, got {got}")]
    InsufficientData { needed: usize, got: usize },

    #[error("Invalid parameters: {0}")]
    InvalidParams(String),

    #[error("Strategy execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Vibe error: {0}")]
    Vibe(#[from] hermes_vibe::VibeError),

    // --- Declarative strategy errors ---

    #[error("Strategy definition error: {0}")]
    DefinitionError(String),

    #[error("Unknown indicator type: {0}")]
    UnknownIndicatorType(String),

    #[error("Invalid rule expression: {0}")]
    InvalidRuleExpression(String),

    #[error("Circular indicator dependency: {0}")]
    CircularDependency(String),

    #[error("Strategy not found: {0}")]
    NotFound(String),

    #[error("Strategy file I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}
