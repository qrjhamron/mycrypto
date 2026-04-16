//! X / Twitter sentiment source.

use serde::Deserialize;

use crate::error::{MycryptoError, Result};

#[derive(Debug, Deserialize)]
struct TweetsResponse {
    data: Option<Vec<TweetData>>,
}

#[derive(Debug, Deserialize)]
struct TweetData {
    text: String,
}

/// Returns Some(score) if token configured; None when token missing.
pub async fn fetch_twitter_sentiment(client: &reqwest::Client) -> Result<Option<f32>> {
    let token = match std::env::var("TWITTER_BEARER_TOKEN") {
        Ok(v) if !v.trim().is_empty() => v,
        _ => return Ok(None),
    };

    let url = "https://api.twitter.com/2/tweets/search/recent?query=bitcoin%20lang:en%20-is:retweet&max_results=25&tweet.fields=public_metrics";
    let response = client.get(url).bearer_auth(token).send().await?;
    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        return Err(MycryptoError::ApiError {
            api: "twitter/search/recent".to_string(),
            status,
            message: body,
        });
    }

    let payload: TweetsResponse = response.json().await?;
    let tweets = payload.data.unwrap_or_default();
    if tweets.is_empty() {
        return Ok(Some(0.0));
    }

    let mut total = 0.0f32;
    let mut count = 0usize;
    for t in tweets {
        total += keyword_sentiment(&t.text);
        count += 1;
    }
    if count == 0 {
        return Ok(Some(0.0));
    }

    Ok(Some((total / count as f32).clamp(-1.0, 1.0)))
}

fn keyword_sentiment(text: &str) -> f32 {
    let t = text.to_ascii_lowercase();
    let positive = ["bullish", "breakout", "uptrend", "buy", "strong", "growth"];
    let negative = ["bearish", "downtrend", "sell", "panic", "weak", "crash"];

    let mut score = 0.0f32;
    for p in positive {
        if t.contains(p) {
            score += 0.25;
        }
    }
    for n in negative {
        if t.contains(n) {
            score -= 0.25;
        }
    }
    score.clamp(-1.0, 1.0)
}
