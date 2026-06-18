//! Dimension 12 · capital flow.

use async_trait::async_trait;

use super::super::r#trait::DimFetcher;
use super::super::types::{DimQuality, DimResult, FetcherSpec, Market};
use crate::providers::akshare::fetch_capital_flow_dim_akshare;
use crate::research::fetchers::context::FetchContext;
use crate::research::fetchers::dim_keys;
use crate::settlement::is_a_share;

pub struct CapitalFlowFetcher;

impl CapitalFlowFetcher {
    pub const SPEC: FetcherSpec = FetcherSpec {
        dim_key: dim_keys::CAPITAL_FLOW,
        depends_on: &[],
        markets: &[Market::A, Market::H],
        sources: &["akshare", "eastmoney_fflow"],
        web_only: false,
    };
}

=======
impl Default for CapitalFlowFetcher {
    fn default() -> Self {
        Self::new()
    }
}

>>>>>>> d5f5467b3 (feat(trading): UZI equity research engine and analyze_stock tool)
#[async_trait]
impl DimFetcher for CapitalFlowFetcher {
    fn spec(&self) -> &FetcherSpec {
        &Self::SPEC
    }

    async fn fetch(&self, ctx: &FetchContext) -> DimResult {
        if !is_a_share(&ctx.symbol) {
            return DimResult::skipped(
                dim_keys::CAPITAL_FLOW,
                &ctx.symbol,
                "港美股资金流用 web_search",
            );
        }
<<<<<<< HEAD
        match fetch_capital_flow_dim_akshare(&ctx.symbol).await {
            Ok((data, source)) => {
                let quality = if data
                    .get("main_fund_5d_net_yi")
                    .and_then(|v| v.as_f64())
                    .is_some()
                {
                    DimQuality::Partial
                } else {
                    DimQuality::Missing
                };
                DimResult::ok(dim_keys::CAPITAL_FLOW, &ctx.symbol, data, source, quality)
            }
            Err(e) => DimResult::error(
                dim_keys::CAPITAL_FLOW,
                &ctx.symbol,
                "akshare",
                e.to_string(),
            ),
        }
    }
}
<<<<<<< HEAD
=======

async fn fetch_capital_flow_util(
    client: &reqwest::Client,
    symbol: &str,
) -> Result<serde_json::Value, crate::error::TradingError> {
    fetch_capital_flow_dim(client, symbol).await
}
>>>>>>> d5f5467b3 (feat(trading): UZI equity research engine and analyze_stock tool)
