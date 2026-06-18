//! Dimension 7 · industry.

use async_trait::async_trait;
use serde_json::json;
use tracing::warn;

use super::super::r#trait::DimFetcher;
use super::super::types::{DimQuality, DimResult, FetcherSpec, Market};
use crate::providers::akshare::DEFAULT_INDUSTRY;
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
