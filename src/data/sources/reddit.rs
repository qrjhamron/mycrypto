//! Reddit sentiment source.

use serde::Deserialize;

use crate::error::{MycryptoError, Result};

#[derive(Debug, Deserialize)]
struct RedditListing {
    data: RedditListingData,
}

#[derive(Debug, Deserialize)]
struct RedditListingData {
    children: Vec<RedditChild>,
}

#[derive(Debug, Deserialize)]
struct RedditChild {
    data: RedditPost,
}

#[derive(Debug, Deserialize)]
struct RedditPost {
    title: String,
    score: i64,
    upvote_ratio: f32,
}

/// Fetch and score Reddit sentiment from hot posts.
pub async fn fetch_reddit_sentiment(client: &reqwest::Client) -> Result<f32> {
    let mut scores = Vec::new();
    scores.push(
        fetch_subreddit_sentiment(client, "CryptoCurrency")
            .await
            .unwrap_or(0.0),
    );
    scores.push(
        fetch_subreddit_sentiment(client, "Bitcoin")
            .await
            .unwrap_or(0.0),
    );

    if scores.is_empty() {
        return Ok(0.0);
    }
    Ok(scores.iter().sum::<f32>() / scores.len() as f32)
}

async fn fetch_subreddit_sentiment(client: &reqwest::Client, name: &str) -> Result<f32> {
    let url = format!("https://www.reddit.com/r/{}/hot.json?limit=25", name);
    let response = client.get(url).send().await?;
    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        return Err(MycryptoError::ApiError {
            api: format!("reddit/{}", name),
            status,
            message: body,
        });
    }

    let payload: RedditListing = response.json().await?;
    let posts = payload.data.children;
    if posts.is_empty() {
        return Ok(0.0);
    }

    let mut total = 0.0f32;
    let mut count = 0usize;
    for child in posts {
        let title_score = keyword_sentiment(&child.data.title);
        let ratio_centered = (child.data.upvote_ratio - 0.5) * 2.0;
        let engagement = (child.data.score as f32).max(1.0).ln().max(1.0);
        let raw = (title_score * 0.4 + ratio_centered * 0.6) * engagement.min(4.0) / 4.0;
        total += raw.clamp(-1.0, 1.0);
        count += 1;
    }
    if count == 0 {
        return Err(MycryptoError::MarketDataParse(
            "reddit posts missing".to_string(),
        ));
    }
    Ok((total / count as f32).clamp(-1.0, 1.0))
}

fn keyword_sentiment(text: &str) -> f32 {
    let t = text.to_ascii_lowercase();
    let positive = [
        "bull", "rally", "breakout", "surge", "adoption", "approval", "green", "moon",
    ];
    let negative = [
        "bear",
        "dump",
        "crash",
        "hack",
        "liquidation",
        "ban",
        "lawsuit",
        "red",
    ];

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
