//! Multi-source data aggregator for macro, sentiment, and news.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use chrono::{DateTime, Utc};
use futures_util::future::join_all;
use tokio::sync::{mpsc, watch};
use tracing::warn;

use crate::config::DataConfig;
use crate::state::{
    EconomicEvent, MacroContext, NewsHeadline, SentimentScore, SourceStatus, SourceStatusLevel,
    StateUpdate,
};

use super::{
    binance::{fetch_binance_funding_rates, FundingRateSnapshot},
    coingecko::{fetch_coingecko_global, CoingeckoGlobalSnapshot},
    cryptopanic::fetch_cryptopanic_news,
    feargreed::{fetch_fear_greed, FearGreedSnapshot},
    finnhub::{fetch_finnhub_crypto_news, fetch_finnhub_economic_calendar},
    newsdata::fetch_newsdata_news,
    reddit::fetch_reddit_sentiment,
    rss::{fetch_rss_headlines, RssSnapshot},
    twitter::fetch_twitter_sentiment,
    yahoo::{fetch_yahoo_macro, YahooMacroSnapshot},
};

const SOURCE_TIMEOUT_SECS: u64 = 10;
const BREAKER_FAIL_THRESHOLD: u32 = 3;
const BREAKER_COOLDOWN_SECS: i64 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BreakerState {
    Healthy,
    Degraded,
    Dead,
}

impl BreakerState {
    fn as_str(self) -> &'static str {
        match self {
            BreakerState::Healthy => "Healthy",
            BreakerState::Degraded => "Degraded",
            BreakerState::Dead => "Dead",
        }
    }
}

#[derive(Debug, Clone)]
struct SourceRuntimeState {
    state: BreakerState,
    consecutive_failures: u32,
    last_success_at: Option<DateTime<Utc>>,
    cooldown_until: Option<DateTime<Utc>>,
    probe_mode: bool,
}

impl Default for SourceRuntimeState {
    fn default() -> Self {
        Self {
            state: BreakerState::Healthy,
            consecutive_failures: 0,
            last_success_at: None,
            cooldown_until: None,
            probe_mode: false,
        }
    }
}

impl SourceRuntimeState {
    fn can_poll(&mut self, now: DateTime<Utc>) -> bool {
        match self.cooldown_until {
            Some(until) if now < until => false,
            Some(_) => {
                self.cooldown_until = None;
                self.probe_mode = true;
                true
            }
            None => true,
        }
    }

    fn mark_success(&mut self, now: DateTime<Utc>) {
        self.state = BreakerState::Healthy;
        self.consecutive_failures = 0;
        self.last_success_at = Some(now);
        self.cooldown_until = None;
        self.probe_mode = false;
    }

    fn mark_failure(&mut self, now: DateTime<Utc>) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        if self.probe_mode {
            self.state = BreakerState::Dead;
            self.cooldown_until = Some(now + chrono::Duration::seconds(BREAKER_COOLDOWN_SECS));
            self.probe_mode = false;
            return;
        }

        if self.consecutive_failures >= BREAKER_FAIL_THRESHOLD {
            self.state = BreakerState::Degraded;
            self.cooldown_until = Some(now + chrono::Duration::seconds(BREAKER_COOLDOWN_SECS));
        }
    }

    fn to_status(&self, name: &str, detail: String) -> SourceStatus {
        let level = match self.state {
            BreakerState::Healthy => SourceStatusLevel::Ok,
            BreakerState::Degraded => SourceStatusLevel::Warn,
            BreakerState::Dead => SourceStatusLevel::Error,
        };

        SourceStatus {
            name: name.to_string(),
            level,
            detail,
            last_ok: self.last_success_at,
            consecutive_failures: self.consecutive_failures,
            runtime_status: self.state.as_str().to_string(),
        }
    }
}

#[derive(Debug, Clone, Default)]
struct SourceStateBook {
    by_name: HashMap<String, SourceRuntimeState>,
}

impl SourceStateBook {
    fn get_mut(&mut self, name: &str) -> &mut SourceRuntimeState {
        self.by_name.entry(name.to_string()).or_default()
    }
}

#[derive(Debug)]
enum FetchOutcome<T> {
    Success(T),
    MissingConfig,
    Failed(String),
}

#[derive(Debug)]
enum SourceTaskResult {
    Yahoo(FetchOutcome<YahooMacroSnapshot>),
    Coingecko(FetchOutcome<CoingeckoGlobalSnapshot>),
    FearGreed(FetchOutcome<FearGreedSnapshot>),
    Reddit(FetchOutcome<f32>),
    Twitter(FetchOutcome<f32>),
    Rss(FetchOutcome<RssSnapshot>),
    FinnhubNews(FetchOutcome<Vec<NewsHeadline>>),
    FinnhubCalendar(FetchOutcome<Vec<EconomicEvent>>),
    CryptoPanic(FetchOutcome<Vec<NewsHeadline>>),
    NewsData(FetchOutcome<Vec<NewsHeadline>>),
    BinanceFunding(FetchOutcome<Vec<FundingRateSnapshot>>),
}

/// Spawns the multi-source aggregator on the current Tokio runtime handle.
///
/// A local shutdown channel is created internally and not externally exposed.
pub fn spawn_sources_aggregator(
    config: DataConfig,
    state_tx: mpsc::Sender<StateUpdate>,
) -> tokio::task::JoinHandle<()> {
    let handle = tokio::runtime::Handle::current();
    let (_shutdown_tx, shutdown_rx) = watch::channel(false);
    spawn_sources_aggregator_on(&handle, config, state_tx, shutdown_rx)
}

/// Spawns the multi-source aggregator with explicit runtime and shutdown control.
///
/// This variant is useful for tests and host applications that manage runtime
/// ownership and graceful shutdown sequencing.
pub fn spawn_sources_aggregator_on(
    handle: &tokio::runtime::Handle,
    config: DataConfig,
    state_tx: mpsc::Sender<StateUpdate>,
    mut shutdown_rx: watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    handle.spawn(async move {
        let client = match reqwest::Client::builder()
            .user_agent("mycrypto/0.1")
            .timeout(Duration::from_secs(20))
            .build()
        {
            Ok(c) => c,
            Err(err) => {
                warn!("Failed to initialize source client: {}", err);
                return;
            }
        };

        let mut interval = tokio::time::interval(Duration::from_secs(
            config.sources_poll_interval_sec.max(15),
        ));
        let mut states = SourceStateBook::default();

        loop {
            tokio::select! {
                _ = interval.tick() => {}
                changed = shutdown_rx.changed() => {
                    if changed.is_err() || *shutdown_rx.borrow() {
                        break;
                    }
                    continue;
                }
            }
            let now = Utc::now();

            let mut sentiment = SentimentScore {
                updated_at: now,
                ..SentimentScore::default()
            };
            let mut macro_ctx = MacroContext {
                updated_at: Some(now),
                ..MacroContext::default()
            };
            let mut headlines: Vec<NewsHeadline> = Vec::new();

            let mut tasks: Vec<tokio::task::JoinHandle<SourceTaskResult>> = Vec::new();

            if config.yahoo_enabled {
                if should_poll_source(&state_tx, &mut states, "Yahoo Finance", now).await {
                    let c = client.clone();
                    tasks.push(tokio::spawn(async move {
                        SourceTaskResult::Yahoo(
                            with_timeout(async {
                                fetch_yahoo_macro(&c)
                                    .await
                                    .map(FetchOutcome::Success)
                                    .unwrap_or_else(|e| FetchOutcome::Failed(e.to_string()))
                            })
                            .await,
                        )
                    }));
                }
            } else {
                send_disabled_status(&state_tx, "Yahoo Finance").await;
            }

            if config.coingecko_enabled {
                if should_poll_source(&state_tx, &mut states, "CoinGecko", now).await {
                    let c = client.clone();
                    tasks.push(tokio::spawn(async move {
                        SourceTaskResult::Coingecko(
                            with_timeout(async {
                                fetch_coingecko_global(&c)
                                    .await
                                    .map(FetchOutcome::Success)
                                    .unwrap_or_else(|e| FetchOutcome::Failed(e.to_string()))
                            })
                            .await,
                        )
                    }));
                }
            } else {
                send_disabled_status(&state_tx, "CoinGecko").await;
            }

            if config.fear_greed_enabled {
                if should_poll_source(&state_tx, &mut states, "Fear & Greed", now).await {
                    let c = client.clone();
                    tasks.push(tokio::spawn(async move {
                        SourceTaskResult::FearGreed(
                            with_timeout(async {
                                fetch_fear_greed(&c)
                                    .await
                                    .map(FetchOutcome::Success)
                                    .unwrap_or_else(|e| FetchOutcome::Failed(e.to_string()))
                            })
                            .await,
                        )
                    }));
                }
            } else {
                send_disabled_status(&state_tx, "Fear & Greed").await;
            }

            if config.reddit_enabled {
                if should_poll_source(&state_tx, &mut states, "Reddit", now).await {
                    let c = client.clone();
                    tasks.push(tokio::spawn(async move {
                        SourceTaskResult::Reddit(
                            with_timeout(async {
                                fetch_reddit_sentiment(&c)
                                    .await
                                    .map(FetchOutcome::Success)
                                    .unwrap_or_else(|e| FetchOutcome::Failed(e.to_string()))
                            })
                            .await,
                        )
                    }));
                }
            } else {
                send_disabled_status(&state_tx, "Reddit").await;
            }

            if config.twitter_enabled {
                if should_poll_source(&state_tx, &mut states, "X/Twitter", now).await {
                    let c = client.clone();
                    tasks.push(tokio::spawn(async move {
                        SourceTaskResult::Twitter(
                            with_timeout(async {
                                match fetch_twitter_sentiment(&c).await {
                                    Ok(Some(v)) => FetchOutcome::Success(v),
                                    Ok(None) => FetchOutcome::MissingConfig,
                                    Err(e) => FetchOutcome::Failed(e.to_string()),
                                }
                            })
                            .await,
                        )
                    }));
                }
            } else {
                send_disabled_status(&state_tx, "X/Twitter").await;
            }

            if config.reuters_rss_enabled || config.bloomberg_rss_enabled {
                if should_poll_source(&state_tx, &mut states, "RSS", now).await {
                    let c = client.clone();
                    let include_reuters = config.reuters_rss_enabled;
                    let include_bloomberg = config.bloomberg_rss_enabled;
                    tasks.push(tokio::spawn(async move {
                        SourceTaskResult::Rss(
                            with_timeout(async {
                                fetch_rss_headlines(&c, include_reuters, include_bloomberg)
                                    .await
                                    .map(FetchOutcome::Success)
                                    .unwrap_or_else(|e| FetchOutcome::Failed(e.to_string()))
                            })
                            .await,
                        )
                    }));
                }
            } else {
                send_disabled_status(&state_tx, "Reuters RSS").await;
                send_disabled_status(&state_tx, "Bloomberg RSS").await;
            }

            if config.finnhub_enabled {
                if should_poll_source(&state_tx, &mut states, "Finnhub News", now).await {
                    let c = client.clone();
                    tasks.push(tokio::spawn(async move {
                        SourceTaskResult::FinnhubNews(
                            with_timeout(async {
                                match fetch_finnhub_crypto_news(&c).await {
                                    Ok(Some(v)) => FetchOutcome::Success(v),
                                    Ok(None) => FetchOutcome::MissingConfig,
                                    Err(e) => FetchOutcome::Failed(e.to_string()),
                                }
                            })
                            .await,
                        )
                    }));
                }
                if should_poll_source(&state_tx, &mut states, "Finnhub Calendar", now).await {
                    let c = client.clone();
                    tasks.push(tokio::spawn(async move {
                        SourceTaskResult::FinnhubCalendar(
                            with_timeout(async {
                                match fetch_finnhub_economic_calendar(&c).await {
                                    Ok(Some(v)) => FetchOutcome::Success(v),
                                    Ok(None) => FetchOutcome::MissingConfig,
                                    Err(e) => FetchOutcome::Failed(e.to_string()),
                                }
                            })
                            .await,
                        )
                    }));
                }
            } else {
                send_disabled_status(&state_tx, "Finnhub").await;
            }

            if config.cryptopanic_enabled {
                if should_poll_source(&state_tx, &mut states, "CryptoPanic", now).await {
                    let c = client.clone();
                    tasks.push(tokio::spawn(async move {
                        SourceTaskResult::CryptoPanic(
                            with_timeout(async {
                                match fetch_cryptopanic_news(&c).await {
                                    Ok(Some(v)) => FetchOutcome::Success(v),
                                    Ok(None) => FetchOutcome::MissingConfig,
                                    Err(e) => FetchOutcome::Failed(e.to_string()),
                                }
                            })
                            .await,
                        )
                    }));
                }
            } else {
                send_disabled_status(&state_tx, "CryptoPanic").await;
            }

            if config.newsdata_enabled {
                if should_poll_source(&state_tx, &mut states, "NewsData", now).await {
                    let c = client.clone();
                    tasks.push(tokio::spawn(async move {
                        SourceTaskResult::NewsData(
                            with_timeout(async {
                                match fetch_newsdata_news(&c).await {
                                    Ok(Some(v)) => FetchOutcome::Success(v),
                                    Ok(None) => FetchOutcome::MissingConfig,
                                    Err(e) => FetchOutcome::Failed(e.to_string()),
                                }
                            })
                            .await,
                        )
                    }));
                }
            } else {
                send_disabled_status(&state_tx, "NewsData").await;
            }

            if should_poll_source(&state_tx, &mut states, "Binance Funding", now).await {
                let c = client.clone();
                tasks.push(tokio::spawn(async move {
                    SourceTaskResult::BinanceFunding(
                        with_timeout(async {
                            fetch_binance_funding_rates(&c)
                                .await
                                .map(FetchOutcome::Success)
                                .unwrap_or_else(|e| FetchOutcome::Failed(e.to_string()))
                        })
                        .await,
                    )
                }));
            }

            let results = join_all(tasks).await;
            for res in results {
                let Ok(res) = res else {
                    continue;
                };
                match res {
                    SourceTaskResult::Yahoo(outcome) => {
                        let outcome = outcome.and_then(validate_yahoo);
                        apply_source_outcome(
                            &state_tx,
                            &mut states,
                            "Yahoo Finance",
                            now,
                            outcome,
                            |data| {
                                macro_ctx.spy_change_pct = data.spy_change_pct;
                                macro_ctx.dxy_change_pct = data.dxy_change_pct;
                                macro_ctx.vix = data.vix;
                            },
                        )
                        .await;
                    }
                    SourceTaskResult::Coingecko(outcome) => {
                        let outcome = outcome.and_then(validate_coingecko);
                        apply_source_outcome(
                            &state_tx,
                            &mut states,
                            "CoinGecko",
                            now,
                            outcome,
                            |data| {
                                macro_ctx.btc_dominance = Some(data.btc_dominance);
                                macro_ctx.total_market_cap = Some(data.total_market_cap);
                            },
                        )
                        .await;
                    }
                    SourceTaskResult::FearGreed(outcome) => {
                        apply_source_outcome(
                            &state_tx,
                            &mut states,
                            "Fear & Greed",
                            now,
                            outcome,
                            |data| {
                                sentiment.fear_greed = Some(data.value);
                                sentiment.fear_greed_label = Some(data.label);
                                sentiment.sources_available.push("fear_greed".to_string());
                            },
                        )
                        .await;
                    }
                    SourceTaskResult::Reddit(outcome) => {
                        let outcome = outcome.and_then(|v| {
                            sanitize_sentiment("Reddit", v)
                                .ok_or_else(|| "invalid sentiment".to_string())
                        });
                        apply_source_outcome(
                            &state_tx,
                            &mut states,
                            "Reddit",
                            now,
                            outcome,
                            |data| {
                                sentiment.reddit_score = Some(data);
                                sentiment.sources_available.push("reddit".to_string());
                            },
                        )
                        .await;
                    }
                    SourceTaskResult::Twitter(outcome) => {
                        let outcome = outcome.and_then(|v| {
                            sanitize_sentiment("X/Twitter", v)
                                .ok_or_else(|| "invalid sentiment".to_string())
                        });
                        apply_source_outcome(
                            &state_tx,
                            &mut states,
                            "X/Twitter",
                            now,
                            outcome,
                            |data| {
                                sentiment.twitter_score = Some(data);
                                sentiment.sources_available.push("twitter".to_string());
                            },
                        )
                        .await;
                    }
                    SourceTaskResult::Rss(outcome) => {
                        let outcome = outcome.and_then(|mut data| {
                            validate_news_items("RSS", &data.headlines)?;
                            data.sentiment_score = sanitize_sentiment("RSS", data.sentiment_score)
                                .ok_or_else(|| "invalid RSS sentiment".to_string())?;
                            Ok(data)
                        });
                        apply_source_outcome(&state_tx, &mut states, "RSS", now, outcome, |data| {
                            sentiment.news_score = Some(data.sentiment_score);
                            sentiment.sources_available.push("rss_news".to_string());
                            headlines.extend(data.headlines);
                        })
                        .await;
                    }
                    SourceTaskResult::FinnhubNews(outcome) => {
                        let outcome = outcome.and_then(|data| {
                            validate_news_items("Finnhub", &data)?;
                            Ok(data)
                        });
                        apply_source_outcome(
                            &state_tx,
                            &mut states,
                            "Finnhub News",
                            now,
                            outcome,
                            |data| {
                                let mut acc = 0.0f32;
                                let mut count = 0usize;
                                for item in &data {
                                    if let Some(s) = item.sentiment {
                                        if let Some(v) = sanitize_sentiment("Finnhub", s) {
                                            acc += v;
                                            count += 1;
                                        }
                                    }
                                }
                                if count > 0 {
                                    sentiment.news_score = Some(acc / count as f32);
                                }
                                headlines.extend(data);
                            },
                        )
                        .await;
                    }
                    SourceTaskResult::FinnhubCalendar(outcome) => {
                        apply_source_outcome(
                            &state_tx,
                            &mut states,
                            "Finnhub Calendar",
                            now,
                            outcome,
                            |data| {
                                macro_ctx.upcoming_events = data;
                            },
                        )
                        .await;
                    }
                    SourceTaskResult::CryptoPanic(outcome) => {
                        let outcome = outcome.and_then(|data| {
                            validate_news_items("CryptoPanic", &data)?;
                            Ok(data)
                        });
                        apply_source_outcome(
                            &state_tx,
                            &mut states,
                            "CryptoPanic",
                            now,
                            outcome,
                            |data| {
                                headlines.extend(data);
                            },
                        )
                        .await;
                    }
                    SourceTaskResult::NewsData(outcome) => {
                        let outcome = outcome.and_then(|data| {
                            validate_news_items("NewsData", &data)?;
                            Ok(data)
                        });
                        apply_source_outcome(
                            &state_tx,
                            &mut states,
                            "NewsData",
                            now,
                            outcome,
                            |data| {
                                headlines.extend(data);
                            },
                        )
                        .await;
                    }
                    SourceTaskResult::BinanceFunding(outcome) => {
                        let mut pending_funding: Vec<FundingRateSnapshot> = Vec::new();
                        apply_source_outcome(
                            &state_tx,
                            &mut states,
                            "Binance Funding",
                            now,
                            outcome,
                            |data| {
                                pending_funding = data;
                            },
                        )
                        .await;

                        for rate in pending_funding {
                            let _ = state_tx
                                .send(StateUpdate::FundingRateUpdate {
                                    pair: rate.pair,
                                    rate: rate.rate,
                                    next_time: rate.next_time,
                                })
                                .await;
                        }
                    }
                }
            }

            sentiment.composite = compute_composite_sentiment(&sentiment);

            let mut seen = HashSet::new();
            headlines.retain(|h| {
                let key = canonical_news_key(h);
                seen.insert(key)
            });
            headlines.sort_by_key(|h| std::cmp::Reverse(h.published_at));
            headlines.truncate(1000);

            let _ = state_tx.send(StateUpdate::MacroUpdate(macro_ctx)).await;
            let _ = state_tx.send(StateUpdate::SentimentUpdate(sentiment)).await;
            let _ = state_tx.send(StateUpdate::NewsUpdate(headlines)).await;
        }
    })
}

async fn with_timeout<T>(
    fut: impl std::future::Future<Output = FetchOutcome<T>>,
) -> FetchOutcome<T> {
    match tokio::time::timeout(Duration::from_secs(SOURCE_TIMEOUT_SECS), fut).await {
        Ok(v) => v,
        Err(_) => FetchOutcome::Failed(format!("timed out after {}s", SOURCE_TIMEOUT_SECS)),
    }
}

async fn should_poll_source(
    state_tx: &mpsc::Sender<StateUpdate>,
    states: &mut SourceStateBook,
    source: &str,
    now: DateTime<Utc>,
) -> bool {
    let runtime = states.get_mut(source);
    if runtime.can_poll(now) {
        return true;
    }

    let _ = state_tx
        .send(StateUpdate::SourceHealthChanged(runtime.to_status(
            source,
            format!(
                "state={} failures={} cooldown_until={}",
                runtime.state.as_str(),
                runtime.consecutive_failures,
                runtime
                    .cooldown_until
                    .map(|v| v.format("%H:%M:%S").to_string())
                    .unwrap_or_else(|| "n/a".to_string())
            ),
        )))
        .await;
    false
}

async fn apply_source_outcome<T>(
    state_tx: &mpsc::Sender<StateUpdate>,
    states: &mut SourceStateBook,
    source: &str,
    now: DateTime<Utc>,
    outcome: FetchOutcome<T>,
    on_success: impl FnOnce(T),
) {
    let runtime = states.get_mut(source);

    match outcome {
        FetchOutcome::Success(data) => {
            runtime.mark_success(now);
            on_success(data);
            let _ = state_tx
                .send(StateUpdate::SourceHealthChanged(runtime.to_status(
                    source,
                    format!(
                        "state={} failures={} validated=true",
                        runtime.state.as_str(),
                        runtime.consecutive_failures
                    ),
                )))
                .await;
        }
        FetchOutcome::MissingConfig => {
            let _ = state_tx
                .send(StateUpdate::SourceHealthChanged(SourceStatus {
                    name: source.to_string(),
                    level: SourceStatusLevel::MissingConfig,
                    detail: "missing API key".to_string(),
                    last_ok: runtime.last_success_at,
                    consecutive_failures: runtime.consecutive_failures,
                    runtime_status: runtime.state.as_str().to_string(),
                }))
                .await;
        }
        FetchOutcome::Failed(reason) => {
            runtime.mark_failure(now);
            warn!("{} rejected/failure: {}", source, reason);
            let _ = state_tx
                .send(StateUpdate::SourceHealthChanged(runtime.to_status(
                    source,
                    format!(
                        "state={} failures={} reason={}",
                        runtime.state.as_str(),
                        runtime.consecutive_failures,
                        reason
                    ),
                )))
                .await;
        }
    }
}

async fn send_disabled_status(state_tx: &mpsc::Sender<StateUpdate>, source: &str) {
    let _ = state_tx
        .send(StateUpdate::SourceHealthChanged(SourceStatus {
            name: source.to_string(),
            level: SourceStatusLevel::Disabled,
            detail: "disabled in config".to_string(),
            last_ok: None,
            consecutive_failures: 0,
            runtime_status: "Disabled".to_string(),
        }))
        .await;
}

trait OutcomeExt<T> {
    fn and_then<U>(self, f: impl FnOnce(T) -> std::result::Result<U, String>) -> FetchOutcome<U>;
}

impl<T> OutcomeExt<T> for FetchOutcome<T> {
    fn and_then<U>(self, f: impl FnOnce(T) -> std::result::Result<U, String>) -> FetchOutcome<U> {
        match self {
            FetchOutcome::Success(v) => match f(v) {
                Ok(out) => FetchOutcome::Success(out),
                Err(err) => FetchOutcome::Failed(err),
            },
            FetchOutcome::MissingConfig => FetchOutcome::MissingConfig,
            FetchOutcome::Failed(err) => FetchOutcome::Failed(err),
        }
    }
}

fn validate_yahoo(data: YahooMacroSnapshot) -> std::result::Result<YahooMacroSnapshot, String> {
    if data.spy_change_pct.is_none() && data.dxy_change_pct.is_none() && data.vix.is_none() {
        return Err("yahoo payload missing all macro fields".to_string());
    }
    Ok(data)
}

fn validate_coingecko(
    data: CoingeckoGlobalSnapshot,
) -> std::result::Result<CoingeckoGlobalSnapshot, String> {
    if !data.btc_dominance.is_finite() || !data.total_market_cap.is_finite() {
        return Err("coingecko returned non-finite numeric values".to_string());
    }
    if data.total_market_cap <= 0.0 {
        return Err("coingecko missing total market cap".to_string());
    }
    Ok(data)
}

fn validate_news_items(source: &str, items: &[NewsHeadline]) -> std::result::Result<(), String> {
    if items.is_empty() {
        return Err(format!("{} response has no news items", source));
    }
    for item in items {
        if item.title.trim().is_empty() {
            return Err(format!("{} news item has empty title", source));
        }
        if item.published_at > Utc::now() + chrono::Duration::minutes(5) {
            return Err(format!("{} news item has invalid future timestamp", source));
        }
    }
    Ok(())
}

fn sanitize_sentiment(source: &str, value: f32) -> Option<f32> {
    if !value.is_finite() {
        warn!("{} sentiment rejected: non-finite value", source);
        return None;
    }
    Some(value.clamp(-1.0, 1.0))
}

fn compute_composite_sentiment(sentiment: &SentimentScore) -> f32 {
    let mut weighted_sum = 0.0f32;
    let mut weight_total = 0.0f32;

    if let Some(fng) = sentiment.fear_greed {
        let mapped = (fng as f32 - 50.0) / 50.0;
        if let Some(v) = sanitize_sentiment("Fear & Greed", mapped) {
            weighted_sum += v * 0.30;
            weight_total += 0.30;
        }
    }
    if let Some(v) = sentiment
        .reddit_score
        .and_then(|v| sanitize_sentiment("Reddit", v))
    {
        weighted_sum += v * 0.20;
        weight_total += 0.20;
    }
    if let Some(v) = sentiment
        .twitter_score
        .and_then(|v| sanitize_sentiment("X/Twitter", v))
    {
        weighted_sum += v * 0.25;
        weight_total += 0.25;
    }
    if let Some(v) = sentiment
        .news_score
        .and_then(|v| sanitize_sentiment("News", v))
    {
        weighted_sum += v * 0.25;
        weight_total += 0.25;
    }

    if weight_total > 0.0 {
        (weighted_sum / weight_total).clamp(-1.0, 1.0)
    } else {
        0.0
    }
}

fn canonical_news_key(headline: &NewsHeadline) -> String {
    if let Some(url) = &headline.url {
        if !url.trim().is_empty() {
            return format!("url:{}", url.trim().to_ascii_lowercase());
        }
    }
    format!(
        "fallback:{}:{}:{}",
        headline.source.to_ascii_lowercase(),
        headline.title.to_ascii_lowercase(),
        headline.published_at.timestamp() / 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sentiment_clamp() {
        assert_eq!(sanitize_sentiment("x", 1.9), Some(1.0));
        assert_eq!(sanitize_sentiment("x", -1.7), Some(-1.0));
        assert_eq!(sanitize_sentiment("x", f32::NAN), None);
        assert_eq!(sanitize_sentiment("x", f32::INFINITY), None);
    }

    #[test]
    fn test_circuit_breaker_state_transitions() {
        let now = Utc::now();
        let mut state = SourceRuntimeState::default();

        state.mark_failure(now);
        state.mark_failure(now);
        assert_eq!(state.state, BreakerState::Healthy);

        state.mark_failure(now);
        assert_eq!(state.state, BreakerState::Degraded);
        assert!(state.cooldown_until.is_some());

        state.probe_mode = true;
        state.mark_failure(now + chrono::Duration::seconds(301));
        assert_eq!(state.state, BreakerState::Dead);
    }
}
