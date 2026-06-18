//! Dimension 6_fund_holders · 股东户数 / 基金持仓.

use async_trait::async_trait;
use tracing::warn;

use super::super::r#trait::DimFetcher;
use super::super::types::{DimQuality, DimResult, FetcherSpec, Market};
use crate::providers::akshare::fetch_fund_holders_dim;
use crate::research::fetchers::context::FetchContext;
use crate::research::fetchers::dim_keys;
use crate::settlement::is_a_share;

pub struct FundHoldersFetcher;

impl FundHoldersFetcher {
    pub const SPEC: FetcherSpec = FetcherSpec {
        dim_key: dim_keys::FUND_HOLDERS,
        depends_on: &[],
        markets: &[Market::A],
        sources: &["akshare", "eastmoney_datacenter"],
        web_only: false,
    };
}

#[async_trait]
impl DimFetcher for FundHoldersFetcher {
    fn spec(&self) -> &FetcherSpec {
        &Self::SPEC
    }

    async fn fetch(&self, ctx: &FetchContext) -> DimResult {
        if !is_a_share(&ctx.symbol) {
            return DimResult::skipped(dim_keys::FUND_HOLDERS, &ctx.symbol, "仅 A 股");
        }

        // ponytail: UZI 9-endpoint fund_hold_detail chain deferred; gdhs + Sina fund holder cover P0.
        match fetch_fund_holders_dim(&ctx.symbol).await {
            Ok(data) => {
                let has_holder = data
                    .get("holder_count")
                    .and_then(|v| v.as_f64())
                    .is_some_and(|v| v > 0.0);
                let has_funds = data
                    .get("fund_holdings")
                    .and_then(|v| v.as_array())
                    .is_some_and(|a| !a.is_empty());
                let quality = if has_holder && has_funds {
                    DimQuality::Full
                } else if has_holder {
                    DimQuality::Partial
                } else {
                    DimQuality::Missing
                };
                DimResult::ok(
                    dim_keys::FUND_HOLDERS,
                    &ctx.symbol,
                    data,
                    "akshare",
                    quality,
                )
            }
            Err(e) => {
                warn!(symbol = %ctx.symbol, error = %e, "fund_holders fetch failed");
                DimResult::ok(
                    dim_keys::FUND_HOLDERS,
                    &ctx.symbol,
                    serde_json::json!({ "_note": format!("fetch failed: {e}") }),
                    "akshare",
                    DimQuality::Missing,
                )
            }
        }
    }
}
