//! Symbol format detection and normalization for multi-market routing.

use crate::error::TradingError;

=======
/// Whether a symbol is a Hong Kong listed stock.
///
/// Accepted formats: `0700.HK`, `HK_00700`.
#[must_use]
pub fn is_hk_share(symbol: &str) -> bool {
    let upper = symbol.to_uppercase();
    if upper.ends_with(".HK") {
        return true;
    }
    if let Some(suffix) = upper.strip_prefix("HK_") {
        return !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit());
    }
    false
}

/// Whether a symbol is a US listed stock.
///
/// Accepted formats: `AAPL`, `AAPL.US`.
#[must_use]
pub fn is_us_share(symbol: &str) -> bool {
    let upper = symbol.to_uppercase();
    if upper.ends_with(".US") {
        return true;
    }
    if symbol.contains('.') || symbol.contains('-') {
        return false;
    }
    let len = upper.len();
    (1..=5).contains(&len) && upper.chars().all(|c| c.is_ascii_alphabetic())
}

/// Normalize alternate symbol formats to canonical form (`HK_00700` → `0700.HK`).
#[must_use]
pub fn normalize_symbol(symbol: &str) -> String {
    let trimmed = symbol.trim();
    // ponytail: tiny alias table; extend when agents keep passing bare crypto tickers.
    match trimmed {
        "比特币" => return "BTC-USDT".to_string(),
        "以太坊" => return "ETH-USDT".to_string(),
        _ => {}
    }
    let upper = trimmed.to_uppercase();
    match upper.as_str() {
        "BTC" => return "BTC-USDT".to_string(),
        "ETH" => return "ETH-USDT".to_string(),
        _ => {}
    }
    if let Some(suffix) = upper.strip_prefix("HK_")
        && !suffix.is_empty()
        && suffix.chars().all(|c| c.is_ascii_digit())
    {
        let code: u32 = suffix.parse().unwrap_or(0);
        return format!("{code:04}.HK");
>>>>>>> cc681fb4b (feat(trading): spot quote stack with get_quote tool and spot-quote skill)
    }
    upper
}

/// Reject US/HK symbols for historical OHLCV and backtest until live APIs are wired.
pub fn ensure_ohlcv_supported(symbol: &str) -> Result<(), TradingError> {
    let canonical = normalize_symbol(symbol);
    if is_hk_share(&canonical) || is_us_share(&canonical) {
        return Err(TradingError::SymbolNotFound(format!(
            "Historical OHLCV for '{symbol}' is not supported yet. \
             Use get_quote for US/HK spot prices; backtest A-share or crypto symbols only."
        )));
    }
<<<<<<< HEAD
    Ok(())
}

}
