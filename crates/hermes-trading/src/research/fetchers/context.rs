//! Shared fetch context passed between dimension fetchers.

use std::collections::BTreeMap;

use super::types::{DimResult, Market};
use crate::quote_data::QuoteData;
}

impl FetchContext {
    #[must_use]
    pub fn new(symbol: impl Into<String>) -> Self {
        let symbol = symbol.into();
        let market = Market::from_symbol(&symbol);
        Self {
            symbol,
            market,
            prior: BTreeMap::new(),
            cached_quote: None,
    pub fn prior_data(&self, dim_key: &str) -> Option<&serde_json::Value> {
        self.prior.get(dim_key).map(|r| &r.data)
    }

    #[must_use]
    pub fn prior_industry(&self) -> Option<String> {
        self.prior_data("0_basic")
            .and_then(|d| d.get("industry"))
            .and_then(|v| v.as_str())
            .map(str::to_string)
    }
<<<<<<< HEAD

    /// Prior `0_basic` dim payload and source tag.
    #[must_use]
    pub fn prior_basic(&self) -> Option<(&serde_json::Value, &str)> {
        self.prior
            .get("0_basic")
            .map(|r| (&r.data, r.source.as_str()))
    }
=======
>>>>>>> d5f5467b3 (feat(trading): UZI equity research engine and analyze_stock tool)
}
