//! Minimal HTML report (P0.5) + SVG stub (P2).

pub mod html;
pub mod labels;
pub mod markdown;
pub mod svg;

pub use html::render_html_report;
<<<<<<< HEAD
pub use markdown::render_summary_markdown;
=======
>>>>>>> 2071fbf41 (feat(trading): 4-wave equity research end-state)
pub use svg::{render_svg_gauge, render_svg_percentile};
