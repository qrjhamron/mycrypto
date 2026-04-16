//! Reuters/Bloomberg RSS headline source.

use chrono::{DateTime, Duration, Utc};
use rss::Channel;

use crate::error::{MycryptoError, Result};
use crate::state::NewsHeadline;

#[derive(Debug, Clone)]
pub struct RssSnapshot {
    pub headlines: Vec<NewsHeadline>,
    pub sentiment_score: f32,
}

/// Fetches and merges Reuters/Bloomberg RSS headlines, then derives sentiment.
///
/// Only items within the last two hours are retained in the returned snapshot.
pub async fn fetch_rss_headlines(
    client: &reqwest::Client,
    include_reuters: bool,
    include_bloomberg: bool,
) -> Result<RssSnapshot> {
    let mut all = Vec::new();

    if include_reuters {
        let mut h = fetch_single_feed(
            client,
            "https://feeds.reuters.com/reuters/technologyNews",
            "Reuters",
        )
        .await?;
        all.append(&mut h);
    }

    if include_bloomberg {
        let mut h = fetch_single_feed(
            client,
            "https://feeds.bloomberg.com/markets/news.rss",
            "Bloomberg",
        )
        .await?;
        all.append(&mut h);
    }

    let now = Utc::now();
    all.retain(|h| now.signed_duration_since(h.published_at) <= Duration::hours(2));

    let mut acc = 0.0f32;
    let mut cnt = 0usize;
    for h in &all {
        acc += keyword_sentiment(&h.title);
        cnt += 1;
    }
    let sentiment_score = if cnt > 0 { acc / cnt as f32 } else { 0.0 };

    Ok(RssSnapshot {
        headlines: all,
        sentiment_score: sentiment_score.clamp(-1.0, 1.0),
    })
}

async fn fetch_single_feed(
    client: &reqwest::Client,
    url: &str,
    source: &str,
) -> Result<Vec<NewsHeadline>> {
    let response = client.get(url).send().await?;
    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        return Err(MycryptoError::ApiError {
            api: format!("rss/{}", source.to_ascii_lowercase()),
            status,
            message: body,
        });
    }

    let body = response.text().await?;
    let channel = Channel::read_from(body.as_bytes())
        .map_err(|e| MycryptoError::MarketDataParse(e.to_string()))?;
    let mut out = Vec::new();
    for item in channel.items() {
        let title = match item.title() {
            Some(t) if !t.trim().is_empty() => t.to_string(),
            _ => continue,
        };
        let published_at = item
            .pub_date()
            .and_then(parse_rss_date)
            .unwrap_or_else(Utc::now);
        out.push(NewsHeadline {
            source: source.to_string(),
            title,
            url: item.link().map(|u| u.to_string()),
            published_at,
            sentiment: Some(keyword_sentiment(item.title().unwrap_or_default())),
        });
    }
    Ok(out)
}

fn parse_rss_date(s: &str) -> Option<DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc2822(s)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

fn keyword_sentiment(text: &str) -> f32 {
    let t = text.to_ascii_lowercase();
    let positive = [
        "rally",
        "gain",
        "surge",
        "up",
        "approval",
        "growth",
        "record high",
        "strong",
    ];
    let negative = [
        "drop", "fall", "decline", "ban", "lawsuit", "risk", "hack", "weak",
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
