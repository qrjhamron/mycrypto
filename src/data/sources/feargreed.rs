//! Alternative.me Fear & Greed source.

use serde::Deserialize;

use crate::error::{MycryptoError, Result};

#[derive(Debug, Clone)]
pub struct FearGreedSnapshot {
    pub value: u8,
    pub label: String,
}

#[derive(Debug, Deserialize)]
struct FearGreedResponse {
    data: Vec<FearGreedPoint>,
}

#[derive(Debug, Deserialize)]
struct FearGreedPoint {
    value: String,
    value_classification: String,
}

/// Fetches the latest Alternative.me Fear & Greed snapshot.
///
/// Returns the newest index value and its human-readable classification label.
pub async fn fetch_fear_greed(client: &reqwest::Client) -> Result<FearGreedSnapshot> {
    let response = client.get("https://api.alternative.me/fng/").send().await?;
    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        return Err(MycryptoError::ApiError {
            api: "alternative/fng".to_string(),
            status,
            message: body,
        });
    }

    let payload: FearGreedResponse = response.json().await?;
    let first = payload
        .data
        .first()
        .ok_or_else(|| MycryptoError::MarketDataParse("fear greed payload empty".to_string()))?;
    let value = first
        .value
        .parse::<u8>()
        .map_err(|_| MycryptoError::MarketDataParse("fear greed value parse".to_string()))?;
    Ok(FearGreedSnapshot {
        value,
        label: first.value_classification.clone(),
    })
}
