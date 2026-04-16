//! Binance WebSocket feed for real-time market data.

use std::collections::{HashMap, HashSet, VecDeque};
use std::str::FromStr;
use std::time::Duration;

use chrono::{DateTime, TimeZone, Utc};
use futures_util::{SinkExt, StreamExt};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, watch};
use tokio::time::Instant;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Error as WsError, Message},
};
use tracing::{debug, error, info, warn};

use crate::config::DataConfig;
use crate::error::{MycryptoError, Result};
use crate::state::{
    ConnectionStatus, LogEntry, SourceStatus, SourceStatusLevel, StateUpdate, Ticker, Timeframe,
    OHLCV,
};

const INITIAL_RECONNECT_DELAY_SECS: u64 = 1;
const MAX_RECONNECT_DELAY_SECS: u64 = 60;
const PING_INTERVAL_SECS: u64 = 30;
const STALE_FEED_TIMEOUT_SECS: u64 = 30;
const RECONNECT_BUFFER_MS: u64 = 500;
const MAX_SEEN_KLINES: usize = 8_000;
const MAX_RECENT_PRICE_KEYS: usize = 256;

#[derive(Debug, Deserialize)]
struct CombinedStreamMessage {
    stream: String,
    data: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct BinanceSymbolTicker {
    #[serde(rename = "E")]
    _event_time: u64,
    #[serde(rename = "e")]
    _event_type: String,
    s: String,
    p: String,
    #[serde(rename = "P")]
    price_change_pct: String,
    c: String,
    b: String,
    a: String,
    h: String,
    l: String,
    v: String,
    q: String,
}

#[derive(Debug, Deserialize)]
struct BinanceKline {
    t: u64,
    #[serde(rename = "T")]
    _close_time: u64,
    s: String,
    i: String,
    o: String,
    c: String,
    h: String,
    l: String,
    v: String,
    n: u64,
    x: bool,
}

#[derive(Debug, Deserialize)]
struct BinanceKlineEvent {
    #[serde(rename = "E")]
    _event_time: u64,
    #[serde(rename = "e")]
    _event_type: String,
    #[serde(rename = "s")]
    _symbol: String,
    k: BinanceKline,
}

#[derive(Debug, Serialize)]
struct SubscriptionRequest {
    method: String,
    params: Vec<String>,
    id: u64,
}

#[derive(Debug, Clone)]
enum BufferedUpdate {
    MarketTick(Ticker),
    Candle {
        pair: String,
        timeframe: Timeframe,
        candle: OHLCV,
    },
}

/// Market data feed actor.
pub struct MarketFeed {
    ws_url: String,
    pairs: Vec<String>,
    pair_lookup: HashSet<String>,
    timeframes: Vec<Timeframe>,
    state_tx: mpsc::Sender<StateUpdate>,
    running: bool,

    reconnect_count_total: u64,
    consecutive_backoff_attempts: u32,
    feed_started_at: Instant,
    connected_since: Option<Instant>,
    connected_total: Duration,
    last_message_at: Instant,
    last_message_utc: DateTime<Utc>,

    recent_prices: HashMap<String, VecDeque<Decimal>>,
    recent_price_order: VecDeque<String>,
    seen_klines: HashSet<(String, Timeframe, i64)>,
    seen_kline_order: VecDeque<(String, Timeframe, i64)>,

    reconnecting: bool,
    reconnect_buffer: VecDeque<(Instant, BufferedUpdate)>,
    shutdown_rx: watch::Receiver<bool>,
}

impl MarketFeed {
    /// Creates a market feed actor with Binance stream settings and tracked pairs.
    pub fn new(
        config: &DataConfig,
        pairs: Vec<String>,
        state_tx: mpsc::Sender<StateUpdate>,
        shutdown_rx: watch::Receiver<bool>,
    ) -> Self {
        let timeframes = vec![
            Timeframe::M1,
            Timeframe::M5,
            Timeframe::M15,
            Timeframe::H1,
            Timeframe::H4,
        ];
        let pair_lookup: HashSet<String> =
            pairs.iter().map(|pair| pair.to_ascii_uppercase()).collect();

        Self {
            ws_url: config.binance_ws_url.clone(),
            pairs,
            pair_lookup,
            timeframes,
            state_tx,
            running: true,
            reconnect_count_total: 0,
            consecutive_backoff_attempts: 0,
            feed_started_at: Instant::now(),
            connected_since: None,
            connected_total: Duration::ZERO,
            last_message_at: Instant::now(),
            last_message_utc: Utc::now(),
            recent_prices: HashMap::new(),
            recent_price_order: VecDeque::new(),
            seen_klines: HashSet::new(),
            seen_kline_order: VecDeque::new(),
            reconnecting: false,
            reconnect_buffer: VecDeque::new(),
            shutdown_rx,
        }
    }

    /// Runs the feed lifecycle loop until shutdown is requested.
    ///
    /// This method manages connect/reconnect behavior, stream processing,
    /// telemetry updates, and status propagation to `AppState`.
    pub async fn run(&mut self) -> Result<()> {
        info!("Starting market feed for {} pairs", self.pairs.len());

        while self.running {
            if *self.shutdown_rx.borrow() {
                self.running = false;
                break;
            }

            match self.connect_and_stream().await {
                Ok(processed_count) => {
                    if processed_count > 0 {
                        self.consecutive_backoff_attempts = 0;
                    } else {
                        self.consecutive_backoff_attempts =
                            self.consecutive_backoff_attempts.saturating_add(1);
                    }
                    if self.running {
                        info!("WebSocket stream ended, reconnecting...");
                    }
                }
                Err(e) => {
                    self.consecutive_backoff_attempts =
                        self.consecutive_backoff_attempts.saturating_add(1);
                    error!("WebSocket error: {}", e);
                    self.send_status(ConnectionStatus::Error).await;
                    self.send_log(LogEntry::error(format!("Feed error: {}", e)))
                        .await;
                }
            }

            if self.running {
                self.reconnect_count_total = self.reconnect_count_total.saturating_add(1);
                self.reconnecting = true;
                let delay = self.jittered_backoff_delay();
                warn!(
                    "Reconnect #{} in {}ms",
                    self.reconnect_count_total,
                    delay.as_millis()
                );
                self.send_status(ConnectionStatus::Connecting).await;
                self.send_ws_telemetry().await;
                tokio::select! {
                    _ = tokio::time::sleep(delay) => {}
                    changed = self.shutdown_rx.changed() => {
                        if changed.is_err() || *self.shutdown_rx.borrow() {
                            self.running = false;
                            break;
                        }
                    }
                }
            }
        }

        info!("Market feed stopped");
        Ok(())
    }

    async fn connect_and_stream(&mut self) -> Result<usize> {
        self.send_status(ConnectionStatus::Connecting).await;

        let (ws_stream, _response) =
            connect_async(&self.ws_url)
                .await
                .map_err(|e| MycryptoError::WebSocketConnection {
                    url: self.ws_url.clone(),
                    reason: e.to_string(),
                })?;

        let (mut write, mut read) = ws_stream.split();

        let subscribe_msg = SubscriptionRequest {
            method: "SUBSCRIBE".to_string(),
            params: self.build_subscription_streams(),
            id: 1,
        };
        let msg_json = serde_json::to_string(&subscribe_msg).map_err(MycryptoError::Json)?;
        write.send(Message::Text(msg_json)).await.map_err(|e| {
            MycryptoError::WebSocketConnection {
                url: self.ws_url.clone(),
                reason: e.to_string(),
            }
        })?;

        self.connected_since = Some(Instant::now());
        self.send_status(ConnectionStatus::Connected).await;
        self.send_log(LogEntry::info("Market feed connected")).await;

        let mut processed_count = 0usize;
        let mut ping_interval = tokio::time::interval(Duration::from_secs(PING_INTERVAL_SECS));
        let mut watchdog = tokio::time::interval(Duration::from_secs(1));
        let mut buffer_until = if self.reconnecting {
            Some(Instant::now() + Duration::from_millis(RECONNECT_BUFFER_MS))
        } else {
            None
        };

        loop {
            tokio::select! {
                changed = self.shutdown_rx.changed() => {
                    if changed.is_err() || *self.shutdown_rx.borrow() {
                        self.running = false;
                        break;
                    }
                }
                msg = read.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            self.last_message_at = Instant::now();
                            self.last_message_utc = Utc::now();
                            self.send_ws_telemetry().await;

                            if let Some(update) = self.handle_message(text.as_ref()).await? {
                                processed_count += 1;
                                if let Some(until) = buffer_until {
                                    if Instant::now() < until {
                                        self.push_reconnect_buffer(update);
                                    } else {
                                        self.flush_reconnect_buffer().await?;
                                        buffer_until = None;
                                        self.emit_update(update).await?;
                                    }
                                } else {
                                    self.emit_update(update).await?;
                                }
                            }
                        }
                        Some(Ok(Message::Ping(data))) => {
                            if let Err(e) = write.send(Message::Pong(data)).await {
                                error!("Failed to send pong: {}", e);
                                break;
                            }
                        }
                        Some(Ok(Message::Close(_))) => {
                            info!("Received close frame");
                            break;
                        }
                        Some(Err(WsError::ConnectionClosed)) => {
                            info!("Connection closed");
                            break;
                        }
                        Some(Err(e)) => {
                            error!("WebSocket stream error: {}", e);
                            break;
                        }
                        None => {
                            info!("WebSocket stream ended");
                            break;
                        }
                        _ => {}
                    }
                }
                _ = ping_interval.tick() => {
                    if let Err(e) = write.send(Message::Ping(Vec::new())).await {
                        error!("Failed to send ping: {}", e);
                        break;
                    }
                }
                _ = watchdog.tick() => {
                    if self.last_message_at.elapsed() > Duration::from_secs(STALE_FEED_TIMEOUT_SECS) {
                        warn!("Feed stale for > {}s, forcing reconnect", STALE_FEED_TIMEOUT_SECS);
                        self.send_log(LogEntry::warn(format!(
                            "No market frames for {}s, reconnecting",
                            STALE_FEED_TIMEOUT_SECS
                        ))).await;
                        break;
                    }
                    if let Some(until) = buffer_until {
                        if Instant::now() >= until {
                            self.flush_reconnect_buffer().await?;
                            buffer_until = None;
                        }
                    }
                }
            }
        }

        if let Some(since) = self.connected_since.take() {
            self.connected_total += since.elapsed();
        }
        self.reconnecting = false;
        self.send_status(ConnectionStatus::Disconnected).await;
        self.send_ws_telemetry().await;

        Ok(processed_count)
    }

    fn build_subscription_streams(&self) -> Vec<String> {
        let mut streams = Vec::new();
        for pair in &self.pairs {
            let pair_lower = pair.to_lowercase();
            streams.push(format!("{}@ticker", pair_lower));
            for tf in &self.timeframes {
                streams.push(format!("{}@kline_{}", pair_lower, tf.as_binance_interval()));
            }
        }
        streams
    }

    async fn handle_message(&mut self, text: &str) -> Result<Option<BufferedUpdate>> {
        let root: serde_json::Value = match serde_json::from_str(text) {
            Ok(v) => v,
            Err(err) => {
                self.send_log(LogEntry::warn(format!(
                    "Dropped malformed WS frame: invalid JSON ({})",
                    err
                )))
                .await;
                return Ok(None);
            }
        };

        let msg: CombinedStreamMessage = match validate_and_parse_combined(root) {
            Ok(v) => v,
            Err(reason) => {
                self.send_log(LogEntry::warn(format!(
                    "Dropped malformed WS frame: {}",
                    reason
                )))
                .await;
                return Ok(None);
            }
        };

        if msg.stream.ends_with("@ticker") {
            return self.handle_ticker(&msg.data).await;
        }
        if msg.stream.contains("@kline_") {
            return self.handle_kline(&msg.data).await;
        }

        Ok(None)
    }

    async fn handle_ticker(&mut self, data: &serde_json::Value) -> Result<Option<BufferedUpdate>> {
        if let Err(reason) = validate_ticker_schema(data) {
            self.send_log(LogEntry::warn(format!("Dropped ticker frame: {}", reason)))
                .await;
            return Ok(None);
        }

        let ticker_data: BinanceSymbolTicker = match serde_json::from_value(data.clone()) {
            Ok(v) => v,
            Err(err) => {
                self.send_log(LogEntry::warn(format!(
                    "Dropped ticker frame: decode error ({})",
                    err
                )))
                .await;
                return Ok(None);
            }
        };

        if !self
            .pair_lookup
            .contains(&ticker_data.s.to_ascii_uppercase())
        {
            return Ok(None);
        }

        let price = parse_decimal(&ticker_data.c)?;
        if self.is_price_outlier(&ticker_data.s, price) {
            let median = self
                .recent_prices
                .get(&ticker_data.s)
                .and_then(median_decimal)
                .unwrap_or(price);
            self.send_log(LogEntry::warn(format!(
                "Dropped outlier ticker {}: price={} median={} (>5% deviation)",
                ticker_data.s, price, median
            )))
            .await;
            return Ok(None);
        }
        self.record_price(&ticker_data.s, price);

        let ticker = Ticker {
            pair: ticker_data.s,
            price,
            price_change_24h: parse_decimal(&ticker_data.p)?,
            price_change_pct_24h: parse_decimal(&ticker_data.price_change_pct)?,
            high_24h: parse_decimal(&ticker_data.h)?,
            low_24h: parse_decimal(&ticker_data.l)?,
            volume_24h: parse_decimal(&ticker_data.v)?,
            quote_volume_24h: parse_decimal(&ticker_data.q)?,
            bid_price: parse_decimal(&ticker_data.b)?,
            ask_price: parse_decimal(&ticker_data.a)?,
            updated_at: Utc::now(),
        };

        Ok(Some(BufferedUpdate::MarketTick(ticker)))
    }

    async fn handle_kline(&mut self, data: &serde_json::Value) -> Result<Option<BufferedUpdate>> {
        if let Err(reason) = validate_kline_schema(data) {
            self.send_log(LogEntry::warn(format!("Dropped kline frame: {}", reason)))
                .await;
            return Ok(None);
        }

        let kline_event: BinanceKlineEvent = match serde_json::from_value(data.clone()) {
            Ok(v) => v,
            Err(err) => {
                self.send_log(LogEntry::warn(format!(
                    "Dropped kline frame: decode error ({})",
                    err
                )))
                .await;
                return Ok(None);
            }
        };
        let kline = &kline_event.k;
        if !self.pair_lookup.contains(&kline.s.to_ascii_uppercase()) {
            return Ok(None);
        }
        let timeframe = match Timeframe::from_binance_interval(&kline.i) {
            Some(v) => v,
            None => {
                self.send_log(LogEntry::warn(format!(
                    "Dropped kline frame: unknown interval {}",
                    kline.i
                )))
                .await;
                return Ok(None);
            }
        };

        let Some(open_time_ms) = i64::try_from(kline.t).ok() else {
            self.send_log(LogEntry::warn(format!(
                "Dropped kline frame: open time overflow {}",
                kline.t
            )))
            .await;
            return Ok(None);
        };

        if !self.remember_kline(&kline.s, timeframe, open_time_ms) {
            debug!(
                "Ignored duplicate kline {} {} {}",
                kline.s, timeframe, kline.t
            );
            return Ok(None);
        }

        let candle = OHLCV {
            timestamp: timestamp_to_datetime(kline.t),
            open: parse_decimal(&kline.o)?,
            high: parse_decimal(&kline.h)?,
            low: parse_decimal(&kline.l)?,
            close: parse_decimal(&kline.c)?,
            volume: parse_decimal(&kline.v)?,
            trades: kline.n,
            closed: kline.x,
        };

        Ok(Some(BufferedUpdate::Candle {
            pair: kline.s.clone(),
            timeframe,
            candle,
        }))
    }

    fn remember_kline(&mut self, pair: &str, timeframe: Timeframe, open_time_ms: i64) -> bool {
        let key = (pair.to_string(), timeframe, open_time_ms);
        if self.seen_klines.contains(&key) {
            return false;
        }

        self.seen_klines.insert(key.clone());
        self.seen_kline_order.push_back(key);
        while self.seen_kline_order.len() > MAX_SEEN_KLINES {
            if let Some(old) = self.seen_kline_order.pop_front() {
                self.seen_klines.remove(&old);
            }
        }
        true
    }

    fn is_price_outlier(&self, pair: &str, price: Decimal) -> bool {
        let Some(window) = self.recent_prices.get(pair) else {
            return false;
        };
        should_reject_outlier(window, price)
    }

    fn record_price(&mut self, pair: &str, price: Decimal) {
        self.touch_recent_price_key(pair);
        let entry = self.recent_prices.entry(pair.to_string()).or_default();
        entry.push_back(price);
        while entry.len() > 10 {
            entry.pop_front();
        }
    }

    fn touch_recent_price_key(&mut self, pair: &str) {
        if let Some(idx) = self.recent_price_order.iter().position(|item| item == pair) {
            self.recent_price_order.remove(idx);
        }
        self.recent_price_order.push_back(pair.to_string());

        while self.recent_price_order.len() > MAX_RECENT_PRICE_KEYS {
            if let Some(evicted) = self.recent_price_order.pop_front() {
                self.recent_prices.remove(&evicted);
            }
        }
    }

    fn push_reconnect_buffer(&mut self, update: BufferedUpdate) {
        self.reconnect_buffer.push_back((Instant::now(), update));
        while self.reconnect_buffer.len() > 1024 {
            self.reconnect_buffer.pop_front();
        }
    }

    async fn flush_reconnect_buffer(&mut self) -> Result<()> {
        while let Some((_ts, update)) = self.reconnect_buffer.pop_front() {
            self.emit_update(update).await?;
        }
        Ok(())
    }

    async fn emit_update(&self, update: BufferedUpdate) -> Result<()> {
        match update {
            BufferedUpdate::MarketTick(ticker) => self
                .state_tx
                .send(StateUpdate::MarketTick(ticker))
                .await
                .map_err(|_| MycryptoError::channel_send("state_tx")),
            BufferedUpdate::Candle {
                pair,
                timeframe,
                candle,
            } => self
                .state_tx
                .send(StateUpdate::CandleUpdate {
                    pair,
                    timeframe,
                    candle,
                })
                .await
                .map_err(|_| MycryptoError::channel_send("state_tx")),
        }
    }

    fn jittered_backoff_delay(&self) -> Duration {
        let exp = INITIAL_RECONNECT_DELAY_SECS
            .saturating_mul(2u64.saturating_pow(self.consecutive_backoff_attempts.min(16)));
        let base_secs = exp.clamp(INITIAL_RECONNECT_DELAY_SECS, MAX_RECONNECT_DELAY_SECS);
        let seed = (Utc::now().timestamp_millis() as i128
            + self.reconnect_count_total as i128
            + self.consecutive_backoff_attempts as i128)
            .unsigned_abs() as u64;
        jitter_duration(Duration::from_secs(base_secs), seed)
    }

    fn uptime_ratio(&self) -> f32 {
        let elapsed = self.feed_started_at.elapsed().as_secs_f32();
        if elapsed <= f32::EPSILON {
            return 1.0;
        }
        let mut connected = self.connected_total;
        if let Some(since) = self.connected_since {
            connected += since.elapsed();
        }
        (connected.as_secs_f32() / elapsed).clamp(0.0, 1.0)
    }

    async fn send_ws_telemetry(&self) {
        let _ = self
            .state_tx
            .send(StateUpdate::WsFeedTelemetry {
                reconnect_count: self.reconnect_count_total,
                last_message_at: self.last_message_utc,
                uptime_ratio: self.uptime_ratio(),
            })
            .await;
    }

    async fn send_status(&self, status: ConnectionStatus) {
        let _ = self
            .state_tx
            .send(StateUpdate::FeedStatusChanged(status))
            .await;

        let (level, runtime_status, detail) = match status {
            ConnectionStatus::Connected => (
                SourceStatusLevel::Connected,
                "Healthy",
                "realtime stream active",
            ),
            ConnectionStatus::Connecting => (
                SourceStatusLevel::Warn,
                "Degraded",
                "reconnecting websocket",
            ),
            ConnectionStatus::Disconnected => (
                SourceStatusLevel::Warn,
                "Degraded",
                "websocket disconnected",
            ),
            ConnectionStatus::Error => (SourceStatusLevel::Error, "Dead", "websocket error"),
        };

        let _ = self
            .state_tx
            .send(StateUpdate::SourceHealthChanged(SourceStatus {
                name: "Binance WS".to_string(),
                level,
                detail: detail.to_string(),
                last_ok: if matches!(status, ConnectionStatus::Connected) {
                    Some(self.last_message_utc)
                } else {
                    None
                },
                consecutive_failures: self.consecutive_backoff_attempts,
                runtime_status: runtime_status.to_string(),
            }))
            .await;
    }

    async fn send_log(&self, entry: LogEntry) {
        let _ = self.state_tx.send(StateUpdate::Log(entry)).await;
    }

    /// Stops the feed loop on the next iteration tick.
    pub fn stop(&mut self) {
        self.running = false;
    }
}

fn parse_decimal(s: &str) -> Result<Decimal> {
    Decimal::from_str(s)
        .map_err(|e| MycryptoError::MarketDataParse(format!("Invalid decimal '{}': {}", s, e)))
}

fn timestamp_to_datetime(ts_ms: u64) -> DateTime<Utc> {
    let ts_ms = i64::try_from(ts_ms).unwrap_or(i64::MAX);
    Utc.timestamp_millis_opt(ts_ms)
        .single()
        .unwrap_or_else(Utc::now)
}

fn jitter_duration(base: Duration, seed: u64) -> Duration {
    let jitter_pct = (seed % 41) as i64 - 20; // -20..20
    let base_ms = base.as_millis() as i64;
    let jittered = (base_ms * (100 + jitter_pct) / 100).max(1) as u64;
    Duration::from_millis(jittered)
}

fn median_decimal(window: &VecDeque<Decimal>) -> Option<Decimal> {
    if window.is_empty() {
        return None;
    }
    let mut values: Vec<Decimal> = window.iter().copied().collect();
    values.sort();
    let mid = values.len() / 2;
    if values.len().is_multiple_of(2) {
        Some((values[mid - 1] + values[mid]) / Decimal::from(2))
    } else {
        Some(values[mid])
    }
}

fn should_reject_outlier(window: &VecDeque<Decimal>, new_price: Decimal) -> bool {
    if window.len() < 10 {
        return false;
    }
    let Some(median) = median_decimal(window) else {
        return false;
    };
    if median <= Decimal::ZERO {
        return false;
    }
    let deviation = ((new_price - median).abs() / median)
        .to_f64()
        .unwrap_or(0.0);
    deviation > 0.05
}

fn validate_and_parse_combined(
    value: serde_json::Value,
) -> std::result::Result<CombinedStreamMessage, String> {
    let root = value
        .as_object()
        .ok_or_else(|| "root payload must be an object".to_string())?;
    let stream = root
        .get("stream")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing string field 'stream'".to_string())?;
    if stream.trim().is_empty() {
        return Err("stream must not be empty".to_string());
    }
    let data = root
        .get("data")
        .ok_or_else(|| "missing field 'data'".to_string())?;
    if !data.is_object() {
        return Err("field 'data' must be an object".to_string());
    }

    serde_json::from_value::<CombinedStreamMessage>(serde_json::Value::Object(root.clone()))
        .map_err(|e| format!("combined stream decode failed: {}", e))
}

fn validate_ticker_schema(data: &serde_json::Value) -> std::result::Result<(), String> {
    let obj = data
        .as_object()
        .ok_or_else(|| "ticker data must be object".to_string())?;
    for key in ["s", "p", "P", "c", "b", "a", "o", "h", "l", "v", "q"] {
        if !obj.get(key).is_some_and(|v| v.is_string()) {
            return Err(format!("ticker missing/invalid '{}': expected string", key));
        }
    }
    Ok(())
}

fn validate_kline_schema(data: &serde_json::Value) -> std::result::Result<(), String> {
    let obj = data
        .as_object()
        .ok_or_else(|| "kline event must be object".to_string())?;
    if !obj.get("k").is_some_and(|v| v.is_object()) {
        return Err("kline missing object 'k'".to_string());
    }
    let k = obj
        .get("k")
        .and_then(|v| v.as_object())
        .ok_or_else(|| "kline 'k' invalid".to_string())?;
    for key in ["s", "i", "o", "c", "h", "l", "v"] {
        if !k.get(key).is_some_and(|v| v.is_string()) {
            return Err(format!("kline missing/invalid '{}': expected string", key));
        }
    }
    if !k.get("t").is_some_and(|v| v.is_u64()) {
        return Err("kline missing/invalid 't': expected u64".to_string());
    }
    if !k.get("n").is_some_and(|v| v.is_u64()) {
        return Err("kline missing/invalid 'n': expected u64".to_string());
    }
    if !k.get("x").is_some_and(|v| v.is_boolean()) {
        return Err("kline missing/invalid 'x': expected bool".to_string());
    }
    Ok(())
}

/// Spawns the market feed on the current Tokio runtime handle.
///
/// A local shutdown channel is created internally and not externally exposed.
pub fn spawn_market_feed(
    config: &DataConfig,
    pairs: Vec<String>,
    state_tx: mpsc::Sender<StateUpdate>,
) -> tokio::task::JoinHandle<()> {
    let handle = tokio::runtime::Handle::current();
    let (_shutdown_tx, shutdown_rx) = watch::channel(false);
    spawn_market_feed_on(&handle, config, pairs, state_tx, shutdown_rx)
}

/// Spawns the market feed using an explicit runtime handle and shutdown signal.
///
/// Use this variant when the caller needs deterministic lifecycle control in
/// tests or embedding scenarios.
pub fn spawn_market_feed_on(
    handle: &tokio::runtime::Handle,
    config: &DataConfig,
    pairs: Vec<String>,
    state_tx: mpsc::Sender<StateUpdate>,
    shutdown_rx: watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    let config = config.clone();
    handle.spawn(async move {
        let mut feed = MarketFeed::new(&config, pairs, state_tx, shutdown_rx);
        if let Err(e) = feed.run().await {
            error!("Market feed fatal error: {}", e);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_decimal() {
        assert_eq!(parse_decimal("50000.00").unwrap(), Decimal::from(50000));
        assert!(parse_decimal("invalid").is_err());
    }

    #[test]
    fn test_timestamp_conversion() {
        let ts_ms = 1704067200000_u64;
        let dt = timestamp_to_datetime(ts_ms);
        assert_eq!(dt.timestamp_millis(), ts_ms as i64);
    }

    #[test]
    fn test_subscription_streams() {
        let config = DataConfig::default();
        let (tx, _rx) = mpsc::channel(100);
        let (_shutdown_tx, shutdown_rx) = watch::channel(false);
        let feed = MarketFeed::new(&config, vec!["BTCUSDT".to_string()], tx, shutdown_rx);

        let streams = feed.build_subscription_streams();
        assert!(streams.contains(&"btcusdt@ticker".to_string()));
        assert!(streams.contains(&"btcusdt@kline_1m".to_string()));
        assert!(streams.contains(&"btcusdt@kline_1h".to_string()));
    }

    #[test]
    fn test_outlier_filter_rejection() {
        let mut window = VecDeque::new();
        for p in [100, 101, 100, 99, 101, 100, 102, 100, 99, 101] {
            window.push_back(Decimal::from(p));
        }

        assert!(should_reject_outlier(&window, Decimal::from(120)));
        assert!(!should_reject_outlier(&window, Decimal::from(103)));
    }

    #[test]
    fn test_kline_deduplication() {
        let config = DataConfig::default();
        let (tx, _rx) = mpsc::channel(8);
        let (_shutdown_tx, shutdown_rx) = watch::channel(false);
        let mut feed = MarketFeed::new(&config, vec!["BTCUSDT".to_string()], tx, shutdown_rx);

        assert!(feed.remember_kline("BTCUSDT", Timeframe::M5, 1_700_000_000_000));
        assert!(!feed.remember_kline("BTCUSDT", Timeframe::M5, 1_700_000_000_000));
        assert!(feed.remember_kline("BTCUSDT", Timeframe::M5, 1_700_000_300_000));
    }

    #[test]
    fn test_recent_price_key_cache_is_bounded() {
        let config = DataConfig::default();
        let (tx, _rx) = mpsc::channel(8);
        let (_shutdown_tx, shutdown_rx) = watch::channel(false);
        let mut feed = MarketFeed::new(&config, vec!["BTCUSDT".to_string()], tx, shutdown_rx);

        for i in 0..(MAX_RECENT_PRICE_KEYS + 40) {
            let pair = format!("PAIR{:03}", i);
            feed.record_price(&pair, Decimal::from(100 + i as i64));
        }

        assert!(feed.recent_prices.len() <= MAX_RECENT_PRICE_KEYS);
        assert!(feed.recent_price_order.len() <= MAX_RECENT_PRICE_KEYS);
    }
}
