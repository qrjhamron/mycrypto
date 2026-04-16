//! Dedicated news refresh worker helpers.

use chrono::Utc;
use futures_util::future::join_all;
use tracing::warn;

use crate::config::DataConfig;
use crate::state::NewsHeadline;

use super::{
    cryptopanic::fetch_cryptopanic_news, finnhub::fetch_finnhub_crypto_news,
    newsdata::fetch_newsdata_news, rss::fetch_rss_headlines,
};

/// Fetches all enabled news sources concurrently and returns normalized headlines.
pub async fn fetch_news_headlines(
    client: &reqwest::Client,
    config: &DataConfig,
) -> Vec<NewsHeadline> {
    let mut tasks = Vec::new();

    if config.reuters_rss_enabled || config.bloomberg_rss_enabled {
        let c = client.clone();
        let include_reuters = config.reuters_rss_enabled;
        let include_bloomberg = config.bloomberg_rss_enabled;
        tasks.push(tokio::spawn(async move {
            match fetch_rss_headlines(&c, include_reuters, include_bloomberg).await {
                Ok(snapshot) => snapshot.headlines,
                Err(err) => {
                    warn!("news refresh rss failed: {}", err);
                    Vec::new()
                }
            }
        }));
    }

    if config.finnhub_enabled {
        let c = client.clone();
        tasks.push(tokio::spawn(async move {
            match fetch_finnhub_crypto_news(&c).await {
                Ok(Some(items)) => items,
                Ok(None) => Vec::new(),
                Err(err) => {
                    warn!("news refresh finnhub failed: {}", err);
                    Vec::new()
                }
            }
        }));
    }

    if config.cryptopanic_enabled {
        let c = client.clone();
        tasks.push(tokio::spawn(async move {
            match fetch_cryptopanic_news(&c).await {
                Ok(Some(items)) => items,
                Ok(None) => Vec::new(),
                Err(err) => {
                    warn!("news refresh cryptopanic failed: {}", err);
                    Vec::new()
                }
            }
        }));
    }

    if config.newsdata_enabled {
        let c = client.clone();
        tasks.push(tokio::spawn(async move {
            match fetch_newsdata_news(&c).await {
                Ok(Some(items)) => items,
                Ok(None) => Vec::new(),
                Err(err) => {
                    warn!("news refresh newsdata failed: {}", err);
                    Vec::new()
                }
            }
        }));
    }

    let mut combined = Vec::new();
    for mut items in join_all(tasks).await.into_iter().flatten() {
        combined.append(&mut items);
    }

    normalize_news(combined, 1000)
}

/// Dedupe/sort helper for cache + UI.
pub fn normalize_news(mut headlines: Vec<NewsHeadline>, limit: usize) -> Vec<NewsHeadline> {
    let now = Utc::now();
    headlines.retain(|item| item.published_at <= now + chrono::Duration::minutes(5));

    let mut seen = std::collections::HashSet::new();
    headlines.retain(|item| seen.insert(canonical_news_key(item)));
    headlines.sort_by_key(|item| std::cmp::Reverse(item.published_at));
    headlines.truncate(limit);
    headlines
}

fn canonical_news_key(headline: &NewsHeadline) -> String {
    if let Some(url) = &headline.url {
        let normalized = url.trim().to_ascii_lowercase();
        if !normalized.is_empty() {
            return format!("url:{}", normalized);
        }
    }

    format!(
        "fallback:{}:{}:{}",
        headline.source.trim().to_ascii_lowercase(),
        headline.title.trim().to_ascii_lowercase(),
        headline.published_at.timestamp() / 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_news_dedup() {
        let now = Utc::now();
        let a = NewsHeadline {
            source: "A".to_string(),
            title: "First".to_string(),
            url: Some("https://example.com/x".to_string()),
            published_at: now - chrono::Duration::minutes(5),
            sentiment: None,
        };
        let a_dup = NewsHeadline {
            source: "A2".to_string(),
            title: "First duplicate".to_string(),
            url: Some("https://example.com/x".to_string()),
            published_at: now,
            sentiment: Some(0.1),
        };
        let b = NewsHeadline {
            source: "B".to_string(),
            title: "Second".to_string(),
            url: Some("https://example.com/y".to_string()),
            published_at: now - chrono::Duration::minutes(1),
            sentiment: None,
        };

        let normalized = normalize_news(vec![a, a_dup, b.clone()], 1000);
        assert_eq!(normalized.len(), 2);
        assert_eq!(normalized[0].title, b.title);
    }
}
