//! Per-dimension fetcher implementations (mirrors UZI `fetch_*.py`).

pub mod basic;
pub mod capital_flow;
pub mod events;
=======
pub mod financials;
pub mod fund_holders;
pub mod industry;
pub mod kline;
pub mod kline_util;
pub mod lhb;
pub mod peers;
pub mod research;
>>>>>>> 98eae4748 (feat(trading): akshare-rs primary path for A-share research dims)
pub mod valuation;
pub mod web_skipped;

pub use basic::BasicFetcher;
pub use capital_flow::CapitalFlowFetcher;
pub use events::EventsFetcher;
<<<<<<< HEAD
};
