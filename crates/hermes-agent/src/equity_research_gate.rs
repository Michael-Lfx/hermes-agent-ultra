//! Listed-equity research tool ordering (no user-message keyword routing).
//!
//! When `analyze_stock` is available, defer `web_search` / `web_extract` until
//! `analyze_stock` has run for an A-share symbol touched via `get_quote`, `get_market_data`,
//! `resolve_a_share_symbol`, seeded from the user message, or when the search query references a ticker.

use hermes_core::ToolSchema;
use hermes_core::{ToolCall, ToolResult};
use serde_json::Value;

const BLOCK_MSG: &str = "Listed-equity pipeline: call analyze_stock(symbol, use_providers=true) before web_search/web_extract. \
analyze_stock fetches hard data + DCF + scoring; use web_search only after it returns, to fill gaps in missing_dims / dim_summary / data_confidence.";

#[derive(Debug, Clone, Default)]
pub struct EquityResearchGate {
    enabled: bool,
    pending_symbol: Option<String>,
    analyze_done: bool,
    /// `/quick-scan` (depth=lite): do not defer web tools — skill forbids web anyway.
    lite_mode: bool,
    /// Parsed from last `analyze_stock` when `data_confidence.score < 0.5`.
    low_confidence_hint: Option<String>,
}

impl EquityResearchGate {
    #[must_use]
    pub fn from_tool_schemas(schemas: &[ToolSchema]) -> Self {
        let enabled = schemas.iter().any(|t| t.name == "analyze_stock");
        Self {
            enabled,
            ..Default::default()
        }
    }

    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Seed symbol from user-message resolution (not keyword routing).
    pub fn seed_pending_symbol(&mut self, symbol: &str) {
        if self.enabled && is_a_share_symbol(symbol) {
            self.pending_symbol = Some(symbol.to_string());
        }
    }

    /// Remove deferred web tools; returns synthetic error results for the model.
    pub fn gate_tool_calls(&mut self, tool_calls: &mut Vec<ToolCall>) -> Vec<ToolResult> {
        if !self.enabled || self.lite_mode {
            return Vec::new();
        }

        self.ingest_symbol_tools(tool_calls);

        if self.analyze_done {
            return Vec::new();
        }

        let mut blocked = Vec::new();
        let mut kept = Vec::new();
        for tc in tool_calls.drain(..) {
            if self.should_block_web_tool(&tc) {
                blocked.push(ToolResult::err(tc.id.clone(), &self.block_message()));
            } else {
                kept.push(tc);
            }
        }
        *tool_calls = kept;
        blocked
    }

    pub fn record_tool_batch(&mut self, tool_calls: &[ToolCall], results: &[ToolResult]) {
        if !self.enabled {
            return;
        }
        for (tc, result) in tool_calls.iter().zip(results.iter()) {
            if result.is_error {
                continue;
            }
            match tc.function.name.as_str() {
                "get_quote" | "resolve_a_share_symbol" | "get_market_data" => {
                    if let Some(sym) = symbol_from_tool_json(&result.content) {
                        if is_a_share_symbol(&sym) {
                            self.pending_symbol = Some(sym);
                        }
                    }
                }
                "analyze_stock" => {
                    self.analyze_done = true;
                    if let Ok(args) = serde_json::from_str::<Value>(&tc.function.arguments) {
                        if is_lite_depth(args.get("depth").and_then(|v| v.as_str())) {
                            self.lite_mode = true;
                        }
                    }
                    if let Some(sym) = symbol_from_tool_json(&result.content) {
                        self.pending_symbol = Some(sym);
                    }
                    if depth_from_result(&result.content).as_deref() == Some("lite") {
                        self.lite_mode = true;
                    }
                    self.low_confidence_hint = low_confidence_hint_from_result(&result.content);
                }
                _ => {}
            }
        }
    }

    fn ingest_symbol_tools(&mut self, tool_calls: &[ToolCall]) {
        for tc in tool_calls {
            match tc.function.name.as_str() {
                "get_quote" | "resolve_a_share_symbol" | "get_market_data" => {
                    if let Ok(args) = serde_json::from_str::<Value>(&tc.function.arguments) {
                        if let Some(sym) = args.get("symbol").and_then(|v| v.as_str()) {
                            if is_a_share_symbol(sym) {
                                self.pending_symbol = Some(sym.to_string());
                            }
                        } else if tc.function.name == "resolve_a_share_symbol" {
                            if let Some(q) = args.get("query").and_then(|v| v.as_str()) {
                                if let Some(sym) = six_digit_a_share(q) {
                                    self.pending_symbol = Some(sym);
                                }
                            }
                        }
                    }
                }
                "analyze_stock" => {
                    if let Ok(args) = serde_json::from_str::<Value>(&tc.function.arguments) {
                        if let Some(sym) = args.get("symbol").and_then(|v| v.as_str()) {
                            self.pending_symbol = Some(sym.to_string());
                        }
                        if is_lite_depth(args.get("depth").and_then(|v| v.as_str())) {
                            self.lite_mode = true;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn should_block_web_tool(&self, tc: &ToolCall) -> bool {
        if self.analyze_done {
            return false;
        }
        let name = tc.function.name.as_str();
        if !matches!(name, "web_search" | "web_extract") {
            return false;
        }
        if self.pending_symbol.is_some() {
            return true;
        }
        if name == "web_search" {
            if let Ok(args) = serde_json::from_str::<Value>(&tc.function.arguments) {
                if let Some(q) = args.get("query").and_then(|v| v.as_str()) {
                    return query_references_listed_ticker(q);
                }
            }
        }
        false
    }

    fn block_message(&self) -> String {
        if let Some(hint) = &self.low_confidence_hint {
            format!("{BLOCK_MSG}\n{hint}")
        } else {
            BLOCK_MSG.into()
        }
    }

    /// Hint from last low-confidence `analyze_stock` (missing_dims / dim_summary).
    #[must_use]
    pub fn low_confidence_hint(&self) -> Option<&str> {
        self.low_confidence_hint.as_deref()
    }
}

fn symbol_from_tool_json(content: &str) -> Option<String> {
    let v: Value = serde_json::from_str(content).ok()?;
    v.get("symbol").and_then(|s| s.as_str()).map(str::to_string)
}

fn depth_from_result(content: &str) -> Option<String> {
    let v: Value = serde_json::from_str(content).ok()?;
    v.get("depth").and_then(|s| s.as_str()).map(str::to_string)
}

fn low_confidence_hint_from_result(content: &str) -> Option<String> {
    let v: Value = serde_json::from_str(content).ok()?;
    let score = v.get("data_confidence")?.get("score")?.as_f64()?;
    if score >= 0.5 {
        return None;
    }
    let missing: Vec<String> = v
        .get("missing_dims")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    if missing.is_empty() {
        return Some(format!(
            "Last analyze_stock data_confidence={score:.2} (<0.5): check dim_summary and use web_search for qualitative gaps."
        ));
    }
    Some(format!(
        "Last analyze_stock data_confidence={score:.2}; missing_dims=[{}] — web_search to fill before final narrative.",
        missing.join(", ")
    ))
}

fn is_lite_depth(depth: Option<&str>) -> bool {
    matches!(
        depth.map(str::trim).map(str::to_ascii_lowercase).as_deref(),
        Some("lite" | "quick" | "quick-scan")
    )
}

fn is_a_share_symbol(sym: &str) -> bool {
    sym.ends_with(".SH") || sym.ends_with(".SZ")
}

fn six_digit_a_share(query: &str) -> Option<String> {
    let digits: String = query.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() != 6 {
        return None;
    }
    let suffix = if digits.starts_with('6') || digits.starts_with('9') {
        "SH"
    } else {
        "SZ"
    };
    Some(format!("{digits}.{suffix}"))
}

fn query_references_listed_ticker(query: &str) -> bool {
    if query.contains(".SH") || query.contains(".SZ") {
        return true;
    }
    let bytes = query.as_bytes();
    bytes
        .windows(6)
        .any(|w| w.iter().all(|c| c.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hermes_core::FunctionCall;
    use hermes_core::JsonSchema;

    fn tc(id: &str, name: &str, args: &str) -> ToolCall {
        ToolCall {
            id: id.into(),
            function: FunctionCall {
                name: name.into(),
                arguments: args.into(),
            },
            extra_content: None,
        }
    }

    #[test]
    fn blocks_web_after_get_market_data_until_analyze() {
        let schemas = vec![ToolSchema::new(
            "analyze_stock",
            "",
            JsonSchema::new("object"),
        )];
        let mut gate = EquityResearchGate::from_tool_schemas(&schemas);
        let md_ok = ToolResult {
            tool_call_id: "1".into(),
            content: r#"{"symbol":"600584.SH","rows":[]}"#.into(),
            is_error: false,
        };
        gate.record_tool_batch(
            &[tc("1", "get_market_data", r#"{"symbol":"600584.SH"}"#)],
            &[md_ok],
        );
        let mut batch = vec![tc("2", "web_search", r#"{"query":"news"}"#)];
        let blocked = gate.gate_tool_calls(&mut batch);
        assert_eq!(blocked.len(), 1);
    }

    #[test]
    fn seed_from_user_message_blocks_web() {
        let schemas = vec![ToolSchema::new(
            "analyze_stock",
            "",
            JsonSchema::new("object"),
        )];
        let mut gate = EquityResearchGate::from_tool_schemas(&schemas);
        gate.seed_pending_symbol("600584.SH");
        let mut batch = vec![tc("1", "web_search", r#"{"query":"长电科技"}"#)];
        let blocked = gate.gate_tool_calls(&mut batch);
        assert_eq!(blocked.len(), 1);
    }

    #[test]
    fn blocks_web_after_a_share_quote_until_analyze() {
        let schemas = vec![ToolSchema::new(
            "analyze_stock",
            "",
            JsonSchema::new("object"),
        )];
        let mut gate = EquityResearchGate::from_tool_schemas(&schemas);
        let quote_ok = ToolResult {
            tool_call_id: "1".into(),
            content: r#"{"symbol":"300750.SZ","price":1.0}"#.into(),
            is_error: false,
        };
        gate.record_tool_batch(
            &[tc("1", "get_quote", r#"{"symbol":"300750.SZ"}"#)],
            &[quote_ok],
        );

        let mut batch = vec![tc("2", "web_search", r#"{"query":"foo"}"#)];
        let blocked = gate.gate_tool_calls(&mut batch);
        assert_eq!(blocked.len(), 1);
        assert!(batch.is_empty());

        let analyze_ok = ToolResult {
            tool_call_id: "3".into(),
            content: r#"{"symbol":"300750.SZ","dcf":{}}"#.into(),
            is_error: false,
        };
        gate.record_tool_batch(
            &[tc("3", "analyze_stock", r#"{"symbol":"300750.SZ"}"#)],
            &[analyze_ok],
        );
        let mut batch2 = vec![tc("4", "web_search", r#"{"query":"gap fill"}"#)];
        let blocked2 = gate.gate_tool_calls(&mut batch2);
        assert!(blocked2.is_empty());
        assert_eq!(batch2.len(), 1);
    }

    #[test]
    fn blocks_web_when_query_has_ticker_code() {
        let schemas = vec![ToolSchema::new(
            "analyze_stock",
            "",
            JsonSchema::new("object"),
        )];
        let mut gate = EquityResearchGate::from_tool_schemas(&schemas);
        let mut batch = vec![tc("1", "web_search", r#"{"query":"300750 earnings"}"#)];
        let blocked = gate.gate_tool_calls(&mut batch);
        assert_eq!(blocked.len(), 1);
    }

    #[test]
    fn lite_depth_disables_web_gate() {
        let schemas = vec![ToolSchema::new(
            "analyze_stock",
            "",
            JsonSchema::new("object"),
        )];
        let mut gate = EquityResearchGate::from_tool_schemas(&schemas);
        gate.seed_pending_symbol("688126.SH");
        gate.lite_mode = true;
        let mut batch = vec![tc("1", "web_search", r#"{"query":"news"}"#)];
        let blocked = gate.gate_tool_calls(&mut batch);
        assert!(blocked.is_empty());
        assert_eq!(batch.len(), 1);
    }

    #[test]
    fn analyze_stock_lite_arg_disables_gate() {
        let schemas = vec![ToolSchema::new(
            "analyze_stock",
            "",
            JsonSchema::new("object"),
        )];
        let mut gate = EquityResearchGate::from_tool_schemas(&schemas);
        gate.seed_pending_symbol("688126.SH");
        let mut batch = vec![tc(
            "1",
            "analyze_stock",
            r#"{"symbol":"688126.SH","depth":"lite"}"#,
        )];
        let _ = gate.gate_tool_calls(&mut batch);
        let mut web = vec![tc("2", "web_search", r#"{"query":"foo"}"#)];
        let blocked = gate.gate_tool_calls(&mut web);
        assert!(blocked.is_empty());
    }

    #[test]
    fn no_block_without_analyze_stock_tool() {
        let mut gate = EquityResearchGate::from_tool_schemas(&[]);
        let mut batch = vec![tc("1", "web_search", r#"{"query":"300750"}"#)];
        let blocked = gate.gate_tool_calls(&mut batch);
        assert!(blocked.is_empty());
        assert_eq!(batch.len(), 1);
    }
}
