//! CoinGecko source integration.

use serde::Deserialize;

use crate::error::{MycryptoError, Result};

#[derive(Debug, Clone, Default)]
pub struct CoingeckoGlobalSnapshot {
    pub btc_dominance: f32,
    pub total_market_cap: f64,
}

#[derive(Debug, Deserialize)]
struct GlobalResponse {
    data: GlobalData,
}

#[derive(Debug, Deserialize)]
struct GlobalData {
    market_cap_percentage: MarketCapPercentage,
    total_market_cap: std::collections::HashMap<String, f64>,
}

#[derive(Debug, Deserialize)]
struct MarketCapPercentage {
    btc: f32,
}

/// Fetch global metrics (dominance + total cap).
pub async fn fetch_coingecko_global(client: &reqwest::Client) -> Result<CoingeckoGlobalSnapshot> {
    let response = client
        .get("https://api.coingecko.com/api/v3/global")
        .send()
        .await?;
    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        return Err(MycryptoError::ApiError {
            api: "coingecko/global".to_string(),
            status,
            message: body,
        });
    }

    let payload: GlobalResponse = response.json().await?;
    let total_market_cap = payload
        .data
        .total_market_cap
        .get("usd")
        .copied()
        .unwrap_or(0.0);
    Ok(CoingeckoGlobalSnapshot {
        btc_dominance: payload.data.market_cap_percentage.btc,
        total_market_cap,
    })
}
