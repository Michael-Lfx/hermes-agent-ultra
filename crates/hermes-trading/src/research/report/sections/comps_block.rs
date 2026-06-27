//! 07 / COMPS · peer multiples block (UZI comps table parity).

use serde_json::Value;

use crate::research::models::comps::{CompsOk, CompsResult};
use crate::research::report::sections::util::escape_html;

const METRIC_LABELS: &[(&str, &str)] = &[
    ("pe", "PE"),
    ("pb", "PB"),
    ("ps", "PS"),
    ("ev_ebitda", "EV/EBITDA"),
    ("ev_sales", "EV/Sales"),
    ("roe", "ROE %"),
    ("net_margin", "净利率 %"),
    ("revenue_growth", "营收增速 %"),
];

#[must_use]
pub fn render_comps_section(comps: &Value) -> String {
    let Some(ok) = parse_comps_ok(comps) else {
        return String::new();
    };
    render_comps_ok(&ok)
}

fn parse_comps_ok(comps: &Value) -> Option<CompsOk> {
    if comps.get("skipped").is_some() || comps.get("error").is_some() {
        return None;
    }
    match serde_json::from_value::<CompsResult>(comps.clone()) {
        Ok(CompsResult::Ok(ok)) => Some(ok),
        _ => serde_json::from_value(comps.clone()).ok(),
    }
}

fn render_comps_ok(ok: &CompsOk) -> String {
    let verdict = escape_html(&ok.valuation_verdict);
    let n_peers = ok.peers.len();
    let pe_pct = ok.target_percentile.get("pe").copied().unwrap_or(f64::NAN);
    let implied_pe = ok.implied_price.get("via_median_pe").copied();
    let pe_median = ok.peer_stats.get("pe").map(|s| s.median);

    let kpi_implied = implied_pe
        .map(|v| format!("¥{v:.2}"))
        .unwrap_or_else(|| "—".into());
    let kpi_pe_pct = if pe_pct.is_nan() {
        "—".into()
    } else {
        format!("{pe_pct:.0}%")
    };
    let kpi_median = pe_median
        .map(|v| format!("{v:.1}x"))
        .unwrap_or_else(|| "—".into());

    let stats_table = render_peer_stats_table(&ok.peer_stats, &ok.target_percentile);
    let peers_table = render_peers_table(&ok.peers, &ok.target);
    let log_items = render_methodology_log(&ok.methodology_log);

    format!(
        r#"<section class="card" id="section-comps">
<div class="section-head">
<div class="section-tag">07 / COMPS</div>
<h2 class="section-title">可比公司估值</h2>
<div class="section-line"></div>
</div>
<div class="comps-block">
<div class="comps-head">
<div>
<span class="comps-badge">COMPS VALUATION</span>
<span class="comps-subtitle">{method}</span>
</div>
</div>
<div class="comps-summary">
<div class="comps-kpi"><div class="k">同行池</div><div class="v">{n_peers}</div><div class="hint">可比公司数量</div></div>
<div class="comps-kpi"><div class="k">PE 中位数</div><div class="v">{kpi_median}</div><div class="hint">同行倍数中枢</div></div>
<div class="comps-kpi"><div class="k">目标 PE 分位</div><div class="v">{kpi_pe_pct}</div><div class="hint">越低越便宜</div></div>
<div class="comps-kpi"><div class="k">隐含价 (PE×EPS)</div><div class="v">{kpi_implied}</div><div class="hint">{verdict}</div></div>
</div>
{stats_table}
{peers_table}
<details class="comps-methodology">
<summary>📐 推导步骤</summary>
<ol>{log_items}</ol>
</details>
</div>
</section>"#,
        method = escape_html(&ok.method),
        stats_table = stats_table,
        peers_table = peers_table,
        log_items = log_items,
    )
}

fn render_peer_stats_table(
    stats: &std::collections::BTreeMap<String, crate::research::models::comps::MetricStats>,
    percentiles: &std::collections::BTreeMap<String, f64>,
) -> String {
    if stats.is_empty() {
        return String::new();
    }
    let mut rows = String::new();
    for (key, label) in METRIC_LABELS {
        let Some(s) = stats.get(*key) else {
            continue;
        };
        let pct = percentiles
            .get(*key)
            .map(|p| format!("{p:.0}%"))
            .unwrap_or_else(|| "—".into());
        rows.push_str(&format!(
            "<tr><td>{label}</td><td>{min:.2}</td><td>{p25:.2}</td><td>{med:.2}</td><td>{p75:.2}</td><td>{max:.2}</td><td>{pct}</td></tr>",
            label = escape_html(label),
            min = s.min,
            p25 = s.p25,
            med = s.median,
            p75 = s.p75,
            max = s.max,
            pct = escape_html(&pct),
        ));
    }
    if rows.is_empty() {
        return String::new();
    }
    format!(
        r#"<div class="comps-sens-title">📊 同行倍数分布（n = 各指标有效样本）</div>
<table class="comps-stats"><tr><th>指标</th><th>Min</th><th>P25</th><th>Median</th><th>P75</th><th>Max</th><th>目标分位</th></tr>{rows}</table>"#
    )
}

fn render_peers_table(
    peers: &[crate::research::models::comps::CompsPeer],
    target: &crate::research::models::comps::CompsTarget,
) -> String {
    if peers.is_empty() {
        return String::new();
    }
    let target_name = target
        .name
        .as_deref()
        .or(target.ticker.as_deref())
        .unwrap_or("目标");
    let mut rows = String::new();
    rows.push_str(&format!(
        r#"<tr class="comps-target"><td><strong>{}</strong></td><td>—</td><td>{}</td><td>{}</td><td>{}</td></tr>"#,
        escape_html(target_name),
        fmt_opt(target.pe, 2),
        fmt_opt(target.pb, 2),
        fmt_opt(target.roe, 1),
    ));
    for p in peers.iter().take(12) {
        let name = p.name.as_deref().or(p.ticker.as_deref()).unwrap_or("—");
        rows.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            escape_html(name),
            escape_html(&p.ticker.clone().unwrap_or_else(|| "—".into())),
            fmt_opt(p.pe, 2),
            fmt_opt(p.pb, 2),
            fmt_opt(p.roe, 1),
        ));
    }
    format!(
        r#"<div class="comps-sens-title">🏢 同行明细</div>
<table class="comps-peers"><tr><th>公司</th><th>代码</th><th>PE</th><th>PB</th><th>ROE</th></tr>{rows}</table>"#
    )
}

fn render_methodology_log(log: &[String]) -> String {
    log.iter()
        .take(7)
        .map(|line| format!("<li>{}</li>", escape_html(line)))
        .collect()
}

fn fmt_opt(v: Option<f64>, decimals: usize) -> String {
    v.map(|x| format!("{x:.decimals$}"))
        .unwrap_or_else(|| "—".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::research::models::comps::{CompsPeer, CompsTarget, build_comps_table};

    fn sample_comps_json() -> Value {
        let target = CompsTarget {
            name: Some("贵州茅台".into()),
            ticker: Some("600519.SH".into()),
            price: Some(1680.0),
            pe: Some(28.5),
            pb: Some(8.2),
            eps: Some(58.0),
            bvps: Some(200.0),
            roe: Some(32.0),
            ..Default::default()
        };
        let peers = vec![
            CompsPeer {
                name: Some("五粮液".into()),
                ticker: Some("000858".into()),
                pe: Some(18.0),
                pb: Some(4.2),
                roe: Some(22.0),
                ..Default::default()
            },
            CompsPeer {
                name: Some("泸州老窖".into()),
                ticker: Some("000568".into()),
                pe: Some(22.0),
                pb: Some(5.1),
                roe: Some(28.0),
                ..Default::default()
            },
        ];
        let CompsResult::Ok(ok) = build_comps_table(target, &peers) else {
            panic!("comps");
        };
        serde_json::to_value(ok).expect("json")
    }

    #[test]
    fn comps_section_renders_kpi_and_tables() {
        let html = render_comps_section(&sample_comps_json());
        assert!(html.contains("07 / COMPS"));
        assert!(html.contains("COMPS VALUATION"));
        assert!(html.contains("可比公司估值"));
        assert!(html.contains("五粮液"));
        assert!(html.contains("推导步骤"));
        assert!(html.contains("comps-stats"));
    }

    #[test]
    fn comps_section_empty_when_skipped() {
        assert!(render_comps_section(&serde_json::json!({"skipped": "lite"})).is_empty());
        assert!(render_comps_section(&serde_json::json!({"error": "no peers"})).is_empty());
    }
}
