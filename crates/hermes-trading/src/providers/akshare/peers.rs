//! Industry peer table via Eastmoney valuation comparison (akshare).

use akshare::stock::eastmoney_misc::PeerComparison;
use serde_json::{Value, json};

use crate::error::TradingError;
use crate::symbol::normalize_symbol;

use super::{client, map_err};

/// Eastmoney datacenter symbol prefix: `SH600519` / `SZ000895`.
pub fn em_prefix_symbol(symbol: &str) -> Result<String, TradingError> {
    let canon = normalize_symbol(symbol);
    let Some((code, market)) = canon.split_once('.') else {
        return Err(TradingError::SymbolNotFound(format!(
            "invalid A-share symbol: {symbol}"
        )));
    };
    Ok(format!("{market}{code}"))
}

pub async fn fetch_peer_table(symbol: &str) -> Result<Vec<Value>, TradingError> {
    let em_sym = em_prefix_symbol(symbol)?;
    let rows = client()
        .stock_zh_valuation_comparison_em(&em_sym)
        .await
        .map_err(map_err)?;
    Ok(map_peer_table(&rows))
}

pub fn map_peer_table(rows: &[PeerComparison]) -> Vec<Value> {
    rows.iter()
        .filter_map(|row| {
            let name = row.name.clone()?;
            let pe = metric_f64(&row.metrics, &["PE_TTM", "PE", "市盈率", "市盈率TTM"]);
            let pb = metric_f64(&row.metrics, &["PB_MRQ", "PB", "市净率"]);
            Some(json!({
                "name": name,
                "ticker": row.symbol,
                "pe": pe,
                "pb": pb,
            }))
        })
        .collect()
}

pub async fn fetch_industry_growth(symbol: &str) -> Result<Option<f64>, TradingError> {
    let em_sym = em_prefix_symbol(symbol)?;
    let rows = client()
        .stock_zh_growth_comparison_em(&em_sym)
        .await
        .map_err(map_err)?;
    Ok(growth_from_comparison(&rows))
}

fn growth_from_comparison(rows: &[PeerComparison]) -> Option<f64> {
    rows.first().and_then(|row| {
        row.metrics
            .get("NETPROFIT_GROWTHRATE")
            .or_else(|| row.metrics.get("GROWTH_RATE"))
            .and_then(|v| v.as_f64())
    })
}

/// Median PE from a peer table JSON array.
#[must_use]
pub fn median_peer_pe(peer_table: &[Value]) -> Option<f64> {
    let mut pes: Vec<f64> = peer_table
        .iter()
        .filter_map(|r| r.get("pe").and_then(|v| v.as_f64()))
        .filter(|v| v.is_finite() && *v > 0.0)
        .collect();
    if pes.is_empty() {
        return None;
    }
    pes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = pes.len() / 2;
    Some(if pes.len().is_multiple_of(2) {
        (pes[mid - 1] + pes[mid]) / 2.0
    } else {
        pes[mid]
    })
}

fn metric_f64(metrics: &std::collections::HashMap<String, Value>, keys: &[&str]) -> Option<f64> {
    for key in keys {
        if let Some(v) = metrics.get(*key).and_then(|v| v.as_f64()) {
            return Some(v);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn map_peer_table_fixture() {
        let mut metrics = HashMap::new();
        metrics.insert("PE_TTM".into(), json!(18.5));
        metrics.insert("PB_MRQ".into(), json!(4.2));
        let rows = vec![
            PeerComparison {
                symbol: Some("600809.SH".into()),
                name: Some("山西汾酒".into()),
                metrics: metrics.clone(),
            },
            PeerComparison {
                symbol: Some("000858.SZ".into()),
                name: Some("五粮液".into()),
                metrics,
            },
        ];
        let table = map_peer_table(&rows);
        assert_eq!(table.len(), 2);
        assert_eq!(table[0].get("pe").and_then(|v| v.as_f64()), Some(18.5));
        assert_eq!(median_peer_pe(&table), Some(18.5));
    }

    #[test]
    fn em_prefix_symbol_maps() {
        assert_eq!(em_prefix_symbol("600519.SH").unwrap(), "SH600519");
        assert_eq!(em_prefix_symbol("000895.SZ").unwrap(), "SZ000895");
    }
}
