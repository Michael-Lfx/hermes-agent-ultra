//! Per-dimension fetcher implementations (mirrors UZI `fetch_*.py`).

pub mod basic;
pub mod capital_flow;
pub mod events;
pub mod valuation;
pub mod web_skipped;

pub use basic::BasicFetcher;
pub use capital_flow::CapitalFlowFetcher;
pub use events::EventsFetcher;
};
