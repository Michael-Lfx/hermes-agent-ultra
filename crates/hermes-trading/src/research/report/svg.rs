//! Minimal SVG gauge + valuation percentile bar.

/// Render a simple score gauge SVG.
#[must_use]
pub fn render_svg_gauge(score: f64, max: f64) -> String {
    let pct = (score / max * 100.0).clamp(0.0, 100.0);
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"140\" height=\"36\" viewBox=\"0 0 140 36\">\
         <text x=\"70\" y=\"10\" text-anchor=\"middle\" font-size=\"9\" fill=\"#555\">置信度</text>\
         <rect x=\"10\" y=\"16\" width=\"120\" height=\"8\" fill=\"#eee\" rx=\"2\"/>\
         <rect x=\"10\" y=\"16\" width=\"{pct:.1}\" height=\"8\" fill=\"#4a90d9\" rx=\"2\"/>\
         <text x=\"70\" y=\"34\" text-anchor=\"middle\" font-size=\"10\">{score:.0}/{max:.0}</text>\
         </svg>"
    )
}

/// Render PE 5y percentile bar (0–100).
#[must_use]
pub fn render_svg_percentile(pe_percentile: f64) -> String {
    let pct = pe_percentile.clamp(0.0, 100.0);
    let color = if pct < 40.0 {
        "#22c55e"
    } else if pct < 70.0 {
        "#eab308"
    } else {
        "#ef4444"
    };
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"140\" height=\"36\" viewBox=\"0 0 140 36\">\
         <text x=\"70\" y=\"10\" text-anchor=\"middle\" font-size=\"9\" fill=\"#555\">PE 5年分位</text>\
         <rect x=\"10\" y=\"16\" width=\"120\" height=\"8\" fill=\"#eee\" rx=\"2\"/>\
         <rect x=\"10\" y=\"16\" width=\"{bar:.1}\" height=\"8\" fill=\"{color}\" rx=\"2\"/>\
         <text x=\"70\" y=\"34\" text-anchor=\"middle\" font-size=\"10\">{pct:.0}%</text>\
         </svg>",
        bar = pct * 1.2,
        color = color,
        pct = pct
    )
}

}
