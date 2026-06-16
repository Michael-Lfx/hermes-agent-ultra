//! Auto-router that selects the appropriate provider based on symbol format.
//!
//! Routing rules:
//! - Symbols containing `-` (e.g. `"BTC-USDT"`, `"ETH-BTC"`) → [`BinanceProvider`]
//! - Symbols ending in `.SZ` or `.SH` (e.g. `"000001.SZ"`) → [`EastmoneyProvider`]

use async_trait::async_trait;
use tracing::debug;

use crate::cache::DiskCache;
use crate::error::TradingError;
use crate::provider::MarketDataProvider;
use crate::settlement::is_a_share;
use crate::types::{OhlcvData, OhlcvRequest};

use super::binance::BinanceProvider;
use super::eastmoney::EastmoneyProvider;

/// Explicit data source for market data requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DataSource {
    #[default]
    Auto,
    Binance,
    Eastmoney,
}

impl DataSource {
    /// Parse from tool parameter string (`auto`, `binance`, `eastmoney`).
    pub fn parse(value: &str) -> Result<Self, TradingError> {
        match value.to_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "binance" => Ok(Self::Binance),
            "eastmoney" => Ok(Self::Eastmoney),
            other => Err(TradingError::SymbolNotFound(format!(
                "Unknown data source '{other}'. Use auto, binance, or eastmoney."
            ))),
        }
    }
}

/// Automatic market data router that dispatches to the correct provider
/// based on the symbol format.
#[derive(Debug)]
pub struct AutoRouter {
    binance: Box<dyn MarketDataProvider>,
    eastmoney: Box<dyn MarketDataProvider>,
    cache: DiskCache,
}

impl AutoRouter {
    /// Create a new `AutoRouter` with default providers and disk cache enabled.
    #[must_use]
    pub fn new() -> Self {
        Self::with_providers_and_cache(
            BinanceProvider::new(),
            EastmoneyProvider::new(),
            DiskCache::default_path(),
        )
    }

    /// Create with pre-configured providers and disabled cache (parity tests).
    #[must_use]
    pub fn with_providers(
        binance: impl MarketDataProvider + 'static,
        eastmoney: impl MarketDataProvider + 'static,
    ) -> Self {
        Self::with_providers_and_cache(binance, eastmoney, DiskCache::disabled())
    }

    /// Create with pre-configured providers and an explicit cache.
    #[must_use]
    pub fn with_providers_and_cache(
        binance: impl MarketDataProvider + 'static,
        eastmoney: impl MarketDataProvider + 'static,
        cache: DiskCache,
    ) -> Self {
        Self {
            binance: Box::new(binance),
            eastmoney: Box::new(eastmoney),
            cache,
        }
    }

    /// Determine which provider to use based on the symbol format.
    fn select<'a>(&'a self, symbol: &str) -> Result<&'a dyn MarketDataProvider, TradingError> {
        let upper = symbol.to_uppercase();
        if upper.ends_with(".SZ") || upper.ends_with(".SH") {
            debug!(symbol = %symbol, provider = "eastmoney", "AutoRouter selected");
            Ok(&self.eastmoney)
        } else if symbol.contains('-') {
            debug!(symbol = %symbol, provider = "binance", "AutoRouter selected");
            Ok(&self.binance)
        } else {
            Err(TradingError::SymbolNotFound(format!(
                "Cannot determine provider for symbol '{symbol}'. \
                 Use 'XXX-YYY' for crypto (Binance) or 'XXXXXX.SZ/.SH' for A-shares (Eastmoney)."
            )))
        }
    }

    /// Resolve provider for an explicit or auto-detected data source.
    fn resolve_provider<'a>(
        &'a self,
        symbol: &str,
        source: DataSource,
    ) -> Result<&'a dyn MarketDataProvider, TradingError> {
        ensure_ohlcv_supported(symbol)?;
}

impl Default for AutoRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MarketDataProvider for AutoRouter {
    async fn fetch_ohlcv(&self, req: &OhlcvRequest) -> Result<OhlcvData, TradingError> {
        self.fetch_ohlcv_with_source(req, DataSource::Auto, false)
            .await
    }

    fn name(&self) -> &str {
        "auto-router"
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use chrono::NaiveDate;

    use super::*;
    use crate::providers::mock::MockProvider;
    use crate::types::Interval;

    #[derive(Debug)]
    struct CountingProvider {
        count: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl MarketDataProvider for CountingProvider {
        async fn fetch_ohlcv(&self, req: &OhlcvRequest) -> Result<OhlcvData, TradingError> {
            self.count.fetch_add(1, Ordering::SeqCst);
            MockProvider::new().fetch_ohlcv(req).await
        }

        fn name(&self) -> &str {
            "mock"
        }
    }

    fn sample_req() -> OhlcvRequest {
        OhlcvRequest {
            symbol: "BTC-USDT".to_string(),
            start: NaiveDate::from_ymd_opt(2026, 5, 1).unwrap(),
            end: NaiveDate::from_ymd_opt(2026, 5, 5).unwrap(),
            interval: Interval::Daily,
        }
    }

    #[test]
    fn test_router_selects_binance() {
        let router = AutoRouter::new();
        assert_eq!(router.select("BTC-USDT").unwrap().name(), "binance");
        assert_eq!(router.select("ETH-BTC").unwrap().name(), "binance");
    }

    #[test]
    fn test_router_selects_eastmoney() {
        let router = AutoRouter::new();
        assert_eq!(router.select("000001.SZ").unwrap().name(), "eastmoney");
        assert_eq!(router.select("600519.SH").unwrap().name(), "eastmoney");
    }

    #[test]
    fn test_forced_binance_rejects_a_share() {
        let router = AutoRouter::new();
        assert!(router.select("AAPL").is_err());
>>>>>>> 930eea825 (refactor(trading): rename hermes-vibe to hermes-trading across workspace)
    }
}
