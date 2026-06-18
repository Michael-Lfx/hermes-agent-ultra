//! Dimension 7 · industry.

use async_trait::async_trait;
use serde_json::json;
use tracing::warn;

use super::super::r#trait::DimFetcher;
use super::super::types::{DimQuality, DimResult, FetcherSpec, Market};
use crate::providers::akshare::DEFAULT_INDUSTRY;
use crate::providers::akshare::{fetch_industry_growth, median_peer_pe};
use crate::research::fetchers::context::FetchContext;
use crate::research::fetchers::dim_keys;
use crate::settlement::is_a_share;

pub struct IndustryFetcher;

impl IndustryFetcher {
    pub const SPEC: FetcherSpec = FetcherSpec {
        dim_key: dim_keys::INDUSTRY,
        depends_on: &[dim_keys::BASIC, dim_keys::PEERS],
        markets: &[Market::A, Market::H, Market::U],
        sources: &["0_basic", "akshare"],
        web_only: false,
    };
}

#[async_trait]
impl DimFetcher for IndustryFetcher {
    fn spec(&self) -> &FetcherSpec {
        &Self::SPEC
    }

    async fn fetch(&self, ctx: &FetchContext) -> DimResult {
        let industry = ctx
            .prior_industry()
            .unwrap_or_else(|| DEFAULT_INDUSTRY.into());
        let industry_pe = ctx
            .prior_data("4_peers")
            .and_then(|d| d.get("peer_table"))
            .and_then(|v| v.as_array())
            .and_then(|a| median_peer_pe(a));
        let mut growth = None;
        let mut source = "0_basic".to_string();

        if is_a_share(&ctx.symbol) && industry != DEFAULT_INDUSTRY {
            match fetch_industry_growth(&ctx.symbol).await {
                Ok(g) => {
                    growth = g;
                    source = "akshare".into();
                }
                Err(e) => {
                    warn!(symbol = %ctx.symbol, error = %e, "industry growth fetch failed");
                }
            }
        }

        let quality = if industry_pe.is_some() && growth.is_some() {
            DimQuality::Full
        } else if industry != DEFAULT_INDUSTRY {
            DimQuality::Partial
        } else {
            DimQuality::Missing
        };

        DimResult::ok(
            dim_keys::INDUSTRY,
            &ctx.symbol,
            json!({
                "industry": industry,
                "industry_pe": industry_pe,
                "growth": growth,
            }),
            &source,
            quality,
        )
    }
}
