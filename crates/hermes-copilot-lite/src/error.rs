//! Error types for the hermes-copilot-lite crate.

use thiserror::Error;

/// Errors that can occur in the copilot orchestrator.
#[derive(Debug, Error)]
pub enum CopilotError {
    #[error("No strategy configured")]
    NoStrategy,

    #[error("No data provider configured")]
    NoProvider,

    #[error("Strategy execution failed: {0}")]
    StrategyFailed(#[from] hermes_strategies::StrategyError),

    #[error("Market watch error: {0}")]
    Watch(#[from] hermes_market_watch::WatchError),

    #[error("Trading error: {0}")]
    Trading(#[from] hermes_trading::TradingError),

    #[error("Orchestration error: {0}")]
    Orchestration(String),
}
