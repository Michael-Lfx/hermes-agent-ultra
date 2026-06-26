//! Resolve company name + live price for user-facing report headers.

use serde_json::Value;

use crate::research::analyze::AnalyzeStockResult;
use crate::research::types::FundamentalsSnapshot;

/// Display identity for brief / HTML hero (name, symbol, price).
#[derive(Debug, Clone, PartialEq)]
pub struct ReportIdentity {
    pub company_name: Option<String>,
    pub symbol: String,
    pub price: Option<f64>,
    pub change_pct: Option<f64>,
    pub industry: Option<String>,
}

impl ReportIdentity {
    #[must_use]
    pub fn from_snapshot(snap: &FundamentalsSnapshot) -> Self {
        Self {
            company_name: snap.name.clone(),
            symbol: snap.symbol.clone(),
            price: snap.price,
            change_pct: snap.change_pct,
            industry: snap.industry.clone(),
        }
    }

    #[must_use]
    pub fn from_analyze_result(result: &AnalyzeStockResult) -> Self {
        let mut id = Self {
            company_name: None,
            symbol: result.symbol.clone(),
            price: None,
            change_pct: None,
            industry: None,
        };
        id.enrich_from_dcf(&result.dcf);
        id.enrich_from_comps(&result.comps);
        id
    }

    pub fn enrich_from_dcf(&mut self, dcf: &Value) {
        if self.price.is_none() {
            self.price = dcf.get("current_price").and_then(|v| v.as_f64());
        }
    }

    pub fn enrich_from_comps(&mut self, comps: &Value) {
        if self.price.is_none() {
            self.price = comps.get("current_price").and_then(|v| v.as_f64());
        }
        if let Some(target) = comps.get("target") {
            if self.company_name.is_none() {
                self.company_name = target
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
            }
            if self.price.is_none() {
                self.price = target.get("price").and_then(|v| v.as_f64());
            }
        }
        if self.company_name.is_none() {
            self.company_name = peer_name_for_symbol(comps, &self.symbol);
        }
    }

    /// Hero / brief title prefix, e.g. `华海药业 · 600521.SH · 现价 ¥14.01`.
    #[must_use]
    pub fn title_prefix(&self) -> String {
        match (&self.company_name, self.price) {
            (Some(name), Some(price)) => {
                format!("{name} · {} · 现价 ¥{price:.2}", self.symbol)
            }
            (Some(name), None) => format!("{name} · {}", self.symbol),
            (None, Some(price)) => format!("{} · 现价 ¥{price:.2}", self.symbol),
            (None, None) => self.symbol.clone(),
        }
    }

    #[must_use]
    pub fn html_document_title(&self) -> String {
        match &self.company_name {
            Some(name) => format!("{name} ({}) · 研报", self.symbol),
            None => format!("{} · 研报", self.symbol),
        }
    }
}

#[must_use]
pub fn infer_target_name_from_peers(
    peers: &[crate::research::models::CompsPeer],
    symbol: &str,
) -> Option<String> {
    let code = symbol_code6(symbol);
    for peer in peers {
        let tick = peer.ticker.as_deref().unwrap_or("");
        if tick == code || tick == symbol {
            return peer.name.clone();
        }
    }
    None
}

fn symbol_code6(symbol: &str) -> &str {
    symbol.split('.').next().unwrap_or(symbol)
}

fn peer_name_for_symbol(comps: &Value, symbol: &str) -> Option<String> {
    let code = symbol_code6(symbol);
    let peers = comps.get("peers")?.as_array()?;
    for peer in peers {
        let tick = peer.get("ticker").and_then(|v| v.as_str()).unwrap_or("");
        if tick == code || tick == symbol {
            return peer
                .get("name")
                .and_then(|v| v.as_str())
                .map(str::to_string);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::research::models::CompsPeer;
    use serde_json::json;

    #[test]
    fn title_prefix_includes_name_and_price() {
        let id = ReportIdentity {
            company_name: Some("华海药业".into()),
            symbol: "600521.SH".into(),
            price: Some(14.01),
            change_pct: None,
            industry: None,
        };
        assert!(id.title_prefix().contains("华海药业"));
        assert!(id.title_prefix().contains("14.01"));
    }

    #[test]
    fn infer_name_from_peer_table() {
        let peers = vec![CompsPeer {
            name: Some("华海药业".into()),
            ticker: Some("600521".into()),
            ..Default::default()
        }];
        assert_eq!(
            infer_target_name_from_peers(&peers, "600521.SH").as_deref(),
            Some("华海药业")
        );
    }

    #[test]
    fn enrich_from_comps_fills_missing_name() {
        let mut id = ReportIdentity {
            company_name: None,
            symbol: "600521.SH".into(),
            price: Some(14.01),
            change_pct: None,
            industry: None,
        };
        let comps = json!({
            "current_price": 14.01,
            "target": { "name": null, "price": 14.01 },
            "peers": [{ "name": "华海药业", "ticker": "600521" }]
        });
        id.enrich_from_comps(&comps);
        assert_eq!(id.company_name.as_deref(), Some("华海药业"));
    }
}
