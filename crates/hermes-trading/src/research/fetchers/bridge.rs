//! Apply dimension results → `FundamentalsSnapshot` + feature fields.

use serde_json::Value;

use super::types::{CollectOutput, DimQuality, DimResult};
use crate::research::types::{FundamentalsSnapshot, ProvenanceSource};

/// Merge all HTTP dimension outputs into one snapshot (additive).
pub fn apply_dims_to_snapshot(snap: &mut FundamentalsSnapshot, output: &CollectOutput) {
    for result in output.dims.values() {
        if matches!(result.quality, DimQuality::Skipped | DimQuality::Error) {
            continue;
        }
        apply_one_dim(snap, result);
    }
}

fn apply_one_dim(snap: &mut FundamentalsSnapshot, result: &DimResult) {
    match result.dim_key.as_str() {
        "4_peers" => apply_peers(snap, &result.data),
        "6_fund_holders" => apply_fund_holders(snap, &result.data),
        "6_research" => apply_research(snap, &result.data),
        "12_capital_flow" => apply_capital_flow(snap, &result.data),
        "15_events" => apply_events(snap, &result.data),
    if let Some(arr) = data.get("roe_history").and_then(|v| v.as_array()) {
        snap.roe_history = arr.iter().filter_map(|v| v.as_f64()).collect();
        if !snap.roe_history.is_empty() {
            mark(snap, "roe_history");
        }
    }
    if let Some(arr) = data.get("revenue_history").and_then(|v| v.as_array()) {
        snap.revenue_history = arr.iter().filter_map(|v| v.as_f64()).collect();
        if !snap.revenue_history.is_empty() {
            mark(snap, "revenue_history");
        }
    }
    if data
        .get("fcf_positive")
        .and_then(|v| v.as_bool())
        .is_some_and(|b| b)
    {
        snap.fcf_positive = Some(true);
        mark(snap, "fcf_positive");
    }
}

fn apply_kline(snap: &mut FundamentalsSnapshot, data: &Value) {
    if let Some(v) = data.get("stage").and_then(|v| v.as_str()) {
        snap.stage = Some(v.to_string());
        mark(snap, "stage");
    }
    if let Some(v) = data.get("ma_align").and_then(|v| v.as_str()) {
        snap.ma_align = Some(v.to_string());
        mark(snap, "ma_align");
    }
    if let Some(stats) = data.get("kline_stats") {
        set_f64(snap, "max_drawdown_1y", stats, "max_drawdown");
    }
}

fn apply_valuation(snap: &mut FundamentalsSnapshot, data: &Value) {
    set_f64(snap, "pe", data, "pe_ttm");
    set_f64(snap, "pb", data, "pb");
    set_f64(snap, "ps", data, "ps_ttm");
    set_f64(snap, "eps", data, "eps");
    set_f64(snap, "bvps", data, "bvps");
    set_f64(snap, "pe_quantile_5y", data, "pe_percentile");
    set_f64(snap, "industry_pe", data, "industry_pe");
}

fn apply_lhb(snap: &mut FundamentalsSnapshot, data: &Value) {
    if let Some(arr) = data.get("matched_youzi").and_then(|v| v.as_array()) {
        snap.matched_youzi = arr
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect();
        if !snap.matched_youzi.is_empty() {
            mark(snap, "matched_youzi");
        }
    }
}

<<<<<<< HEAD
fn apply_peers(snap: &mut FundamentalsSnapshot, data: &Value) {
    if snap.industry_pe.is_none()
        && let Some(median) = data
            .get("peer_table")
            .and_then(|v| v.as_array())
            .and_then(|a| crate::providers::akshare::median_peer_pe(a))
    {
        snap.industry_pe = Some(median);
        mark(snap, "industry_pe");
    }
}

fn apply_fund_holders(snap: &mut FundamentalsSnapshot, data: &Value) {
    if data
        .get("fund_holdings")
        .and_then(|v| v.as_array())
        .is_some_and(|a| !a.is_empty())
    {
        mark(snap, "fund_holdings");
    }
    if data.get("holder_count").and_then(|v| v.as_f64()).is_some() {
        mark(snap, "holder_count");
    }
}

fn apply_research(snap: &mut FundamentalsSnapshot, data: &Value) {
    if data
        .get("research_count")
        .and_then(|v| v.as_u64())
        .is_some_and(|n| n > 0)
    {
        mark(snap, "research_reports");
    }
}

fn apply_capital_flow(snap: &mut FundamentalsSnapshot, data: &Value) {
    if data
        .get("main_fund_5d_net_yi")
        .and_then(|v| v.as_f64())
        .is_some()
    {
        mark(snap, "main_fund_5d");
    }
}

fn apply_events(snap: &mut FundamentalsSnapshot, data: &Value) {
    if data
        .get("announcement_count")
        .and_then(|v| v.as_u64())
        .is_some_and(|n| n > 0)
    {
        mark(snap, "announcements");
    }
}

=======
>>>>>>> d5f5467b3 (feat(trading): UZI equity research engine and analyze_stock tool)
fn apply_industry(snap: &mut FundamentalsSnapshot, data: &Value) {
    if let Some(v) = data.get("industry").and_then(|v| v.as_str()) {
        snap.industry = Some(v.to_string());
        mark(snap, "industry");
    }
    set_f64(snap, "industry_pe", data, "industry_pe");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::research::fetchers::types::{CollectOutput, DimQuality, Market};

    #[test]
    fn build_raw_dims_shape() {
        let mut output = CollectOutput {
            ticker: "600809.SH".into(),
            market: Market::A,
            dims: Default::default(),
        };
        output.dims.insert(
            "1_financials".into(),
            DimResult::ok(
                "1_financials",
                "600809.SH",
                serde_json::json!({"roe": 28.0, "net_margin": 32.0}),
                "eastmoney_financials",
                DimQuality::Partial,
            ),
        );
        let raw = output.build_raw_dims();
        assert!(
            raw.get("1_financials")
                .and_then(|v| v.get("data"))
                .is_some()
        );
    }
}
