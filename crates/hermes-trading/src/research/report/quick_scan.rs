//! Quick-scan markdown report (`/quick-scan`, depth=lite).

use crate::research::models::dcf::DcfResult;
use crate::research::personas::PersonaVote;
use crate::research::personas::investors::find_investor;
use crate::research::profile::AnalysisProfile;
use crate::research::scoring::{DimScore, ScoreDimensionsResult};
use crate::research::types::DataConfidence;

/// Render lite quick-scan markdown (no 66-judge table, no full JSON).
#[must_use]
pub fn render_quick_scan_markdown(
    symbol: &str,
    scored: &ScoreDimensionsResult,
    votes: &[PersonaVote],
    confidence: &DataConfidence,
    dcf: &DcfResult,
    profile: &AnalysisProfile,
) -> String {
    debug_assert!(profile.is_lite());

    let overall = scored.fundamental_score;
    let trap = scored.dimensions.get("18_trap");
    let trap_label = trap.map(|d| d.label.as_str()).unwrap_or("—");
    let verdict = quick_verdict(overall, &dcf.verdict);

    let mut out = format!(
        "## {symbol} · 速判 (quick-scan)\n\n\
         **{verdict}** · 综合 {overall:.1}/100 · 数据置信度 {:.0}% · {trap_label}\n\n",
        confidence.score * 100.0
    );

    out.push_str(&format!(
        "- DCF: **{}** · 安全边际 {:+.1}%\n",
        dcf.verdict, dcf.safety_margin_pct
    ));

    out.push_str("\n### Top 10 评委\n\n");
    out.push_str("| 评委 | 结论 | 分数 | 引用规则 |\n| --- | --- | --- | --- |\n");
    for vote in votes {
        let name = find_investor(&vote.id)
            .map(|m| m.name)
            .unwrap_or(vote.id.as_str());
        let cited = vote.cited_rule.as_deref().unwrap_or("—");
        out.push_str(&format!(
            "| {name} | {} | {:.0} | {cited} |\n",
            vote.vote, vote.score
        ));
    }

    if let Some(risks) = extract_risk_lines(scored) {
        out.push_str("\n### 关键风险\n\n");
        for line in risks {
            out.push_str(&format!("- {line}\n"));
        }
    }

    out.push_str(
        "\n---\n\
         _LLM: 用上面数字写 ≤2 句 one-liner，禁止 web_search / HTML。_\n",
    );
    out
}

fn quick_verdict(score: f64, dcf_verdict: &str) -> &'static str {
    if score >= 70.0 {
        "偏多"
    } else if score < 45.0 {
        "偏空"
    } else if dcf_verdict.contains("低估") {
        "观望偏多"
    } else if dcf_verdict.contains("高估") {
        "观望偏空"
    } else {
        "观望"
    }
}

fn extract_risk_lines(scored: &ScoreDimensionsResult) -> Option<Vec<String>> {
    let mut risks = Vec::new();
    for dim in scored.dimensions.values() {
        for fail in &dim.reasons_fail {
            if risks.len() < 2 && !risks.contains(fail) {
                risks.push(fail.clone());
            }
        }
    }
    if risks.len() < 2
        && let Some(fin) = scored.dimensions.get("1_financials")
    {
        append_fin_risk(fin, &mut risks);
    }
    if risks.is_empty() {
        None
    } else {
        risks.truncate(2);
        Some(risks)
    }
}

fn append_fin_risk(fin: &DimScore, risks: &mut Vec<String>) {
    for fail in &fin.reasons_fail {
        if risks.len() < 2 && !risks.contains(fail) {
            risks.push(fail.clone());
        }
    }
    if risks.len() < 2 && fin.score <= 4 {
        let line = fin.label.clone();
        if !line.is_empty() && !risks.contains(&line) {
            risks.push(line);
        }
    }
}
