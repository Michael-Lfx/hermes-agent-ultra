//! Trading: 0py market data and backtesting library for Hermes Agent.
//!
//! This crate provides:
//! - `MarketDataProvider` trait and implementations for fetching OHLCV data
//! - `BacktestEngine` for running template-based backtests
//!
//! **0py constraint**: No Python runtime, PyO3, or Python subprocess dependencies.

pub mod backtest;
pub mod cache;
pub mod error;
pub mod http;
pub mod indicators;
pub mod provider;
pub mod providers;
pub mod settlement;
pub mod symbol;
pub mod types;

pub use backtest::{BacktestEngine, Period, RunCard, SignalKind, StrategyInfo, StrategyRegistry};
pub use cache::DiskCache;
pub use error::TradingError;
pub use indicators::{rsi, sma};
pub use provider::MarketDataProvider;
#[cfg(any(test, feature = "test-mock"))]
pub use providers::MockProvider;
pub use providers::{AutoRouter, BinanceProvider, DataSource, EastmoneyProvider, StubProvider};
pub use settlement::{SettlementMode, is_a_share, settlement_for_symbol};
pub use symbol::{is_hk_share, is_us_share, normalize_symbol};
pub use types::{Interval, OhlcvData, OhlcvRequest, OhlcvRow, mark_partial};
