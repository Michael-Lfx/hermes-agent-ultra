//! Concrete market data provider implementations.

pub mod binance;

pub mod binance_quote;

pub mod akshare;

pub mod eastmoney;
#[cfg(any(test, feature = "test-mock"))]
pub mod mock;
pub mod router;

<<<<<<< HEAD
pub mod stub;

pub mod yahoo;
>>>>>>> 98eae4748 (feat(trading): akshare-rs primary path for A-share research dims)

