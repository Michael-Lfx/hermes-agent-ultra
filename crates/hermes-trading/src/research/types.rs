//! Type contracts for equity research — all inputs optional with provenance tracking.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::quote_data::QuoteData;

/// Where a field value came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceSource {
    Quote,
    Web,
    Provider,
    Computed,
}

/// Weighted field requirements for `DataConfidence` (aligned with UZI dim weights).
const CONFIDENCE_FIELDS: &[(&str, f64)] = &[
    ("price", 5.0),
    ("fcf_latest_yi", 5.0),
    ("revenue_latest_yi", 5.0),
    ("net_margin", 4.0),
    ("roe_latest", 4.0),
    ("market_cap_yi", 4.0),
    ("shares_outstanding_yi", 3.0),
    ("pe", 5.0),
    ("pe_quantile_5y", 3.0),
    ("pb", 3.0),
    ("debt_ratio", 3.0),
    ("total_debt_yi", 2.0),
    ("cash_yi", 2.0),
    ("ebitda_yi", 3.0),
    ("equity_yi", 3.0),
    ("eps", 3.0),
    ("bvps", 2.0),
    ("industry", 2.0),
            "market_cap_yi" => self.market_cap_yi.is_some(),
            "shares_outstanding_yi" => self.shares_outstanding_yi.is_some(),
            "total_debt_yi" => self.total_debt_yi.is_some(),
            "cash_yi" => self.cash_yi.is_some(),
            "pe" => self.pe.is_some(),
            "pb" => self.pb.is_some(),
            "ebitda_yi" => self.ebitda_yi.is_some(),
            "equity_yi" => self.equity_yi.is_some(),
            "eps" => self.eps.is_some(),
            "bvps" => self.bvps.is_some(),
            _ => false,
        }
    }

    /// Build from live quote (price, pe).
    #[must_use]
    pub fn from_quote(quote: &QuoteData) -> Self {
        let mut snap = Self {
            symbol: quote.symbol.clone(),
            name: quote.short_name.clone(),
            price: quote.price,
            pe: quote.pe_ratio,
            ..Default::default()
        };
        if let Some(p) = quote.price {
            snap.provenance
                .insert("price".into(), ProvenanceSource::Quote);
            if quote.pe_ratio.is_some() {
                snap.provenance.insert("pe".into(), ProvenanceSource::Quote);
            }
            snap.change_pct = quote.change_pct;
            let _ = p;
        }
        snap
    }

    /// Merge optional JSON from web_search / agent (snake_case keys).
    pub fn merge_json(&mut self, value: &Value) {
        let obj = match value.as_object() {
            Some(o) => o,
            None => return,
        };
        macro_rules! merge_f64 {
            ($field:ident) => {
                if let Some(v) = obj.get(stringify!($field)).and_then(|v| v.as_f64()) {
                    self.$field = Some(v);
                    self.provenance
                        .insert(stringify!($field).into(), ProvenanceSource::Web);
                }
            };
        }
        merge_f64!(price);
        merge_f64!(market_cap_yi);
        merge_f64!(shares_outstanding_yi);
        merge_f64!(fcf_latest_yi);
        merge_f64!(revenue_latest_yi);
        merge_f64!(net_margin);
        merge_f64!(total_debt_yi);
        merge_f64!(cash_yi);
        merge_f64!(ebitda_yi);
        merge_f64!(equity_yi);
        merge_f64!(pe);
        merge_f64!(pb);
        merge_f64!(ps);
        merge_f64!(eps);
        merge_f64!(bvps);
        merge_f64!(roe_latest);
        merge_f64!(debt_ratio);
        merge_f64!(moat_total);
        merge_f64!(pe_quantile_5y);
        if let Some(s) = obj.get("name").and_then(|v| v.as_str()) {
            self.name = Some(s.to_string());
        }
        if let Some(s) = obj.get("industry").and_then(|v| v.as_str()) {
            self.industry = Some(s.to_string());
        }
        if let Some(arr) = obj.get("matched_youzi").and_then(|v| v.as_array()) {
            self.matched_youzi = arr
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect();
        }
        if let Some(b) = obj.get("fcf_positive").and_then(|v| v.as_bool()) {
            self.fcf_positive = Some(b);
        }
    }

    /// Merge provider snapshot fields (P1).
    pub fn merge_provider_snapshot(&mut self, part: &FundamentalsSnapshot) {
        macro_rules! merge_opt {
            ($field:ident) => {
                if part.$field.is_some() {
                    self.$field = part.$field;
                    if let Some(src) = part.provenance.get(stringify!($field)) {
                        self.provenance.insert(stringify!($field).into(), *src);
                    }
                }
            };
        }
        merge_opt!(revenue_latest_yi);
        merge_opt!(net_margin);
        merge_opt!(gross_margin);
        merge_opt!(equity_yi);
        merge_opt!(total_debt_yi);
        merge_opt!(cash_yi);
        merge_opt!(fcf_latest_yi);
        merge_opt!(market_cap_yi);
        merge_opt!(shares_outstanding_yi);
        merge_opt!(roe_latest);
        merge_opt!(debt_ratio);
        merge_opt!(current_ratio);
        merge_opt!(fcf_margin);
        merge_opt!(pe);
        merge_opt!(pb);
        merge_opt!(eps);
        merge_opt!(bvps);
        merge_opt!(pe_quantile_5y);
        merge_opt!(industry_pe);
        merge_opt!(price);
        if !part.roe_history.is_empty() {
            self.roe_history.clone_from(&part.roe_history);
        }
        if !part.revenue_history.is_empty() {
            self.revenue_history.clone_from(&part.revenue_history);
        }
        if !part.matched_youzi.is_empty() {
            self.matched_youzi.clone_from(&part.matched_youzi);
        }
    }

    /// Convert to flat map for rule message formatting.
    #[must_use]
    pub fn as_format_map(&self) -> BTreeMap<String, String> {
        let mut m = BTreeMap::new();
        macro_rules! insert_opt {
            ($key:expr, $val:expr) => {
                if let Some(v) = $val {
                    m.insert($key.into(), format!("{v:.1}"));
                }
            };
        }
        insert_opt!("price", self.price);
        insert_opt!("pe", self.pe);
        insert_opt!("pb", self.pb);
        insert_opt!("net_margin", self.net_margin);
        insert_opt!("roe_5y_min", self.roe_5y_min);
        insert_opt!("roe_5y_above_15", self.roe_5y_above_15);
        insert_opt!("debt_ratio", self.debt_ratio);
        insert_opt!("fcf_margin", self.fcf_margin);
        insert_opt!("moat_total", self.moat_total);
        insert_opt!("pe_quantile_5y", self.pe_quantile_5y);
        insert_opt!(
            "consecutive_dividend_years",
            self.consecutive_dividend_years
        );
        insert_opt!("pe_x_pb", self.pe_x_pb);
        m
    }
}

/// DCF assumption overrides (UZI fin_models defaults).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DcfAssumptions {
    pub stage1_growth: f64,
    pub stage2_growth: f64,
    pub stage1_years: u32,
    pub stage2_years: u32,
    pub terminal_g: f64,
    pub beta: f64,
    pub tax: f64,
    pub target_debt_ratio: f64,
}

impl Default for DcfAssumptions {
    fn default() -> Self {
        Self {
            stage1_growth: 0.10,
            stage2_growth: 0.05,
            stage1_years: 5,
            stage2_years: 5,
            terminal_g: 0.025,
            beta: 1.0,
            tax: 0.25,
            target_debt_ratio: 0.30,
        }
    }
}

/// Trait for model outputs that track fallback paths.
pub trait HasFallbacks {
    fn used_fallback(&self) -> &[String];
}
