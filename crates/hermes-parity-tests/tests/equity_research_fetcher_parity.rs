//! Fetcher mapper golden tests (offline JSON fixtures, no network).

use std::fs;
use std::path::PathBuf;

use serde_json::{Value, json};

use hermes_trading::research::fetchers::bridge::apply_dims_to_snapshot;
use hermes_trading::research::fetchers::types::{CollectOutput, DimQuality, DimResult, Market};
use hermes_trading::research::scoring::score_dimensions;
use hermes_trading::research::types::{DataConfidence, FundamentalsSnapshot};

#[derive(Debug, serde::Deserialize)]
struct FixtureFile {
    cases: Vec<FixtureCase>,
}

#[derive(Debug, serde::Deserialize)]
struct FixtureCase {
    id: String,
    op: String,
    input: Value,
    expected: Value,
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/trading_research_fetch/fetcher_golden.json")
}

fn collect_from_input(input: &Value) -> CollectOutput {
    let symbol = input["symbol"].as_str().unwrap_or("TEST.SH");
    let mut output = CollectOutput {
        ticker: symbol.into(),
        market: Market::A,
        dims: Default::default(),
    };
    if let Some(dims) = input.get("dims").and_then(|v| v.as_object()) {
        for (key, wrapper) in dims {
            let data = wrapper.get("data").cloned().unwrap_or(Value::Null);
            output.dims.insert(
                key.clone(),
                DimResult::ok(key, symbol, data, "fixture", DimQuality::Partial),
            );
        }
    }
    output
}

fn run_bridge_and_score(case: &FixtureCase) {
    let collect = collect_from_input(&case.input);
    let raw_dims = collect.build_raw_dims();
    let mut snap = FundamentalsSnapshot {
        symbol: case.input["symbol"].as_str().unwrap_or("TEST").into(),
        ..Default::default()
    };
    apply_dims_to_snapshot(&mut snap, &collect);
    let confidence = DataConfidence::from_snapshot(&snap);
    let scored = score_dimensions(&snap.symbol, &raw_dims, &snap);
    let exp = &case.expected;

    if let Some(min) = exp.get("min_confidence").and_then(|v| v.as_f64()) {
        assert!(
            confidence.score >= min,
            "{} confidence {} < {min}",
            case.id,
            confidence.score
        );
    }
    if let Some(max) = exp.get("max_confidence").and_then(|v| v.as_f64()) {
        assert!(
            confidence.score <= max,
            "{} confidence {} > {max}",
            case.id,
            confidence.score
        );
    }
    if let Some(min) = exp.get("min_fundamental_score").and_then(|v| v.as_f64()) {
        assert!(
            scored.fundamental_score >= min,
            "{} score {} < {min}",
            case.id,
            scored.fundamental_score
        );
    }
    if let Some(max) = exp.get("max_fundamental_score").and_then(|v| v.as_f64()) {
        assert!(
            scored.fundamental_score <= max,
            "{} score {} > {max}",
            case.id,
            scored.fundamental_score
        );
    }
    if exp
        .get("has_industry_pe")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        assert!(
            snap.industry_pe.is_some(),
            "{} expected industry_pe",
            case.id
        );
    }
    if let Some(lte) = exp.get("pe_dim_score_lte").and_then(|v| v.as_u64()) {
        let pe_score = scored
            .dimensions
            .get("10_valuation")
            .map(|d| d.score)
            .unwrap_or(0);
        assert!(
            u64::from(pe_score) <= lte,
            "{} pe dim {pe_score} > {lte}",
            case.id
        );
    }
}

#[test]
fn equity_research_fetcher_golden() {
    let content = fs::read_to_string(fixture_path()).expect("read fetcher golden");
    let fixture: FixtureFile = serde_json::from_str(&content).expect("parse");
    for case in &fixture.cases {
        match case.op.as_str() {
            "bridge_and_score" => run_bridge_and_score(case),
            other => panic!("unknown op {other}"),
        }
    }
}

#[test]
fn fetcher_golden_raw_dims_shape() {
    let collect = collect_from_input(&json!({
        "symbol": "600519.SH",
        "dims": { "0_basic": { "data": { "price": 1.0 } } }
    }));
    let raw = collect.build_raw_dims();
    assert!(raw.get("0_basic").and_then(|v| v.get("data")).is_some());
}
