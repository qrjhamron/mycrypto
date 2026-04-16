//! Yahoo Finance macro quote fetcher.

use serde_json::Value;

use crate::error::{MycryptoError, Result};

/// Macro snapshot from Yahoo chart endpoints.
#[derive(Debug, Clone, Default)]
pub struct YahooMacroSnapshot {
    pub spy_change_pct: Option<f32>,
    pub dxy_change_pct: Option<f32>,
    pub vix: Option<f32>,
}

/// Fetch SPY, DXY and VIX references from Yahoo.
pub async fn fetch_yahoo_macro(client: &reqwest::Client) -> Result<YahooMacroSnapshot> {
    let spy = fetch_symbol_chart(client, "SPY").await?;
    let dxy = fetch_symbol_chart(client, "DX-Y.NYB").await?;
    let vix = fetch_symbol_chart(client, "%5EVIX").await?;

    Ok(YahooMacroSnapshot {
        spy_change_pct: spy.change_pct,
        dxy_change_pct: dxy.change_pct,
        vix: vix.last_price,
    })
}

#[derive(Debug, Clone, Default)]
struct SymbolChart {
    last_price: Option<f32>,
    change_pct: Option<f32>,
}

async fn fetch_symbol_chart(client: &reqwest::Client, symbol: &str) -> Result<SymbolChart> {
    let url = format!(
        "https://query1.finance.yahoo.com/v8/finance/chart/{}?interval=1d&range=5d",
        symbol
    );

    let response = client.get(url).send().await?;
    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        return Err(MycryptoError::ApiError {
            api: "yahoo/chart".to_string(),
            status,
            message: body,
        });
    }

    let payload: Value = response.json().await?;
    let result = payload
        .get("chart")
        .and_then(|v| v.get("result"))
        .and_then(|v| v.get(0))
        .ok_or_else(|| MycryptoError::MarketDataParse("yahoo result missing".to_string()))?;

    let closes = result
        .get("indicators")
        .and_then(|v| v.get("quote"))
        .and_then(|v| v.get(0))
        .and_then(|v| v.get("close"))
        .and_then(|v| v.as_array())
        .ok_or_else(|| MycryptoError::MarketDataParse("yahoo closes missing".to_string()))?;

    let prices: Vec<f32> = closes
        .iter()
        .filter_map(|v| v.as_f64().map(|x| x as f32))
        .collect();

    let last_price = prices.last().copied();
    let prev_price = prices
        .iter()
        .rev()
        .copied()
        .nth(1)
        .or_else(|| prices.first().copied());

    let change_pct = match (last_price, prev_price) {
        (Some(last), Some(prev)) if prev.abs() > f32::EPSILON => {
            Some(((last - prev) / prev) * 100.0)
        }
        _ => None,
    };

    Ok(SymbolChart {
        last_price,
        change_pct,
    })
}
