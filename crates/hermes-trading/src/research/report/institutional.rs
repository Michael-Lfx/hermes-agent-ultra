//! Institutional standalone HTML report (wave 2b PR-2).

use crate::research::analyze::AnalyzeStockResult;
use crate::research::personas::investors::find_investor;
use crate::research::scoring::{PanelResult, ScoreDimensionsResult};
use crate::research::synthesis::SynthesisReport;

use super::dim_viz::{render_dim_bar, verdict_badge_class, verdict_label_zh};
use super::identity::ReportIdentity;
use super::labels::{DIM_ORDER, dimension_display_name};
use crate::research::report_filter::{scrub_dim_label, show_gaps_section};

const CONFIDENCE_WARN_THRESHOLD: f64 = 0.55;
pub const MAX_HTML_BYTES: usize = 150_000;

/// Render institutional HTML from a completed analysis (uses embedded `synthesis`).
#[must_use]
pub fn render_institutional_html(result: &AnalyzeStockResult, narrative: Option<&str>) -> String {
    let mut result = result.clone();
    crate::research::report_filter::scrub_user_report(&mut result);
    let syn = &result.synthesis;
    let scored: ScoreDimensionsResult =
        serde_json::from_value(result.scores.clone()).unwrap_or(ScoreDimensionsResult {
            ticker: result.symbol.clone(),
            fundamental_score: 0.0,
            dimensions: Default::default(),
        });
    let panel: PanelResult =
        serde_json::from_value(result.personas.clone()).unwrap_or(PanelResult {
            investors: Vec::new(),
            vote_distribution: Default::default(),
            signal_distribution: Default::default(),
            panel_consensus: scored.fundamental_score,
        });
    let dcf = &result.dcf;

    let identity = ReportIdentity::from_analyze_result(&result);

    let mut html = render_shell_start(&identity);
    if result.data_confidence.score < CONFIDENCE_WARN_THRESHOLD {
        html.push_str(&render_warn_banner(result.data_confidence.score, syn));
    }
    html.push_str(&render_cover(syn));
    html.push_str(&render_key_metrics_grid(syn));
    html.push_str(&render_dcf_section(dcf));
    html.push_str(&render_dimensions_section(&scored));
    html.push_str(&render_panel_section(&panel));
    if show_gaps_section(&result.missing_dims, &syn.missing_highlights) {
        html.push_str(&render_gaps_section(
            &result.missing_dims,
            &syn.missing_highlights,
        ));
    }
    if !syn.risks.is_empty() {
        html.push_str(&render_risks_section(&syn.risks));
    }
    if let Some(text) = narrative {
        html.push_str(&render_narrative_section(text));
    }
    html.push_str("</body></html>");
    debug_assert!(
        html.len() <= MAX_HTML_BYTES,
        "institutional HTML exceeds {MAX_HTML_BYTES} bytes"
    );
    html
}

fn render_shell_start(identity: &ReportIdentity) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{}</title>
<style>
:root {{ --ink:#1e293b; --muted:#64748b; --line:#e2e8f0; --bg:#f8fafc; }}
* {{ box-sizing:border-box; }}
body {{ font-family:"Segoe UI",system-ui,sans-serif; margin:0; color:var(--ink); background:var(--bg); }}
.wrap {{ max-width:920px; margin:0 auto; padding:1.5rem; }}
.hero {{ background:#fff; border:1px solid var(--line); border-radius:12px; padding:1.25rem 1.5rem; margin-bottom:1rem; }}
.hero h1 {{ margin:0 0 .5rem; font-size:1.35rem; }}
.badge {{ display:inline-block; padding:.2rem .65rem; border-radius:999px; font-size:.85rem; font-weight:600; }}
.badge-strong-buy {{ background:#dcfce7; color:#166534; }}
.badge-buy {{ background:#dbeafe; color:#1d4ed8; }}
.badge-watch {{ background:#fef9c3; color:#854d0e; }}
.badge-avoid {{ background:#fee2e2; color:#991b1b; }}
.badge-muted {{ background:#f1f5f9; color:#475569; }}
.sub {{ color:var(--muted); font-size:.95rem; margin:.35rem 0 0; }}
.banner {{ background:#fff7ed; border:1px solid #fed7aa; color:#9a3412; padding:.75rem 1rem; border-radius:8px; margin-bottom:1rem; }}
.card {{ background:#fff; border:1px solid var(--line); border-radius:10px; padding:1rem 1.25rem; margin-bottom:1rem; }}
.card h2 {{ margin:0 0 .75rem; font-size:1.05rem; }}
.metrics {{ display:grid; grid-template-columns:repeat(auto-fill,minmax(160px,1fr)); gap:.75rem; }}
.metric {{ background:var(--bg); border-radius:8px; padding:.65rem .75rem; }}
.metric .k {{ font-size:.78rem; color:var(--muted); }}
.metric .v {{ font-size:1rem; font-weight:600; margin-top:.15rem; }}
table {{ width:100%; border-collapse:collapse; font-size:.9rem; }}
th,td {{ border-bottom:1px solid var(--line); padding:.45rem .35rem; text-align:left; vertical-align:middle; }}
th {{ color:var(--muted); font-weight:600; }}
.dim-bar {{ display:inline-block; width:72px; height:8px; background:#e2e8f0; border-radius:4px; overflow:hidden; vertical-align:middle; }}
.dim-fill {{ display:block; height:100%; border-radius:4px; }}
.chips {{ display:flex; flex-wrap:wrap; gap:.35rem; }}
.chip {{ font-size:.78rem; padding:.15rem .5rem; border-radius:999px; }}
.chip-missing {{ background:#fef2f2; color:#991b1b; border:1px solid #fecaca; }}
.gauges {{ margin:.75rem 0; }}
ul.risk {{ margin:.25rem 0 0 1.1rem; padding:0; }}
.narrative {{ line-height:1.6; white-space:pre-wrap; }}
</style>
</head>
<body><div class="wrap">
"#,
        escape_html(&identity.html_document_title()),
    )
}

fn render_warn_banner(confidence: f64, _syn: &SynthesisReport) -> String {
    format!(
        r#"<div class="banner"><strong>数据置信度 {:.0}%</strong> — 基于公开行情与财报数据。</div>"#,
        confidence * 100.0,
    )
}

fn render_cover(syn: &SynthesisReport) -> String {
    let badge_class = verdict_badge_class(&syn.verdict);
    let badge_label = verdict_label_zh(&syn.verdict);
    format!(
        r#"<section class="hero">
<h1>{headline}</h1>
<p><span class="badge {badge_class}">{badge_label}</span>
<span class="sub">置信档位 {tier} · {dcf}</span></p>
</section>"#,
        headline = escape_html(&syn.headline),
        badge_class = badge_class,
        badge_label = badge_label,
        tier = escape_html(&syn.confidence_tier),
        dcf = escape_html(&syn.dcf_one_liner),
    )
}

fn render_key_metrics_grid(syn: &SynthesisReport) -> String {
    let mut rows = String::from(r#"<section class="card"><h2>关键指标</h2><div class="metrics">"#);
    for m in &syn.key_metrics {
        rows.push_str(&format!(
            r#"<div class="metric"><div class="k">{}</div><div class="v">{}</div></div>"#,
            escape_html(&m.label),
            escape_html(&m.value)
        ));
    }
    rows.push_str("</div></section>");
    rows
}

fn render_dcf_section(dcf: &serde_json::Value) -> String {
    let intrinsic = dcf
        .get("intrinsic_per_share")
        .and_then(|v| v.as_f64())
        .map(|v| format!("¥{v:.2}"))
        .unwrap_or_else(|| "—".into());
    let safety = dcf
        .get("safety_margin_pct")
        .and_then(|v| v.as_f64())
        .map(|v| format!("{v:+.1}%"))
        .unwrap_or_else(|| "—".into());
    let center = dcf
        .get("sensitivity_table")
        .and_then(|t| t.get("center_cell"))
        .and_then(|v| v.as_f64())
        .map(|v| format!("¥{v:.2}"))
        .unwrap_or_else(|| "—".into());
    let verdict = dcf.get("verdict").and_then(|v| v.as_str()).unwrap_or("—");
    let fallbacks = dcf
        .get("used_fallback")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "无".into());

    format!(
        r#"<section class="card"><h2>DCF 估值</h2>
<table>
<tr><th>指标</th><th>值</th></tr>
<tr><td>内在价值 (每股)</td><td>{intrinsic}</td></tr>
<tr><td>安全边际</td><td>{safety}</td></tr>
<tr><td>敏感性中心格</td><td>{center}</td></tr>
<tr><td>结论</td><td>{verdict}</td></tr>
<tr><td>模型假设 fallback</td><td>{fallbacks}</td></tr>
</table></section>"#,
        verdict = escape_html(verdict),
        fallbacks = escape_html(&fallbacks),
    )
}

fn render_dimensions_section(scored: &ScoreDimensionsResult) -> String {
    let mut out = String::from(
        r#"<section class="card"><h2>19 维评分</h2>
<table><tr><th>维度</th><th>得分</th><th>说明</th></tr>"#,
    );
    for key in DIM_ORDER {
        let Some(d) = scored.dimensions.get(*key) else {
            continue;
        };
        let name = if d.display_name.is_empty() {
            dimension_display_name(key)
        } else {
            d.display_name.clone()
        };
        let bar = render_dim_bar(d.score, 10);
        out.push_str(&format!(
            "<tr><td>{name}</td><td>{bar} {score}/10</td><td>{}</td></tr>",
            escape_html(&scrub_dim_label(&d.label)),
            score = d.score,
        ));
    }
    out.push_str("</table></section>");
    out
}

fn render_panel_section(panel: &PanelResult) -> String {
    let vd = &panel.vote_distribution;
    let mut out = format!(
        r#"<section class="card"><h2>66 位评委</h2>
<p>共识 <strong>{:.1}/10</strong> · 买入 {} · 回避 {} · 共 {} 位</p>
<table><tr><th>类别</th><th>人数</th></tr>
<tr><td>强烈买入</td><td>{}</td></tr>
<tr><td>买入</td><td>{}</td></tr>
<tr><td>关注</td><td>{}</td></tr>
<tr><td>观望</td><td>{}</td></tr>
<tr><td>回避</td><td>{}</td></tr>
<tr><td>跳过</td><td>{}</td></tr>
</table>"#,
        panel.panel_consensus,
        vd.strongly_buy + vd.buy,
        vd.avoid,
        panel.investors.len(),
        vd.strongly_buy,
        vd.buy,
        vd.watch,
        vd.wait,
        vd.avoid,
        vd.skip + vd.n_a,
    );
    out.push_str("<table><tr><th>评委</th><th>结论</th><th>分数</th></tr>");
    for vote in &panel.investors {
        let name = find_investor(&vote.id)
            .map(|m| m.name)
            .unwrap_or(vote.id.as_str());
        out.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{:.0}</td></tr>",
            escape_html(name),
            escape_html(&vote.vote),
            vote.score,
        ));
    }
    out.push_str("</table></section>");
    out
}

fn render_gaps_section(missing_dims: &[String], highlights: &[String]) -> String {
    use super::dim_viz::render_missing_chip;
    let mut chips: Vec<String> = highlights
        .iter()
        .map(|h| render_missing_chip(&escape_html(h)))
        .collect();
    for d in missing_dims {
        let esc = escape_html(d);
        if !highlights.iter().any(|h| h == d) {
            chips.push(render_missing_chip(&esc));
        }
    }
    format!(
        r#"<section class="card"><h2>数据缺口</h2><div class="chips">{}</div></section>"#,
        chips.join("")
    )
}

fn render_risks_section(risks: &[String]) -> String {
    let items: String = risks
        .iter()
        .map(|r| format!("<li>{}</li>", escape_html(r)))
        .collect();
    format!(r#"<section class="card"><h2>关键风险</h2><ul class="risk">{items}</ul></section>"#)
}

fn render_narrative_section(text: &str) -> String {
    format!(
        r#"<section class="card"><h2>分析结论</h2><div class="narrative">{}</div></section>"#,
        escape_html(text)
    )
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::research::analyze::analyze_stock;
    use crate::research::fetchers::bridge::apply_dims_to_snapshot;
    use crate::research::fetchers::types::{CollectOutput, DimQuality, DimResult, Market};
    use crate::research::profile::AnalysisProfile;
    use crate::research::types::FundamentalsSnapshot;
    use serde_json::{Value, json};

    fn moutai_result() -> AnalyzeStockResult {
        let symbol = "600519.SH";
        let dims = json!({
            "0_basic": { "data": { "name": "贵州茅台", "industry": "白酒", "price": 1680.0, "pe_ttm": 28.5, "pb": 8.2, "market_cap_yi": 21000, "shares_outstanding_yi": 12.56 } },
            "1_financials": { "data": { "roe": 32.0, "net_margin": 52.0, "revenue_latest_yi": 1500, "fcf_yi": 600, "financial_health": { "debt_ratio": 18.0 } } },
            "10_valuation": { "data": { "pe_ttm": 28.5, "pe_percentile": 35.0 } },
            "4_peers": { "data": { "peer_table": [{ "name": "五粮液", "pe": 18.0 }] } },
            "6_research": { "data": { "research_count": 10 } },
            "7_industry": { "data": { "industry": "白酒", "growth": 12.0, "industry_pe": 22.0 } },
            "6_fund_holders": { "data": { "holder_change_ratio": -8.0, "holder_count": 95000 } },
            "12_capital_flow": { "data": { "main_fund_5d_net_yi": 3.5 } }
        });
        let mut collect = CollectOutput {
            ticker: symbol.into(),
            market: Market::A,
            dims: Default::default(),
        };
        if let Some(obj) = dims.as_object() {
            for (key, wrapper) in obj {
                let data = wrapper.get("data").cloned().unwrap_or(Value::Null);
                collect.dims.insert(
                    key.clone(),
                    DimResult::ok(key, symbol, data, "fixture", DimQuality::Partial),
                );
            }
        }
        let raw_dims = collect.build_raw_dims();
        let mut snap = FundamentalsSnapshot {
            symbol: symbol.into(),
            ..Default::default()
        };
        apply_dims_to_snapshot(&mut snap, &collect);
        analyze_stock(
            &snap,
            Some(&raw_dims),
            None,
            &AnalysisProfile::medium(),
            Some(&collect),
        )
    }

    #[test]
    fn institutional_html_contains_synthesis_and_dims() {
        let html = render_institutional_html(&moutai_result(), None);
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("600519.SH"));
        assert!(html.contains("19 维评分"));
        assert!(html.contains("66 位评委"));
        assert!(html.contains("dim-bar"));
        assert!(html.len() < MAX_HTML_BYTES);
    }

    #[test]
    fn institutional_html_shows_warn_when_low_confidence() {
        let mut result = moutai_result();
        result.data_confidence.score = 0.40;
        result.synthesis.confidence_tier = "low".into();
        let html = render_institutional_html(&result, None);
        assert!(html.contains("数据置信度"));
    }

    #[test]
    fn escape_html_strips_tags() {
        assert!(escape_html("<script>").contains("&lt;"));
    }
}
