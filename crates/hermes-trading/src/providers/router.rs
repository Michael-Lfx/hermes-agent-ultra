//! Auto-router that selects the appropriate provider based on symbol format.
//!
//! Routing rules:
//! - Symbols containing `-` (e.g. `"BTC-USDT"`, `"ETH-BTC"`) → [`BinanceProvider`]
//! - Symbols ending in `.SZ` or `.SH` (e.g. `"000001.SZ"`) → [`EastmoneyProvider`]

use async_trait::async_trait;
use tracing::debug;

use crate::error::TradingError;
use crate::provider::MarketDataProvider;
use crate::types::{OhlcvData, OhlcvRequest};

use super::binance::BinanceProvider;
use super::eastmoney::EastmoneyProvider;

/// Automatic market data router that dispatches to the correct provider
/// based on the symbol format.
#[derive(Debug)]
pub struct AutoRouter {
    binance: Box<dyn MarketDataProvider>,
    eastmoney: Box<dyn MarketDataProvider>,
}

impl AutoRouter {
    /// Create a new `AutoRouter` with default providers.
    #[must_use]
    pub fn new() -> Self {
        Self {
            binance: Box::new(BinanceProvider::new()),
            eastmoney: Box::new(EastmoneyProvider::new()),
        }
    }

    /// Create with pre-configured providers.
    #[must_use]
    pub fn with_providers(
        binance: impl MarketDataProvider + 'static,
        eastmoney: impl MarketDataProvider + 'static,
    ) -> Self {
        Self {
            binance: Box::new(binance),
            eastmoney: Box::new(eastmoney),
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
        match source {
            DataSource::Auto => self.select(symbol),
            DataSource::Binance => {
                if symbol.contains('-') {
                    Ok(&self.binance)
                } else {
                    Err(TradingError::SymbolNotFound(format!(
                        "Symbol '{symbol}' is not compatible with source=binance. \
                         Use a crypto pair like BTC-USDT."
                    )))
                }
            }
            DataSource::Eastmoney => {
                if is_a_share(symbol) {
                    Ok(&self.eastmoney)
                } else {
                    Err(TradingError::SymbolNotFound(format!(
                        "Symbol '{symbol}' is not compatible with source=eastmoney. \
                         Use an A-share symbol like 000001.SZ or 600519.SH."
                    )))
                }
            }
        }
    }

    /// Fetch OHLCV using an explicit or auto-routed data source.
    ///
    /// When `refresh` is false, returns a fresh cache entry when available.
    /// When `refresh` is true, skips cache read but still writes the result back.
    pub async fn fetch_ohlcv_with_source(
        &self,
        req: &OhlcvRequest,
        source: DataSource,
        refresh: bool,
    ) -> Result<OhlcvData, TradingError> {
        let normalized_req = OhlcvRequest {
            symbol: normalize_symbol(&req.symbol),
            start: req.start,
            end: req.end,
            interval: req.interval,
        };

        ensure_ohlcv_supported(&normalized_req.symbol)?;

    }

    #[test]
    fn test_unknown_symbol_errors() {
        let router = AutoRouter::new();
        assert!(router.select("INVALID_XYZ").is_err());
        assert!(router.select("NOT_A_SYMBOL!!!").is_err());
    }

    #[test]
    fn test_forced_binance_rejects_a_share() {
        let router = AutoRouter::new();
        assert!(
            router
                .resolve_provider("000001.SZ", DataSource::Binance)
                .is_err()
        );
    }

    #[test]
    fn test_forced_eastmoney_rejects_crypto() {
        let router = AutoRouter::new();
        assert!(
            router
                .resolve_provider("BTC-USDT", DataSource::Eastmoney)
                .is_err()
        );
    }

    #[test]
    fn test_data_source_parse() {
        assert_eq!(DataSource::parse("auto").unwrap(), DataSource::Auto);
        assert_eq!(DataSource::parse("binance").unwrap(), DataSource::Binance);
        assert!(DataSource::parse("stub").is_err());
        assert!(DataSource::parse("invalid").is_err());
    }

    #[tokio::test]
    async fn cache_hit_skips_provider_fetch() {
        let dir = tempfile::tempdir().unwrap();
        let count = Arc::new(AtomicUsize::new(0));
        let counter = || CountingProvider {
            count: count.clone(),
        };
        let router = AutoRouter::with_providers_and_cache(
            counter(),
            counter(),
            DiskCache::with_dir(dir.path().to_path_buf()),
        );
        let req = sample_req();
        router
            .fetch_ohlcv_with_source(&req, DataSource::Binance, false)
            .await
            .unwrap();
        router
            .fetch_ohlcv_with_source(&req, DataSource::Binance, true)
            .await
            .unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 2);
=======
}

impl Default for AutoRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MarketDataProvider for AutoRouter {
    async fn fetch_ohlcv(&self, req: &OhlcvRequest) -> Result<OhlcvData, TradingError> {
        let provider = self.select(&req.symbol)?;
        provider.fetch_ohlcv(req).await
    }

    fn name(&self) -> &str {
        "auto-router"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_router_unknown_symbol() {
        let router = AutoRouter::new();
        assert!(router.select("AAPL").is_err());
>>>>>>> 930eea825 (refactor(trading): rename hermes-vibe to hermes-trading across workspace)
    }
}
