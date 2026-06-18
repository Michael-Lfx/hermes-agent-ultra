//! Valuation percentiles via Baidu historical series (akshare).

use serde_json::{Value, json};

use crate::error::TradingError;

use super::{client, code6, map_err};

/// Compute historical percentile rank (0–100) for `current` within `series`.
#[must_use]
pub fn percentile_rank(series: &[f64], current: f64) -> Option<f64> {
    if series.is_empty() || !current.is_finite() {
        return None;
    }
    let valid: Vec<f64> = series
        .iter()
        .copied()
        .filter(|v| v.is_finite() && *v > 0.0)
        .collect();
    if valid.is_empty() {
        return None;
    }
    let below = valid.iter().filter(|&&v| v <= current).count();
    Some(below as f64 / valid.len() as f64 * 100.0)
}

pub async fn fetch_valuation_percentiles(
    symbol: &str,
    pe_current: Option<f64>,
    pb_current: Option<f64>,
) -> Result<Value, TradingError> {
    let code = code6(symbol)?;
    let pe_series = client()
        .stock_zh_valuation_baidu(&code, "市盈率(TTM)", "近五年")
        .await
        .map_err(map_err)?;
    let pb_series = client()
        .stock_zh_valuation_baidu(&code, "市净率", "近五年")
        .await
        .map_err(map_err)?;

    let pe_vals: Vec<f64> = pe_series.iter().map(|p| p.value).collect();
    let pb_vals: Vec<f64> = pb_series.iter().map(|p| p.value).collect();

    Ok(json!({
        "pe_percentile": pe_current.and_then(|c| percentile_rank(&pe_vals, c)),
        "pb_percentile": pb_current.and_then(|c| percentile_rank(&pb_vals, c)),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_rank_known_series() {
        let series = vec![10.0, 20.0, 30.0, 40.0, 50.0];
        assert_eq!(percentile_rank(&series, 30.0), Some(60.0));
        assert_eq!(percentile_rank(&series, 5.0), Some(0.0));
        assert_eq!(percentile_rank(&series, 50.0), Some(100.0));
        assert_eq!(percentile_rank(&[], 10.0), None);
    }
}
