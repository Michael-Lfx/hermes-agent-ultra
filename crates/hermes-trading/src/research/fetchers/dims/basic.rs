//! Dimension 0 · basic quote / identity.

use async_trait::async_trait;
use serde_json::json;
use tracing::{debug, warn};

use super::super::r#trait::DimFetcher;
use super::super::types::{DimQuality, DimResult, FetcherSpec, Market};
use crate::providers::EastmoneyBasicProvider;
use crate::providers::FundamentalsProvider;
use crate::providers::QuoteRouter;
use crate::providers::QuoteSource;
use crate::providers::akshare::{
    apply_supplement, fetch_a_share_quote_chain, fetch_basic_info_supplement,
};
    fn snap_has_core(snap: &FundamentalsSnapshot) -> bool {
        snap.name.is_some() && snap.price.is_some()
    }

    fn dim_from_snap(snap: &FundamentalsSnapshot, source: &str) -> DimResult {
        let ticker = snap.symbol.clone();
        let data = json!({
            "name": snap.name,
            "price": snap.price,
            "pe_ttm": snap.pe,
            "pb": snap.pb,
            "market_cap_yi": snap.market_cap_yi,
            "shares_outstanding_yi": snap.shares_outstanding_yi,
            "change_pct": snap.change_pct,
            "industry": snap.industry,
            }
            Ok(snap) => {
                warn!(
                    symbol = %ticker,
                    "basic dim partial from eastmoney, trying quote router"
                );
                if let Ok(q) = self
                    .quotes
                    .fetch_quote_with_source(ticker, QuoteSource::Auto, false)
                    .await
                {
                    return Self::merge_snap_and_quote(ticker, snap, &q).await;
                }
            }
            Err(e) => {
                warn!(
                    symbol = %ticker,
                    error = %e,
                    "eastmoney basic failed, trying quote router"
                );
            }
        }

        match self
            .quotes
            .fetch_quote_with_source(ticker, QuoteSource::Auto, false)
            .await
        {
            Ok(q) => {
                let mut snap = FundamentalsSnapshot {
                    symbol: ticker.to_string(),
                    name: q.short_name.clone(),
                    price: q.price,
                    pe: q.pe_ratio,
                    change_pct: q.change_pct,
                    ..Default::default()
                };
                let mut source = q.source.clone();
                Self::supplement_snap_if_needed(ticker, &mut snap, &mut source).await;
                Self::dim_from_snap(&snap, &source)
            }
            Err(e) => DimResult::error(dim_keys::BASIC, ticker, "quote_router", e.to_string()),
        }
    }

    fn merge_snap_fields(snap: &mut FundamentalsSnapshot, q: &QuoteData) {
        if snap.name.is_none() {
            snap.name.clone_from(&q.short_name);
        }
        if snap.price.is_none() {
            snap.price = q.price;
        }
        if snap.pe.is_none() {
            snap.pe = q.pe_ratio;
        }
        if snap.change_pct.is_none() {
            snap.change_pct = q.change_pct;
        }
    }

    async fn merge_snap_and_quote(
        ticker: &str,
        mut snap: FundamentalsSnapshot,
        q: &QuoteData,
    ) -> DimResult {
        Self::merge_snap_fields(&mut snap, q);
        let mut source = if q.source == "akshare" {
            if snap.market_cap_yi.is_some() {
                "akshare+eastmoney_push2".into()
            } else {
                "akshare".into()
            }
        } else if q.source == "tencent_qt" {
            "eastmoney_push2+tencent_qt".into()
        } else {
            q.source.clone()
        };
        Self::supplement_snap_if_needed(ticker, &mut snap, &mut source).await;
        Self::dim_from_snap(&snap, &source)
    }
}

impl Default for BasicFetcher {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DimFetcher for BasicFetcher {
    fn spec(&self) -> &FetcherSpec {
        &Self::SPEC
    }

    async fn fetch(&self, ctx: &FetchContext) -> DimResult {
        let ticker = &ctx.symbol;
        if is_a_share(ticker) {
            return self.fetch_a_share(ctx).await;
        }

        match self
            .quotes
            .fetch_quote_with_source(ticker, QuoteSource::Auto, false)
            .await
        {
            Ok(q) => Self::dim_from_quote(ticker, &q),
            Err(e) => DimResult::error(dim_keys::BASIC, ticker, "quote_router", e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_quote(source: &str, name: Option<&str>, pe: Option<f64>) -> QuoteData {
        QuoteData {
            symbol: "002714.SZ".into(),
            market_date: None,
            as_of: None,
            price: Some(49.0),
            change: None,
            change_pct: Some(-0.5),
            volume: None,
            currency: Some("CNY".into()),
            exchange: None,
            short_name: name.map(str::to_string),
            pe_ratio: pe,
            high_52w: None,
            low_52w: None,
            source: source.into(),
            partial: false,
        }
    }

=======
    #[test]
    fn needs_push2_merge_when_name_or_pe_missing() {
        assert!(BasicFetcher::needs_push2_merge(&sample_quote(
            "akshare", None, None
        )));
        assert!(BasicFetcher::needs_push2_merge(&sample_quote(
            "akshare",
            Some("牧原股份"),
            None
        )));
        assert!(!BasicFetcher::needs_push2_merge(&sample_quote(
            "akshare",
            Some("牧原股份"),
            Some(12.0)
        )));
    }

>>>>>>> 98eae4748 (feat(trading): akshare-rs primary path for A-share research dims)
    #[test]
    fn needs_push2_merge_always_for_capital_fields() {
        assert!(BasicFetcher::needs_push2_merge(&sample_quote(
            "akshare",
            Some("牧原股份"),
            Some(12.0)
        )));
    }

    #[tokio::test]
    async fn merge_snap_and_quote_fills_gaps() {
        let snap = FundamentalsSnapshot {
            symbol: "600519.SH".into(),
            market_cap_yi: Some(1500.0),
            ..Default::default()
        };
        let q = QuoteData {
            symbol: "600519.SH".into(),
            market_date: None,
            as_of: None,
            price: Some(1407.0),
            change: None,
            change_pct: Some(0.1),
            volume: None,
            currency: None,
            exchange: None,
            short_name: Some("贵州茅台".into()),
            pe_ratio: Some(18.0),
            high_52w: None,
            low_52w: None,
            source: "akshare".into(),
            partial: false,
        };
        let dim = BasicFetcher::merge_snap_and_quote("600519.SH", snap, &q).await;
        assert!(dim.error.is_none());
<<<<<<< HEAD
        assert_eq!(dim.source, "akshare+eastmoney_push2+akshare_info");
        assert_eq!(dim.data.get("price").and_then(|v| v.as_f64()), Some(1407.0));
        assert_eq!(
            dim.data.get("name").and_then(|v| v.as_str()),
            Some("贵州茅台")
        );
    }
}
