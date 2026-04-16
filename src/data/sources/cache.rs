//! Disk cache helpers for news and chart persistence.

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};

use crate::error::Result;
use crate::state::{ChartCache, NewsCache, NewsHeadline, OHLCV};

const CHART_SERIES_CAP: usize = 200;
const CHART_CACHE_KEY_CAP: usize = 50;

fn home_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME")
        .map_err(|_| crate::error::MycryptoError::ConfigValidation("HOME not set".to_string()))?;
    Ok(PathBuf::from(home))
}

fn mycrypto_dir() -> Result<PathBuf> {
    let dir = home_dir()?.join(".mycrypto");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Returns `~/.mycrypto/news_cache.json` path.
pub fn news_cache_path() -> Result<PathBuf> {
    Ok(mycrypto_dir()?.join("news_cache.json"))
}

/// Returns `~/.mycrypto/chart_cache.json` path.
pub fn chart_cache_path() -> Result<PathBuf> {
    Ok(mycrypto_dir()?.join("chart_cache.json"))
}

/// Loads persisted news cache.
pub fn load_news_cache() -> Result<Option<NewsCache>> {
    let path = news_cache_path()?;
    if !path.exists() {
        return Ok(None);
    }

    let raw = std::fs::read_to_string(path)?;
    let parsed: NewsCache = serde_json::from_str(&raw)?;
    Ok(Some(parsed))
}

/// Saves persisted news cache with restrictive permissions.
pub fn save_news_cache(
    headlines: Vec<NewsHeadline>,
    last_fetch_at: Option<DateTime<Utc>>,
) -> Result<()> {
    let payload = NewsCache {
        headlines,
        last_fetch_at,
        cached_at: Utc::now(),
    };

    save_news_cache_payload(&payload)
}

/// Saves a fully constructed cache payload to disk.
pub fn save_news_cache_payload(payload: &NewsCache) -> Result<()> {
    let path = news_cache_path()?;

    let json = serde_json::to_string_pretty(payload)?;
    std::fs::write(&path, json)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&path)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(&path, perms)?;
    }

    Ok(())
}

/// Merge existing and incoming headlines, dedupe by URL/fallback, sort desc, limit `cap`.
pub fn merge_news_headlines(
    existing: Vec<NewsHeadline>,
    incoming: Vec<NewsHeadline>,
    cap: usize,
) -> Vec<NewsHeadline> {
    let mut merged = existing;
    merged.extend(incoming);
    normalize_news_for_cache(merged, cap)
}

/// Deterministic normalization for news cache collections.
pub fn normalize_news_for_cache(mut headlines: Vec<NewsHeadline>, cap: usize) -> Vec<NewsHeadline> {
    let mut seen = std::collections::HashSet::new();
    headlines.retain(|item| seen.insert(canonical_news_key(item)));
    headlines.sort_by_key(|item| std::cmp::Reverse(item.published_at));
    headlines.truncate(cap);
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

/// Loads persisted chart cache.
pub fn load_chart_cache() -> Result<Option<ChartCache>> {
    let path = chart_cache_path()?;
    if !path.exists() {
        return Ok(None);
    }

    let raw = std::fs::read_to_string(path)?;
    let parsed: ChartCache = serde_json::from_str(&raw)?;
    Ok(Some(parsed))
}

/// Saves chart cache map to disk.
pub fn save_chart_cache(
    series: HashMap<String, Vec<OHLCV>>,
    last_fetch_at: HashMap<String, DateTime<Utc>>,
    lru_order: Vec<String>,
) -> Result<()> {
    let path = chart_cache_path()?;

    let (bounded_series, bounded_last_fetch_at, bounded_lru_order) =
        normalize_chart_cache_payload(series, last_fetch_at, lru_order);
    let payload = ChartCache {
        series: bounded_series,
        last_fetch_at: bounded_last_fetch_at,
        lru_order: bounded_lru_order,
        cached_at: Utc::now(),
    };
    let json = serde_json::to_string_pretty(&payload)?;
    std::fs::write(&path, json)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&path)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(&path, perms)?;
    }

    Ok(())
}

fn normalize_chart_cache_payload(
    series: HashMap<String, Vec<OHLCV>>,
    last_fetch_at: HashMap<String, DateTime<Utc>>,
    lru_order: Vec<String>,
) -> NormalizedChartCachePayload {
    let mut seen = std::collections::HashSet::new();
    let mut ordered_lru: Vec<String> = lru_order
        .into_iter()
        .filter(|key| series.contains_key(key) && seen.insert(key.clone()))
        .collect();

    let mut missing_keys: Vec<String> = series
        .keys()
        .filter(|key| !seen.contains(*key))
        .cloned()
        .collect();
    missing_keys.sort();

    let mut full_order = missing_keys;
    full_order.append(&mut ordered_lru);

    let keep_start = full_order.len().saturating_sub(CHART_CACHE_KEY_CAP);
    let kept_order = full_order.split_off(keep_start);

    let mut bounded_series = HashMap::new();
    let mut bounded_last_fetch_at = HashMap::new();
    let mut bounded_lru_order = Vec::new();

    for key in kept_order {
        let Some(mut candles) = series.get(&key).cloned() else {
            continue;
        };

        candles.sort_by_key(|c| c.timestamp);
        if candles.len() > CHART_SERIES_CAP {
            let keep_from = candles.len() - CHART_SERIES_CAP;
            candles = candles.split_off(keep_from);
        }

        if let Some(ts) = last_fetch_at.get(&key).cloned() {
            bounded_last_fetch_at.insert(key.clone(), ts);
        }

        bounded_series.insert(key.clone(), candles);
        bounded_lru_order.push(key);
    }

    (bounded_series, bounded_last_fetch_at, bounded_lru_order)
}

type NormalizedChartCachePayload = (
    HashMap<String, Vec<OHLCV>>,
    HashMap<String, DateTime<Utc>>,
    Vec<String>,
);

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};

    use super::*;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn test_news_cache_roundtrip() {
        let _guard = env_lock().lock().unwrap();
        let home =
            std::env::temp_dir().join(format!("mycrypto-cache-test-news-{}", std::process::id()));
        std::fs::create_dir_all(&home).unwrap();
        std::env::set_var("HOME", &home);

        let headlines = vec![NewsHeadline {
            source: "Test".to_string(),
            title: "Headline".to_string(),
            url: Some("https://example.com".to_string()),
            published_at: Utc::now(),
            sentiment: Some(0.1),
        }];

        save_news_cache(headlines.clone(), Some(Utc::now())).unwrap();
        let loaded = load_news_cache().unwrap().unwrap();
        assert_eq!(loaded.headlines.len(), headlines.len());
    }

    #[test]
    fn test_chart_cache_roundtrip() {
        let _guard = env_lock().lock().unwrap();
        let home =
            std::env::temp_dir().join(format!("mycrypto-cache-test-chart-{}", std::process::id()));
        std::fs::create_dir_all(&home).unwrap();
        std::env::set_var("HOME", &home);

        let now = Utc::now();
        let mut map = HashMap::new();
        let mut last_fetch_at = HashMap::new();
        let mut lru_order = Vec::new();

        for i in 0..55 {
            let key = format!("X{i:02}USDT|1h");
            let candles: Vec<OHLCV> = (0i64..250)
                .map(|j| OHLCV {
                    timestamp: now - chrono::Duration::minutes((250 - j) as i64),
                    open: rust_decimal::Decimal::from(j),
                    high: rust_decimal::Decimal::from(j + 1),
                    low: rust_decimal::Decimal::from(j.saturating_sub(1)),
                    close: rust_decimal::Decimal::from(j),
                    volume: rust_decimal::Decimal::ONE,
                    trades: 1,
                    closed: true,
                })
                .collect();

            map.insert(key.clone(), candles);
            last_fetch_at.insert(key.clone(), now - chrono::Duration::seconds(i as i64));
            lru_order.push(key);
        }

        save_chart_cache(map.clone(), last_fetch_at.clone(), lru_order.clone()).unwrap();
        let loaded = load_chart_cache().unwrap().unwrap();

        assert_eq!(loaded.series.len(), 50);
        assert_eq!(loaded.last_fetch_at.len(), 50);
        assert_eq!(loaded.lru_order.len(), 50);
        assert_eq!(loaded.lru_order.first().unwrap(), "X05USDT|1h");
        assert_eq!(loaded.lru_order.last().unwrap(), "X54USDT|1h");

        for key in &loaded.lru_order {
            assert!(loaded.series.contains_key(key));
            assert!(loaded.last_fetch_at.contains_key(key));
            assert!(loaded.series.get(key).unwrap().len() <= 200);
        }
    }

    #[test]
    fn test_merge_news_headlines_dedup_and_order() {
        let _guard = env_lock().lock().unwrap();
        let now = Utc::now();
        let older = now - chrono::Duration::minutes(10);

        let old = NewsHeadline {
            source: "old".to_string(),
            title: "Old".to_string(),
            url: Some("https://example.com/a".to_string()),
            published_at: older,
            sentiment: None,
        };
        let fresh = NewsHeadline {
            source: "fresh".to_string(),
            title: "Fresh".to_string(),
            url: Some("https://example.com/b".to_string()),
            published_at: now,
            sentiment: None,
        };
        let dup = NewsHeadline {
            source: "dup".to_string(),
            title: "Dup".to_string(),
            url: Some("https://example.com/a".to_string()),
            published_at: now,
            sentiment: None,
        };

        let merged = merge_news_headlines(vec![old], vec![fresh.clone(), dup], 1000);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].title, fresh.title);
    }

    #[test]
    fn test_save_news_cache_payload_roundtrip() {
        let _guard = env_lock().lock().unwrap();
        let home = std::env::temp_dir().join(format!(
            "mycrypto-cache-test-news-payload-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&home).unwrap();
        std::env::set_var("HOME", &home);

        let payload = NewsCache {
            headlines: vec![NewsHeadline {
                source: "Test".to_string(),
                title: "Payload".to_string(),
                url: None,
                published_at: Utc::now(),
                sentiment: None,
            }],
            last_fetch_at: Some(Utc::now()),
            cached_at: Utc::now(),
        };

        save_news_cache_payload(&payload).unwrap();
        let loaded = load_news_cache().unwrap().unwrap();
        assert_eq!(loaded.headlines.len(), payload.headlines.len());
    }
}
