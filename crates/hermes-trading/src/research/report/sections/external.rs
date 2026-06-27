//! Policy / macro / sentiment section.

use crate::research::report::content::{ExternalBlock, ExternalCoverage};
use crate::research::report::sections::util::{escape_html, render_bullet_list};

#[must_use]
pub fn render_external_section(block: &ExternalBlock) -> String {
    let mut out = String::from(r#"<section class="card"><h2>政策 / 宏观 / 舆情</h2>"#);
    match block.coverage {
        ExternalCoverage::NotRetrieved => {
            out.push_str(
                r#"<p class="muted-note">本次报告未单独检索政策、宏观与舆情；如需可追问「补充政策与行业影响」。</p>"#,
            );
        }
        ExternalCoverage::HttpPartial => {
            out.push_str(
                r#"<p class="muted-note">部分维度来自 HTTP 采集；政策/舆情建议结合 web 检索。</p>"#,
            );
        }
        ExternalCoverage::WebFilled => {}
    }
    if !block.macro_bullets.is_empty() {
        out.push_str("<h3>宏观环境</h3>");
        out.push_str(&render_bullet_list(&block.macro_bullets));
    }
    if !block.policy_bullets.is_empty() {
        out.push_str("<h3>政策影响</h3>");
        out.push_str(&render_bullet_list(&block.policy_bullets));
    }
    if !block.sentiment_bullets.is_empty() {
        out.push_str("<h3>舆情与情绪</h3>");
        out.push_str(&render_bullet_list(&block.sentiment_bullets));
    }
    render_industry_web_subsections(&mut out, block);
    if !block.sources.is_empty() {
        out.push_str("<h3>参考来源</h3><ul class=\"bullets\">");
        for src in &block.sources {
            out.push_str(&format!("<li>{}</li>", escape_html(src)));
        }
        out.push_str("</ul>");
    }
    out.push_str("</section>");
    out
}

fn render_industry_web_subsections(out: &mut String, block: &ExternalBlock) {
    if block.coverage != ExternalCoverage::WebFilled {
        return;
    }
    let sections = [
        ("产业链", &block.chain_bullets),
        ("原材料成本", &block.materials_bullets),
        ("期货关联", &block.futures_bullets),
        ("治理结构", &block.governance_bullets),
        ("护城河", &block.moat_bullets),
        ("实盘比赛", &block.contests_bullets),
    ];
    for (title, bullets) in sections {
        if bullets.is_empty() {
            continue;
        }
        out.push_str(&format!("<h3>{title}</h3>"));
        out.push_str(&render_bullet_list(bullets));
    }
}
