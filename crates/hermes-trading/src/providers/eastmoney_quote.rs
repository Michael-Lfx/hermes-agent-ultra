//! Eastmoney realtime quote API for A-shares.

use async_trait::async_trait;
use serde::Deserialize;
use tracing::debug;

use crate::error::TradingError;
use crate::http::{default_client, send_with_retry};
use crate::providers::eastmoney::EastmoneyProvider;
use crate::quote_data::QuoteData;
use crate::quote_provider::QuoteProvider;
use crate::settlement::is_a_share;
use crate::symbol::normalize_symbol;

const EASTMONEY_QUOTE_URL: &str = "https://push2.eastmoney.com/api/qt/stock/get";
const FIELDS: &str = "f57,f58,f43,f169,f170,f47,f48,f60,f84,f116,f117,f162";

/// Realtime A-share quote via Eastmoney `push2` (not historical `push2his`).
#[derive(Debug, Clone)]
pub struct EastmoneyQuoteProvider {
    client: reqwest::Client,
}

impl EastmoneyQuoteProvider {
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: default_client(),
        }
    }

    fn scaled_price(raw: Option<i64>) -> Option<f64> {
        raw.map(|v| v as f64 / 100.0)
    }

    fn scaled_change(raw: Option<i64>) -> Option<f64> {
        raw.map(|v| v as f64 / 100.0)
    }

    fn scaled_pct(raw: Option<i64>) -> Option<f64> {
        raw.map(|v| v as f64 / 100.0)
    }
}

impl Default for EastmoneyQuoteProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
struct EastmoneyQuoteResponse {
    data: Option<EastmoneyQuoteData>,
}

#[derive(Debug, Deserialize)]
struct EastmoneyQuoteData {
    #[serde(rename = "f57")]
    code: Option<String>,
    #[serde(rename = "f58")]
    name: Option<String>,
    #[serde(rename = "f43")]
    price_raw: Option<i64>,
    #[serde(rename = "f169")]
    change_raw: Option<i64>,
    #[serde(rename = "f170")]
    change_pct_raw: Option<i64>,
    #[serde(rename = "f47")]
    volume: Option<i64>,
    #[serde(rename = "f116")]
    pe_raw: Option<i64>,
    #[serde(rename = "f162")]
    pe_alt_raw: Option<i64>,
}

=======
impl Default for EastmoneyQuoteProvider {
    fn default() -> Self {
        Self::new()
    }
}

>>>>>>> f76b705d1 (feat(trading): shared eastmoney HTTP layer with Tencent qt fallback)
        crate::providers::akshare::fetch_a_share_quote_chain(&canonical).await
    }

    fn name(&self) -> &str {
        "eastmoney"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaled_fields() {
        assert_eq!(EastmoneyQuoteProvider::scaled_price(Some(1050)), Some(10.5));
    }
}
