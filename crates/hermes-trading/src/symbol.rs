//! Symbol format detection and normalization for multi-market routing.

use crate::error::TradingError;

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

}
