//! Trading: 0py market data and backtesting library for Hermes Agent.
//!
//! This crate provides:
//! - `MarketDataProvider` trait and implementations for fetching OHLCV data
//! - `BacktestEngine` for running template-based backtests
//!
//! **0py constraint**: No Python runtime, PyO3, or Python subprocess dependencies.

pub mod backtest;
pub mod error;
pub mod indicators;
pub mod provider;
pub mod providers;
pub mod quote_cache;
pub mod quote_data;
pub mod quote_provider;
pub mod research;
pub mod types;

pub use backtest::{BacktestEngine, Period, RunCard, SignalKind, StrategyInfo, StrategyRegistry};
pub use error::TradingError;
pub use indicators::{rsi, sma};
pub use provider::MarketDataProvider;
#[cfg(any(test, feature = "test-mock"))]
pub use providers::MockProvider;
#[cfg(any(test, feature = "test-mock"))]
pub use providers::MockQuoteProvider;
pub use providers::{
    AutoRouter, BinanceProvider, BinanceQuoteProvider, DataSource, EastmoneyBasicProvider,
    EastmoneyCapitalFlowProvider, EastmoneyFinancialsProvider, EastmoneyLhbProvider,
    EastmoneyProvider, EastmoneyQuoteProvider, EastmoneyValuationProvider, FundamentalsAggregator,
    FundamentalsProvider, QuoteRouter, QuoteSource, StubProvider, YahooProvider,
>>>>>>> d5f5467b3 (feat(trading): UZI equity research engine and analyze_stock tool)
};
pub use quote_cache::QuoteCache;
pub use quote_data::QuoteData;
pub use quote_provider::QuoteProvider;
<<<<<<< HEAD
<<<<<<< HEAD
