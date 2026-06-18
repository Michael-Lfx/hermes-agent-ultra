//! akshare-rs adapter — primary A-share data path with eastmoney fallback.

mod basic_info;
=======
mod candles;
mod capital_flow;
mod events;
mod financials;
mod fund_holders;
mod lhb;
mod peers;
mod quote;
mod research;
mod valuation;
>>>>>>> d9ae746af (feat(trading): P0 equity fetch — basic Full, valuation/peers/fund_holders, dedupe)

use std::sync::OnceLock;

use akshare::AkShareClient;

use crate::error::TradingError;
use crate::settlement::is_a_share;
use crate::symbol::normalize_symbol;

static CLIENT: OnceLock<AkShareClient> = OnceLock::new();

pub(crate) fn client() -> &'static AkShareClient {
    CLIENT.get_or_init(AkShareClient::new)
}

pub(crate) fn map_err(e: akshare::Error) -> TradingError {
    TradingError::InvalidResponse(e.to_string())
}

/// Six-digit A-share code from `600519.SH`.
pub(crate) fn code6(symbol: &str) -> Result<String, TradingError> {
    let canon = normalize_symbol(symbol);
    if !is_a_share(&canon) {
        return Err(TradingError::SymbolNotFound(format!(
            "akshare A-share only: {symbol}"
        )));
    }
    Ok(canon.split('.').next().unwrap_or(&canon).to_string())
}

/// Sina paper code prefix (`sh600519` / `sz000001`).
pub(crate) fn sina_paper_code(symbol: &str) -> Result<String, TradingError> {
    let code = code6(symbol)?;
    let canon = normalize_symbol(symbol);
    Ok(if canon.ends_with(".SZ") {
        format!("sz{code}")
    } else {
        format!("sh{code}")
    })
}

pub(crate) async fn try_or_fallback<T, P, F>(primary: P, fallback: F) -> Result<T, TradingError>
where
    P: std::future::Future<Output = Result<T, TradingError>>,
    F: std::future::Future<Output = Result<T, TradingError>>,
{
    match primary.await {
        Ok(v) => Ok(v),
        Err(e) => {
            tracing::warn!(error = %e, "akshare path failed, using fallback");
            fallback.await
        }
    }
}

pub use basic_info::{
    BasicInfoSupplement, apply_supplement, fetch_basic_info_supplement, map_individual_info,
};
<<<<<<< HEAD

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code6_and_sina_paper() {
        assert_eq!(code6("600519.SH").unwrap(), "600519");
        assert_eq!(sina_paper_code("600519.SH").unwrap(), "sh600519");
        assert_eq!(sina_paper_code("000001.SZ").unwrap(), "sz000001");
    }

    #[tokio::test]
    #[ignore = "live akshare network"]
    async fn live_akshare_quote_600519() {
        let q = super::quote::fetch_a_share_quote_chain("600519.SH")
            .await
            .expect("quote");
        assert!(q.price.unwrap_or(0.0) > 0.0);
    }

    #[tokio::test]
    #[ignore = "live akshare network"]
    async fn live_akshare_candles_600519() {
        let (closes, source) = super::candles::fetch_a_share_closes("600519.SH")
            .await
            .expect("candles");
        assert!(closes.len() >= 20);
        assert!(closes.last().copied().unwrap_or(0.0) > 0.0);
        assert!(source == "akshare" || source == "eastmoney_push2his");
    }
}
