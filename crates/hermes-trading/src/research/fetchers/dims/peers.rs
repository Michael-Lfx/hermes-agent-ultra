//! Dimension 4 · peers.

use async_trait::async_trait;
use serde_json::json;
use tracing::warn;

use super::super::r#trait::DimFetcher;
use super::super::types::{DimQuality, DimResult, FetcherSpec, Market};
use crate::providers::akshare::fetch_peer_table;
use crate::research::fetchers::context::FetchContext;
use crate::research::fetchers::dim_keys;
use crate::settlement::is_a_share;

pub struct PeersFetcher;

impl PeersFetcher {
    pub const SPEC: FetcherSpec = FetcherSpec {
        dim_key: dim_keys::PEERS,
        depends_on: &[dim_keys::BASIC],
        markets: &[Market::A, Market::H, Market::U],
        sources: &["akshare", "eastmoney_data"],
        web_only: false,
    };
}

#[async_trait]
impl DimFetcher for PeersFetcher {
    fn spec(&self) -> &FetcherSpec {
        &Self::SPEC
    }

    async fn fetch(&self, ctx: &FetchContext) -> DimResult {
        let industry = ctx.prior_industry().unwrap_or_default();
        if !is_a_share(&ctx.symbol) {
            return DimResult::ok(
                dim_keys::PEERS,
                &ctx.symbol,
                json!({
                    "industry": industry,
                    "peer_table": [],
                    "_note": "港美股同业需 web_search",
                }),
                "web_search",
                DimQuality::Missing,
            );
        }

        match fetch_peer_table(&ctx.symbol).await {
            Ok(peer_table) if !peer_table.is_empty() => DimResult::ok(
                dim_keys::PEERS,
                &ctx.symbol,
                json!({
                    "industry": industry,
                    "peer_table": peer_table,
                }),
                "akshare",
                DimQuality::Partial,
            ),
            Ok(_) => DimResult::ok(
                dim_keys::PEERS,
                &ctx.symbol,
                json!({
                    "industry": industry,
                    "peer_table": [],
                }),
                "akshare",
                DimQuality::Missing,
            ),
            Err(e) => {
                warn!(symbol = %ctx.symbol, error = %e, "peers fetch failed");
                DimResult::ok(
                    dim_keys::PEERS,
                    &ctx.symbol,
                    json!({
                        "industry": industry,
                        "peer_table": [],
                        "_note": format!("peers fetch failed: {e}"),
                    }),
                    "akshare",
                    DimQuality::Missing,
                )
            }
        }
    }
}
