//! Shareholder count + fund holdings (akshare, ponytail: not full UZI 9-endpoint chain).

use std::collections::HashMap;

use serde_json::{Value, json};

use crate::error::TradingError;

use super::{client, code6, labels, map_err};

pub async fn fetch_fund_holders_dim(symbol: &str) -> Result<Value, TradingError> {
    let code = code6(symbol)?;
    let gdhs = client()
        .stock_zh_a_gdhs_detail_em(&code)
        .await
        .map_err(map_err)?;
    let fund_rows = client()
        .stock_fund_stock_holder(&code)
        .await
        .map_err(map_err)?;

    let holder = gdhs
        .iter()
        .max_by(|a, b| a.end_date.cmp(&b.end_date))
        .map(|d| (d.holder_count, d.holder_change_ratio));
    let (holder_count, holder_change_ratio) = holder.unwrap_or((0.0, 0.0));

    Ok(map_fund_holders_json(
        holder_count,
        holder_change_ratio,
        &fund_rows,
    ))
}

pub fn map_fund_holdings(rows: &[HashMap<String, Value>]) -> Vec<Value> {
    rows.iter()
        .take(10)
        .filter_map(|row| {
            let fund_name = row
                .get(labels::fund_holders::FUND_NAME)
                .or_else(|| row.get(labels::fund_holders::HOLDER_NAME))
                .and_then(|v| v.as_str())
                .map(str::to_string)?;
            let hold_ratio = row
                .get(labels::fund_holders::FLOAT_RATIO)
                .or_else(|| row.get(labels::fund_holders::HOLD_RATIO))
                .and_then(|v| {
                    v.as_f64()
                        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                });
            Some(json!({
                "fund_name": fund_name,
                "hold_ratio": hold_ratio,
            }))
        })
        .collect()
}

pub fn map_fund_holders_json(
    holder_count: f64,
    holder_change_ratio: f64,
    fund_rows: &[HashMap<String, Value>],
) -> Value {
    let fund_holdings = map_fund_holdings(fund_rows);
    json!({
        "holder_count": if holder_count > 0.0 { Some(holder_count) } else { None },
        "holder_change_ratio": if holder_count > 0.0 { Some(holder_change_ratio) } else { None },
        "fund_holdings": fund_holdings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_fund_holders_json_fixture() {
        let out = map_fund_holders_json(95_000.0, -5.0, &[]);
        assert_eq!(
            out.get("holder_count").and_then(|v| v.as_f64()),
            Some(95_000.0)
        );
        assert_eq!(
            out.get("holder_change_ratio").and_then(|v| v.as_f64()),
            Some(-5.0)
        );
    }

    #[test]
    fn map_fund_holdings_fixture() {
        let mut row = HashMap::new();
        row.insert(labels::fund_holders::FUND_NAME.into(), json!("易方达蓝筹"));
        row.insert(labels::fund_holders::FLOAT_RATIO.into(), json!(1.2));
        let out = map_fund_holdings(&[row]);
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].get("fund_name").and_then(|v| v.as_str()),
            Some("易方达蓝筹")
        );
    }
}
