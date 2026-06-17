//! Stub market data provider for HK/US symbols pending real API integration.
//!
//! Returns deterministic synthetic OHLCV so backtests can run end-to-end.

use async_trait::async_trait;
use chrono::{Duration, NaiveDate};
use tracing::info;

use crate::error::TradingError;
use crate::provider::MarketDataProvider;
use crate::symbol::{is_hk_share, is_us_share, normalize_symbol};
use crate::types::{Interval, OhlcvData, OhlcvRequest, OhlcvRow};

/// Stub provider for HK/US markets — mock OHLCV until live APIs are wired.
#[derive(Debug, Clone, Default)]
pub struct StubProvider;

impl StubProvider {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    fn base_price(symbol: &str) -> f64 {
        match symbol {
            s if s.ends_with(".HK") => 350.0,
            s if s.ends_with(".US") => 180.0,
            _ => 100.0,
        }
    }

    fn generate_rows(
        symbol: &str,
        start: NaiveDate,
        end: NaiveDate,
        _interval: Interval,
    ) -> Vec<OhlcvRow> {
        let base_price = Self::base_price(symbol);
        let mut rows = Vec::new();
        let mut date = start;
        let mut i = 0;
        while date <= end {
            let t = i as f64;
            let close =
                base_price + 0.1 * t + base_price * 0.1 * (t * std::f64::consts::PI / 30.0).sin();
            let open = close * (1.0 + 0.005 * ((i + 1) as f64 * 0.3).sin());
            let high = close.max(open) * 1.02;
            let low = close.min(open) * 0.98;
            let volume = 1_000_000.0 + 500_000.0 * (t * 0.2).sin();

            rows.push(OhlcvRow {
                date,
                open,
                high,
                low,
                close,
                volume,
            });

            date += Duration::days(1);
            i += 1;
        }
        rows
    }
}

#[async_trait]
impl MarketDataProvider for StubProvider {
    async fn fetch_ohlcv(&self, req: &OhlcvRequest) -> Result<OhlcvData, TradingError> {
        if !is_hk_share(&req.symbol) && !is_us_share(&req.symbol) {
            return Err(TradingError::SymbolNotFound(format!(
                "Symbol '{}' is not a supported HK/US format. \
                 Use '0700.HK', 'HK_00700', 'AAPL', or 'AAPL.US'.",
                req.symbol
            )));
        }

        let canonical = normalize_symbol(&req.symbol);
        info!(
            symbol = %canonical,
            "HK/US stub data (real API pending integration)"
        );

        let rows = Self::generate_rows(&canonical, req.start, req.end, req.interval);

        Ok(OhlcvData {
            symbol: canonical,
            interval: req.interval,
            rows,
            partial: false,
        })
    }

    fn name(&self) -> &str {
        "stub"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stub_returns_hk_rows() {
        let provider = StubProvider::new();
        let req = OhlcvRequest {
            symbol: "0700.HK".to_string(),
            start: NaiveDate::from_ymd_opt(2026, 5, 1).unwrap(),
            end: NaiveDate::from_ymd_opt(2026, 5, 10).unwrap(),
            interval: Interval::Daily,
        };
        let data = provider.fetch_ohlcv(&req).await.unwrap();
        assert_eq!(data.symbol, "0700.HK");
        assert_eq!(data.len(), 10);
    }

    #[tokio::test]
    async fn stub_rejects_crypto() {
        let provider = StubProvider::new();
        let req = OhlcvRequest {
            symbol: "BTC-USDT".to_string(),
            start: NaiveDate::from_ymd_opt(2026, 5, 1).unwrap(),
            end: NaiveDate::from_ymd_opt(2026, 5, 5).unwrap(),
            interval: Interval::Daily,
        };
        assert!(provider.fetch_ohlcv(&req).await.is_err());
    }
}
