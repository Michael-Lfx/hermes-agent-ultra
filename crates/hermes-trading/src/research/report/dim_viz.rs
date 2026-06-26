//! Inline SVG/CSS mini visualizations for institutional HTML.

/// Horizontal score bar (0–max), pure CSS + inline width.
#[must_use]
pub fn render_dim_bar(score: u8, max: u8) -> String {
    let pct = (f64::from(score) / f64::from(max) * 100.0).clamp(0.0, 100.0);
    let color = if score >= 7 {
        "#16a34a"
    } else if score <= 4 {
        "#dc2626"
    } else {
        "#ca8a04"
    };
    format!(
        r#"<span class="dim-bar" title="{score}/{max}"><span class="dim-fill" style="width:{pct:.0}%;background:{color}"></span></span>"#
    )
}

/// Badge for missing dimension keys or data fields.
#[must_use]
pub fn render_missing_chip(label: &str) -> String {
    format!(r#"<span class="chip chip-missing">{label}</span>"#)
}

/// Verdict badge class suffix for institutional cover.
#[must_use]
pub fn verdict_badge_class(verdict: &str) -> &'static str {
    match verdict {
        "strongly_buy" => "badge-strong-buy",
        "buy" => "badge-buy",
        "avoid" => "badge-avoid",
        "insufficient_data" => "badge-muted",
        _ => "badge-watch",
    }
}

#[must_use]
pub fn verdict_label_zh(verdict: &str) -> &'static str {
    match verdict {
        "strongly_buy" => "强烈偏多",
        "buy" => "偏多",
        "avoid" => "偏空",
        "insufficient_data" => "数据不足",
        _ => "观望",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dim_bar_width_reflects_score() {
        let s = render_dim_bar(8, 10);
        assert!(s.contains("width:80%"));
        assert!(s.contains("#16a34a"));
    }

    #[test]
    fn missing_chip_has_class() {
        assert!(render_missing_chip("fcf_latest_yi").contains("chip-missing"));
    }
}
