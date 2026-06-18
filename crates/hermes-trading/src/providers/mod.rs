//! Concrete market data provider implementations.



pub mod binance;

pub mod binance_quote;

pub mod akshare;


pub use binance::BinanceProvider;

pub use binance_quote::BinanceQuoteProvider;

pub use eastmoney::EastmoneyProvider;

pub use eastmoney_basic::EastmoneyBasicProvider;

pub use eastmoney_capital_flow::EastmoneyCapitalFlowProvider;

pub use eastmoney_financials::EastmoneyFinancialsProvider;

pub use eastmoney_lhb::EastmoneyLhbProvider;

pub use eastmoney_quote::EastmoneyQuoteProvider;

pub use eastmoney_valuation::EastmoneyValuationProvider;

pub use fundamentals::{FundamentalsAggregator, FundamentalsProvider};

#[cfg(any(test, feature = "test-mock"))]

pub use mock::MockProvider;

#[cfg(any(test, feature = "test-mock"))]

pub use quote_mock::MockQuoteProvider;

pub use quote_router::{QuoteRouter, QuoteSource};

pub use router::{AutoRouter, DataSource};

pub use stub::StubProvider;
<<<<<<< HEAD

pub use yahoo::YahooProvider;

