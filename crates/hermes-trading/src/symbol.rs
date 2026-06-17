//! Symbol format detection and normalization for multi-market routing.

use crate::error::TradingError;
/// Whether a symbol is an A-share (Shenzhen `.SZ` or Shanghai `.SH`).
#[must_use]
pub fn is_a_share(symbol: &str) -> bool {
    let upper = symbol.to_uppercase();
    upper.ends_with(".SZ") || upper.ends_with(".SH")
}
>>>>>>> 3a6234f77 (feat(trading): reject US/HK OHLCV and backtest until live data is wired)

    }
    // Yahoo/Bloomberg Shanghai suffix → Hermes `.SH`
    if upper.ends_with(".SS") {
        return format!("{}.SH", &upper[..upper.len() - 3]);
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
    Ok(())
}

<<<<<<< HEAD
}
