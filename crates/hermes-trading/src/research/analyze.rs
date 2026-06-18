//! End-to-end stock analysis orchestrator.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::quote_data::QuoteData;
use crate::research::models::{
    CompsPeer, CompsTarget, ThreeStmtResult, build_comps_table, compute_dcf, project_three_stmt,
    quick_lbo,
};
use crate::research::report::render_summary_markdown;
=======
use crate::research::scoring::{generate_panel, score_dimensions};
use crate::research::types::{DataConfidence, FeatureVector, FundamentalsSnapshot};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzeStockResult {
    pub symbol: String,
    pub dcf: Value,
    pub comps: Value,
    pub three_statement: Value,
    pub lbo: Value,
    pub scores: Value,
    pub personas: Value,
    pub data_confidence: DataConfidence,
    pub missing_dims: Vec<String>,
    pub used_fallback: Vec<String>,
    /// Deterministic 19-dim + 66-panel Markdown for chat (do not shorten).
    pub summary_markdown: String,
>>>>>>> 7062cddeb (﻿feat(trading): equity research orchestration and full 19-dim report)
}

/// Run full analysis pipeline on a fundamentals snapshot.
#[must_use]
pub fn analyze_stock(
    snap: &FundamentalsSnapshot,
    raw_dims: Option<&Value>,
    peers: Option<&[CompsPeer]>,
) -> AnalyzeStockResult {
    let features: FeatureVector = snap.clone();
    let mut used_fallback = Vec::new();

    let dcf = compute_dcf(&features, None);
    used_fallback.extend(dcf.used_fallback.clone());

    let target = CompsTarget {
        price: snap.price,
        pe: snap.pe,
        pb: snap.pb,
        eps: snap.eps,
        bvps: snap.bvps,
        ..Default::default()
    };
    let effective_peers: Vec<CompsPeer> = match peers {
        Some(p) if !p.is_empty() => p.to_vec(),
        _ => peers_from_raw_dims(raw_dims).unwrap_or_default(),
    };
    let comps = if effective_peers.is_empty() {
        serde_json::json!({"error": "no peers provided"})
    } else {
        serde_json::to_value(build_comps_table(target, &effective_peers)).unwrap_or(Value::Null)
    };

    let three_stmt = match project_three_stmt(&features, None) {
        ThreeStmtResult::Ok(ok) => {
            used_fallback.extend(ok.used_fallback.clone());
            serde_json::to_value(ok).unwrap_or(Value::Null)
        }
        ThreeStmtResult::Error { error, .. } => {
            serde_json::json!({"error": error})
        }
    };

    let lbo = quick_lbo(&features, None);
    used_fallback.extend(lbo.used_fallback.clone());

    let dims = raw_dims
        .cloned()
        .unwrap_or(Value::Object(Default::default()));
    let scored = score_dimensions(&snap.symbol, &dims, &features);
    let missing_dims: Vec<String> = scored
        .dimensions
        .values()
        .flat_map(|d| d.missing.clone())
        .collect();
    let panel = generate_panel(&scored, &features);
    let data_confidence = DataConfidence::from_snapshot(snap);
    let dcf_verdict = Some(dcf.verdict.as_str());
    let summary_markdown =
        render_summary_markdown(&snap.symbol, &scored, &panel, &data_confidence, dcf_verdict);
<<<<<<< HEAD
    }
}

/// Build snapshot from quote + optional JSON fundamentals.
#[must_use]
pub fn snapshot_from_inputs(
    quote: &QuoteData,
    fundamentals: Option<&Value>,
) -> FundamentalsSnapshot {
    let mut snap = FundamentalsSnapshot::from_quote(quote);
    if let Some(f) = fundamentals {
        snap.merge_json(f);
    }
    snap
}
<<<<<<< HEAD
=======
>>>>>>> d9ae746af (feat(trading): P0 equity fetch — basic Full, valuation/peers/fund_holders, dedupe)
