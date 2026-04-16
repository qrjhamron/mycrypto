//! NewsData.io source integration.

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::error::{MycryptoError, Result};
use crate::state::NewsHeadline;

#[derive(Debug, Deserialize)]
struct NewsDataResponse {
    #[serde(default)]
    results: Vec<NewsDataItem>,
}

#[derive(Debug, Deserialize)]
struct NewsDataItem {
    title: Option<String>,
    link: Option<String>,
    pub_date: Option<String>,
    source_id: Option<String>,
    description: Option<String>,
}

/// Fetch NewsData headlines.
///
/// Returns `Ok(None)` when `NEWSDATA_API_KEY` is missing.
pub async fn fetch_newsdata_news(client: &reqwest::Client) -> Result<Option<Vec<NewsHeadline>>> {
    let api_key = match std::env::var("NEWSDATA_API_KEY") {
        Ok(v) if !v.trim().is_empty() => v,
        _ => return Ok(None),
    };

    let response = client
        .get("https://newsdata.io/api/1/news")
        .query(&[
            ("apikey", api_key.as_str()),
            ("q", "crypto OR bitcoin OR ethereum"),
            ("language", "en"),
        ])
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        return Err(MycryptoError::ApiError {
            api: "newsdata/news".to_string(),
            status,
            message: body,
        });
    }

    let payload: NewsDataResponse = response.json().await?;
    let mut headlines = Vec::new();

    for item in payload.results.into_iter().take(40) {
        let title = item
            .title
            .ok_or_else(|| MycryptoError::MarketDataParse("newsdata title missing".to_string()))?;
        if title.trim().is_empty() {
            return Err(MycryptoError::MarketDataParse(
                "newsdata title empty".to_string(),
            ));
        }
        let date_raw = item.pub_date.ok_or_else(|| {
            MycryptoError::MarketDataParse("newsdata pubDate missing".to_string())
        })?;
        let published_at = DateTime::parse_from_rfc3339(&date_raw)
            .or_else(|_| DateTime::parse_from_str(&date_raw, "%Y-%m-%d %H:%M:%S %z"))
            .map(|d| d.with_timezone(&Utc))
            .map_err(|_| MycryptoError::MarketDataParse("newsdata date invalid".to_string()))?;

        let sentiment = item
            .description
            .as_deref()
            .map(keyword_sentiment)
            .or(Some(0.0));

        headlines.push(NewsHeadline {
            source: item.source_id.unwrap_or_else(|| "NewsData".to_string()),
            title,
            url: item.link,
            published_at,
            sentiment,
        });
    }

    Ok(Some(headlines))
}

fn keyword_sentiment(text: &str) -> f32 {
    let t = text.to_ascii_lowercase();
    let positive = ["rally", "surge", "gain", "adoption", "upgrade", "breakout"];
    let negative = ["crash", "drop", "hack", "selloff", "ban", "lawsuit"];

    let mut score = 0.0f32;
    for p in positive {
        if t.contains(p) {
            score += 0.2;
        }
    }
    for n in negative {
        if t.contains(n) {
            score -= 0.2;
        }
    }
    score.clamp(-1.0, 1.0)
}
