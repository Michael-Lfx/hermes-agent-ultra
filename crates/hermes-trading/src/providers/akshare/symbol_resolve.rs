//! Resolve Chinese name or partial query → canonical A-share symbol.

use std::sync::OnceLock;

use tokio::sync::RwLock;

use crate::error::TradingError;
use crate::settlement::is_a_share;
use crate::symbol::normalize_symbol;

use super::{client, map_err};

struct NameIndex {
    entries: Vec<(String, String)>,
}

static INDEX: OnceLock<RwLock<Option<NameIndex>>> = OnceLock::new();

fn index_cell() -> &'static RwLock<Option<NameIndex>> {
    INDEX.get_or_init(|| RwLock::new(None))
}

async fn ensure_index() -> Result<(), TradingError> {
    {
        let guard = index_cell().read().await;
        if guard.is_some() {
            return Ok(());
        }
    }
    let mut guard = index_cell().write().await;
    if guard.is_some() {
        return Ok(());
    }
    let stocks = client().stock_info_a_code_name().await.map_err(map_err)?;
    let entries = stocks
        .into_iter()
        .map(|s| {
            let suffix = if s.code.starts_with('6') || s.code.starts_with('9') {
                "SH"
            } else {
                "SZ"
            };
            (s.name, format!("{}.{}", s.code, suffix))
        })
        .collect();
    *guard = Some(NameIndex { entries });
    Ok(())
}

/// Resolve user query (name, code, or `600519.SH`) to canonical symbol.
pub async fn resolve_a_share_symbol(query: &str) -> Result<String, TradingError> {
    let q = query.trim();
    if q.is_empty() {
        return Err(TradingError::SymbolNotFound("empty symbol query".into()));
    }

    let canon = normalize_symbol(q);
    if is_a_share(&canon) {
        return Ok(canon);
    }

    let digits: String = q.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() == 6 {
        let suffix = if digits.starts_with('6') || digits.starts_with('9') {
            "SH"
        } else {
            "SZ"
        };
        let sym = format!("{digits}.{suffix}");
        if is_a_share(&sym) {
            return Ok(sym);
        }
    }

    ensure_index().await?;
    let guard = index_cell().read().await;
    let entries = guard
        .as_ref()
        .map(|i| &i.entries)
        .ok_or_else(|| TradingError::NoData)?;

    let q_norm: String = q.chars().filter(|c| !c.is_whitespace()).collect();

    // Exact name match first.
    if let Some((_, sym)) = entries.iter().find(|(name, _)| name == &q_norm) {
        return Ok(sym.clone());
    }

    // Substring match — prefer shortest name (more specific).
    let mut matches: Vec<_> = entries
        .iter()
        .filter(|(name, _)| name.contains(&q_norm) || q_norm.contains(name.as_str()))
        .collect();
    matches.sort_by_key(|(name, _)| name.len());
    if let Some((_, sym)) = matches.first() {
        return Ok((*sym).clone());
    }

    Err(TradingError::SymbolNotFound(format!(
        "no A-share match for '{query}'"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_code_formats() {
        assert_eq!(normalize_symbol("600519.SH"), "600519.SH");
        assert!(is_a_share(&normalize_symbol("600519.SH")));
    }
}
