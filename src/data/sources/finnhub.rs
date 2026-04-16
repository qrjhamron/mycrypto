//! Finnhub news and economic calendar.

use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;

use crate::error::{MycryptoError, Result};
use crate::state::{EconomicEvent, NewsHeadline};

#[derive(Debug, Deserialize)]
struct FinnhubNewsItem {
    datetime: i64,
    headline: String,
    source: String,
    url: String,
    summary: String,
}

#[derive(Debug, Deserialize)]
struct FinnhubCalendarResponse {
    #[serde(default)]
    economic_calendar: Vec<FinnhubEvent>,
}

#[derive(Debug, Deserialize)]
struct FinnhubEvent {
    event: Option<String>,
    country: Option<String>,
    date: Option<String>,
    impact: Option<String>,
}

/// Returns None when FINNHUB_API_KEY is not configured.
pub async fn fetch_finnhub_crypto_news(
    client: &reqwest::Client,
) -> Result<Option<Vec<NewsHeadline>>> {
    let api_key = match std::env::var("FINNHUB_API_KEY") {
        Ok(v) if !v.trim().is_empty() => v,
        _ => return Ok(None),
    };

    let response = client
        .get("https://finnhub.io/api/v1/news?category=crypto")
        .query(&[("token", api_key.as_str())])
        .send()
        .await?;
    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        return Err(MycryptoError::ApiError {
            api: "finnhub/news".to_string(),
            status,
            message: body,
        });
    }

    let payload: Vec<FinnhubNewsItem> = response.json().await?;
    let mut headlines = Vec::new();
    for item in payload.into_iter().take(30) {
        headlines.push(NewsHeadline {
            source: if item.source.trim().is_empty() {
                "Finnhub".to_string()
            } else {
                item.source
            },
            title: item.headline,
            url: Some(item.url),
            published_at: unix_to_utc(item.datetime),
            sentiment: Some(keyword_sentiment(&item.summary)),
        });
    }
    Ok(Some(headlines))
}

/// Returns None when FINNHUB_API_KEY is not configured.
pub async fn fetch_finnhub_economic_calendar(
    client: &reqwest::Client,
) -> Result<Option<Vec<EconomicEvent>>> {
    let api_key = match std::env::var("FINNHUB_API_KEY") {
        Ok(v) if !v.trim().is_empty() => v,
        _ => return Ok(None),
    };

    let response = client
        .get("https://finnhub.io/api/v1/calendar/economic")
        .query(&[("token", api_key.as_str())])
        .send()
        .await?;
    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        return Err(MycryptoError::ApiError {
            api: "finnhub/calendar".to_string(),
            status,
            message: body,
        });
    }

    let payload: FinnhubCalendarResponse = response.json().await?;
    let mut events = Vec::new();
    for e in payload.economic_calendar.into_iter().take(20) {
        let event_name = e.event.unwrap_or_else(|| "Macro Event".to_string());
        let country = e.country.unwrap_or_else(|| "US".to_string());
        let impact_raw = e.impact.unwrap_or_else(|| "medium".to_string());
        let impact = match impact_raw.to_ascii_lowercase().as_str() {
            "high" => "high",
            "low" => "low",
            _ => "medium",
        }
        .to_string();

        let time = e
            .date
            .as_deref()
            .and_then(parse_date_ymd)
            .unwrap_or_else(Utc::now);

        events.push(EconomicEvent {
            title: event_name,
            time,
            impact,
            country,
        });
    }
    Ok(Some(events))
}

fn unix_to_utc(unix: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(unix, 0).single().unwrap_or_else(Utc::now)
}

fn parse_date_ymd(s: &str) -> Option<DateTime<Utc>> {
    let date = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()?;
    let dt = date.and_hms_opt(0, 0, 0)?;
    Some(DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
}

fn keyword_sentiment(text: &str) -> f32 {
    let t = text.to_ascii_lowercase();
    let positive = ["beat", "growth", "surge", "improve", "strength"];
    let negative = ["miss", "decline", "fall", "risk", "slowdown"];

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
