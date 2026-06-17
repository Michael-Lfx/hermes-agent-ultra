//! Auto-router that selects the appropriate provider based on symbol format.
//!
//! Routing rules:
//! - Symbols containing `-` (e.g. `"BTC-USDT"`, `"ETH-BTC"`) → [`BinanceProvider`]
//! - Symbols ending in `.SZ` or `.SH` (e.g. `"000001.SZ"`) → [`EastmoneyProvider`]
//! - US/HK symbols → rejected (historical OHLCV not available; use `get_quote` for spot)

use async_trait::async_trait;
use tracing::debug;

use crate::cache::DiskCache;
use crate::error::TradingError;
use crate::provider::MarketDataProvider;
use crate::settlement::is_a_share;
use crate::symbol::{ensure_ohlcv_supported, normalize_symbol};
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
            "stub" => Err(TradingError::SymbolNotFound(
                "source=stub is no longer supported. Use auto, binance, or eastmoney.".into(),
            )),
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
        ensure_ohlcv_supported(symbol)?;
        let upper = symbol.to_uppercase();
        if upper.ends_with(".SZ") || upper.ends_with(".SH") {
            // debug!(symbol = %symbol, provider = "eastmoney", "AutoRouter selected");
            Ok(&self.eastmoney)
        } else if symbol.contains('-') {
            // debug!(symbol = %symbol, provider = "binance", "AutoRouter selected");
            Ok(&self.binance)
        } else {
            Err(TradingError::SymbolNotFound(format!(
                "Cannot determine provider for symbol '{symbol}'. \
                 Use 'XXX-YYY' for crypto or 'XXXXXX.SZ/.SH' for A-shares."
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
=======
            counter(),
>>>>>>> ecc1ce61e (feat(trading): HK/US symbol routing with stub mock provider)
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
    }
}
