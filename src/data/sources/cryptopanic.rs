//! CryptoPanic news source integration.

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::error::{MycryptoError, Result};
use crate::state::NewsHeadline;

#[derive(Debug, Deserialize)]
struct CryptoPanicResponse {
    #[serde(default)]
    results: Vec<CryptoPanicItem>,
}

#[derive(Debug, Deserialize)]
struct CryptoPanicItem {
    title: Option<String>,
    url: Option<String>,
    published_at: Option<String>,
    source: Option<CryptoPanicSource>,
}

#[derive(Debug, Deserialize)]
struct CryptoPanicSource {
    title: Option<String>,
}

/// Fetch latest CryptoPanic headlines.
///
/// Returns `Ok(None)` when `CRYPTOPANIC_API_KEY` is missing.
pub async fn fetch_cryptopanic_news(client: &reqwest::Client) -> Result<Option<Vec<NewsHeadline>>> {
    let api_key = match std::env::var("CRYPTOPANIC_API_KEY") {
        Ok(v) if !v.trim().is_empty() => v,
        _ => return Ok(None),
    };

    let response = client
        .get("https://cryptopanic.com/api/v1/posts/")
        .query(&[("auth_token", api_key.as_str()), ("public", "true")])
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        return Err(MycryptoError::ApiError {
            api: "cryptopanic/posts".to_string(),
            status,
            message: body,
        });
    }

    let payload: CryptoPanicResponse = response.json().await?;
    let mut headlines = Vec::new();

    for item in payload.results.into_iter().take(40) {
        let title = item.title.ok_or_else(|| {
            MycryptoError::MarketDataParse("cryptopanic title missing".to_string())
        })?;
        if title.trim().is_empty() {
            return Err(MycryptoError::MarketDataParse(
                "cryptopanic title empty".to_string(),
            ));
        }
        let published_raw = item.published_at.ok_or_else(|| {
            MycryptoError::MarketDataParse("cryptopanic published_at missing".to_string())
        })?;
        let published_at = DateTime::parse_from_rfc3339(&published_raw)
            .map(|d| d.with_timezone(&Utc))
            .map_err(|_| {
                MycryptoError::MarketDataParse("cryptopanic published_at invalid".to_string())
            })?;

        headlines.push(NewsHeadline {
            source: item
                .source
                .and_then(|s| s.title)
                .unwrap_or_else(|| "CryptoPanic".to_string()),
            title,
            url: item.url,
            published_at,
            sentiment: None,
        });
    }

    Ok(Some(headlines))
}
