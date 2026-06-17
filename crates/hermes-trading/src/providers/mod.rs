//! Concrete market data provider implementations.

pub mod binance;

pub mod binance_quote;

pub mod akshare;

pub mod eastmoney;
pub mod eastmoney_quote;
#[cfg(any(test, feature = "test-mock"))]
pub mod mock;
#[cfg(any(test, feature = "test-mock"))]
pub mod quote_mock;
pub mod quote_router;
pub mod router;
pub mod stub;
pub mod yahoo;

<<<<<<< HEAD
pub mod stub;

pub mod yahoo;
>>>>>>> 98eae4748 (feat(trading): akshare-rs primary path for A-share research dims)

