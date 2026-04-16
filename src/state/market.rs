//! Market data domain types.
//!
//! This module defines structures for real-time market data:
//! - `Ticker` - latest price and 24h stats
//! - `OHLCV` - candlestick data
//! - `OrderBookLevel` / `OrderBook` - depth data
//! - `Timeframe` - candle intervals

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Supported candle timeframes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Timeframe {
    /// 1 minute candles.
    M1,
    /// 5 minute candles.
    M5,
    /// 15 minute candles.
    M15,
    /// 1 hour candles.
    H1,
    /// 4 hour candles.
    H4,
    /// Daily candles.
    D1,
    /// Weekly candles.
    W1,
    /// Monthly candles.
    MO1,
}

impl Timeframe {
    /// Returns the timeframe duration in seconds.
    pub fn as_seconds(&self) -> i64 {
        match self {
            Timeframe::M1 => 60,
            Timeframe::M5 => 300,
            Timeframe::M15 => 900,
            Timeframe::H1 => 3600,
            Timeframe::H4 => 14400,
            Timeframe::D1 => 86400,
            Timeframe::W1 => 604800,
            Timeframe::MO1 => 2_592_000,
        }
    }

    /// Returns the Binance interval string.
    pub fn as_binance_interval(&self) -> &'static str {
        match self {
            Timeframe::M1 => "1m",
            Timeframe::M5 => "5m",
            Timeframe::M15 => "15m",
            Timeframe::H1 => "1h",
            Timeframe::H4 => "4h",
            Timeframe::D1 => "1d",
            Timeframe::W1 => "1w",
            Timeframe::MO1 => "1M",
        }
    }

    /// Parses a Binance interval string.
    pub fn from_binance_interval(s: &str) -> Option<Self> {
        match s {
            "1m" => Some(Timeframe::M1),
            "5m" => Some(Timeframe::M5),
            "15m" => Some(Timeframe::M15),
            "1h" => Some(Timeframe::H1),
            "4h" => Some(Timeframe::H4),
            "1d" => Some(Timeframe::D1),
            "1w" => Some(Timeframe::W1),
            "1M" => Some(Timeframe::MO1),
            _ => None,
        }
    }

    /// Parses chart/user timeframe labels.
    pub fn from_chart_label(s: &str) -> Option<Self> {
        let trimmed = s.trim();
        if trimmed == "1M" {
            return Some(Timeframe::MO1);
        }

        match trimmed.to_ascii_lowercase().as_str() {
            "1m" => Some(Timeframe::M1),
            "5m" => Some(Timeframe::M5),
            "15m" => Some(Timeframe::M15),
            "1h" => Some(Timeframe::H1),
            "4h" => Some(Timeframe::H4),
            "1d" | "d" => Some(Timeframe::D1),
            "1w" | "w" | "7d" => Some(Timeframe::W1),
            "1mo" | "1mon" | "1month" => Some(Timeframe::MO1),
            _ => None,
        }
    }
}

impl std::fmt::Display for Timeframe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Timeframe::M1 => write!(f, "1m"),
            Timeframe::M5 => write!(f, "5m"),
            Timeframe::M15 => write!(f, "15m"),
            Timeframe::H1 => write!(f, "1h"),
            Timeframe::H4 => write!(f, "4h"),
            Timeframe::D1 => write!(f, "1d"),
            Timeframe::W1 => write!(f, "1w"),
            Timeframe::MO1 => write!(f, "1M"),
        }
    }
}

/// Real-time ticker data for a trading pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ticker {
    /// Trading pair symbol (e.g., "BTCUSDT").
    pub pair: String,

    /// Last traded price.
    pub price: Decimal,

    /// 24h price change.
    pub price_change_24h: Decimal,

    /// 24h price change percentage.
    pub price_change_pct_24h: Decimal,

    /// 24h high price.
    pub high_24h: Decimal,

    /// 24h low price.
    pub low_24h: Decimal,

    /// 24h trading volume in base currency.
    pub volume_24h: Decimal,

    /// 24h trading volume in quote currency.
    pub quote_volume_24h: Decimal,

    /// Best bid price.
    pub bid_price: Decimal,

    /// Best ask price.
    pub ask_price: Decimal,

    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
}

impl Ticker {
    /// Creates a new ticker with default values.
    pub fn new(pair: String) -> Self {
        Self {
            pair,
            price: Decimal::ZERO,
            price_change_24h: Decimal::ZERO,
            price_change_pct_24h: Decimal::ZERO,
            high_24h: Decimal::ZERO,
            low_24h: Decimal::ZERO,
            volume_24h: Decimal::ZERO,
            quote_volume_24h: Decimal::ZERO,
            bid_price: Decimal::ZERO,
            ask_price: Decimal::ZERO,
            updated_at: Utc::now(),
        }
    }

    /// Calculates the bid-ask spread.
    pub fn spread(&self) -> Decimal {
        self.ask_price - self.bid_price
    }

    /// Calculates the spread as percentage of price.
    pub fn spread_pct(&self) -> Decimal {
        if self.price > Decimal::ZERO {
            (self.spread() / self.price) * Decimal::from(100)
        } else {
            Decimal::ZERO
        }
    }

    /// Returns true if price is up from 24h ago.
    pub fn is_up(&self) -> bool {
        self.price_change_24h > Decimal::ZERO
    }
}

/// OHLCV candlestick data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OHLCV {
    /// Candle open timestamp.
    pub timestamp: DateTime<Utc>,

    /// Opening price.
    pub open: Decimal,

    /// Highest price.
    pub high: Decimal,

    /// Lowest price.
    pub low: Decimal,

    /// Closing price.
    pub close: Decimal,

    /// Trading volume in base currency.
    pub volume: Decimal,

    /// Number of trades.
    pub trades: u64,

    /// Whether this candle is complete.
    pub closed: bool,
}

impl OHLCV {
    /// Creates a new candle starting at the given timestamp and price.
    pub fn new(timestamp: DateTime<Utc>, price: Decimal) -> Self {
        Self {
            timestamp,
            open: price,
            high: price,
            low: price,
            close: price,
            volume: Decimal::ZERO,
            trades: 0,
            closed: false,
        }
    }

    /// Updates the candle with a new trade.
    pub fn update(&mut self, price: Decimal, volume: Decimal) {
        self.close = price;
        if price > self.high {
            self.high = price;
        }
        if price < self.low {
            self.low = price;
        }
        self.volume += volume;
        self.trades = self.trades.saturating_add(1);
    }

    /// Returns true if this is a bullish (green) candle.
    pub fn is_bullish(&self) -> bool {
        self.close >= self.open
    }

    /// Returns the candle body size.
    pub fn body(&self) -> Decimal {
        (self.close - self.open).abs()
    }

    /// Returns the candle range (high - low).
    pub fn range(&self) -> Decimal {
        self.high - self.low
    }

    /// Returns the upper wick size.
    pub fn upper_wick(&self) -> Decimal {
        self.high - self.close.max(self.open)
    }

    /// Returns the lower wick size.
    pub fn lower_wick(&self) -> Decimal {
        self.close.min(self.open) - self.low
    }
}

/// A collection of candles for a specific pair and timeframe.
#[derive(Debug, Clone)]
pub struct CandleBuffer {
    /// Trading pair.
    pub pair: String,

    /// Timeframe of these candles.
    pub timeframe: Timeframe,

    /// Candles, ordered oldest to newest.
    pub candles: VecDeque<OHLCV>,

    /// Maximum number of candles to keep.
    pub max_size: usize,
}

impl CandleBuffer {
    /// Creates a new candle buffer.
    pub fn new(pair: String, timeframe: Timeframe, max_size: usize) -> Self {
        Self {
            pair,
            timeframe,
            candles: VecDeque::with_capacity(max_size),
            max_size,
        }
    }

    /// Adds a candle to the buffer.
    ///
    /// If the buffer is full, the oldest candle is removed.
    pub fn push(&mut self, candle: OHLCV) {
        if self.candles.len() >= self.max_size {
            self.candles.pop_front();
        }
        self.candles.push_back(candle);
    }

    /// Returns the most recent candle.
    pub fn latest(&self) -> Option<&OHLCV> {
        self.candles.back()
    }

    /// Returns a mutable reference to the most recent candle.
    pub fn latest_mut(&mut self) -> Option<&mut OHLCV> {
        self.candles.back_mut()
    }

    /// Returns the n most recent candles.
    pub fn last_n(&self, n: usize) -> Vec<&OHLCV> {
        self.candles.iter().rev().take(n).collect()
    }

    /// Returns all closing prices.
    pub fn closes(&self) -> Vec<Decimal> {
        self.candles.iter().map(|c| c.close).collect()
    }

    /// Returns all volumes.
    pub fn volumes(&self) -> Vec<Decimal> {
        self.candles.iter().map(|c| c.volume).collect()
    }

    /// Returns the number of candles.
    pub fn len(&self) -> usize {
        self.candles.len()
    }

    /// Returns true if buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.candles.is_empty()
    }
}

/// Order book price level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookLevel {
    /// Price level.
    pub price: Decimal,
    /// Quantity at this level.
    pub quantity: Decimal,
}

/// Order book depth data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBook {
    /// Trading pair symbol.
    pub pair: String,

    /// Bid levels (buy orders), sorted by price descending.
    pub bids: Vec<OrderBookLevel>,

    /// Ask levels (sell orders), sorted by price ascending.
    pub asks: Vec<OrderBookLevel>,

    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
}

impl OrderBook {
    /// Creates an empty order book.
    pub fn new(pair: String) -> Self {
        Self {
            pair,
            bids: Vec::new(),
            asks: Vec::new(),
            updated_at: Utc::now(),
        }
    }

    /// Returns the best bid price.
    pub fn best_bid(&self) -> Option<Decimal> {
        self.bids.first().map(|l| l.price)
    }

    /// Returns the best ask price.
    pub fn best_ask(&self) -> Option<Decimal> {
        self.asks.first().map(|l| l.price)
    }

    /// Returns the mid price.
    pub fn mid_price(&self) -> Option<Decimal> {
        match (self.best_bid(), self.best_ask()) {
            (Some(bid), Some(ask)) => Some((bid + ask) / Decimal::from(2)),
            _ => None,
        }
    }

    /// Returns the spread.
    pub fn spread(&self) -> Option<Decimal> {
        match (self.best_bid(), self.best_ask()) {
            (Some(bid), Some(ask)) => Some(ask - bid),
            _ => None,
        }
    }

    /// Calculates total bid volume within a price range.
    pub fn bid_volume(&self, depth_pct: Decimal) -> Decimal {
        if let Some(best) = self.best_bid() {
            let threshold = best * (Decimal::ONE - depth_pct / Decimal::from(100));
            self.bids
                .iter()
                .filter(|l| l.price >= threshold)
                .map(|l| l.quantity)
                .sum()
        } else {
            Decimal::ZERO
        }
    }

    /// Calculates total ask volume within a price range.
    pub fn ask_volume(&self, depth_pct: Decimal) -> Decimal {
        if let Some(best) = self.best_ask() {
            let threshold = best * (Decimal::ONE + depth_pct / Decimal::from(100));
            self.asks
                .iter()
                .filter(|l| l.price <= threshold)
                .map(|l| l.quantity)
                .sum()
        } else {
            Decimal::ZERO
        }
    }
}

/// Aggregated market data for a trading pair.
#[derive(Debug, Clone)]
pub struct MarketData {
    /// Trading pair symbol.
    pub pair: String,

    /// Latest ticker data.
    pub ticker: Ticker,

    /// Order book depth.
    pub order_book: OrderBook,

    /// Candle buffers by timeframe.
    pub candles: std::collections::HashMap<Timeframe, CandleBuffer>,

    /// Current funding rate (for perpetual futures, if available).
    pub funding_rate: Option<Decimal>,

    /// Next funding time.
    pub next_funding_time: Option<DateTime<Utc>>,
}

impl MarketData {
    /// Creates new market data for a pair.
    pub fn new(pair: String, candle_buffer_size: usize) -> Self {
        let mut candles = std::collections::HashMap::new();

        // Initialize buffers for common timeframes
        for tf in [
            Timeframe::M1,
            Timeframe::M5,
            Timeframe::M15,
            Timeframe::H1,
            Timeframe::H4,
            Timeframe::D1,
            Timeframe::W1,
            Timeframe::MO1,
        ] {
            candles.insert(tf, CandleBuffer::new(pair.clone(), tf, candle_buffer_size));
        }

        Self {
            pair: pair.clone(),
            ticker: Ticker::new(pair.clone()),
            order_book: OrderBook::new(pair),
            candles,
            funding_rate: None,
            next_funding_time: None,
        }
    }

    /// Gets the candle buffer for a timeframe.
    pub fn get_candles(&self, tf: Timeframe) -> Option<&CandleBuffer> {
        self.candles.get(&tf)
    }

    /// Gets a mutable candle buffer for a timeframe.
    pub fn get_candles_mut(&mut self, tf: Timeframe) -> Option<&mut CandleBuffer> {
        self.candles.get_mut(&tf)
    }

    /// Updates ticker price.
    pub fn update_price(&mut self, price: Decimal) {
        self.ticker.price = price;
        self.ticker.updated_at = Utc::now();
    }
}

/// Market update event sent through channels.
#[derive(Debug, Clone)]
pub enum MarketUpdate {
    /// New ticker data.
    Tick(Ticker),
    /// Candle update.
    Candle {
        pair: String,
        timeframe: Timeframe,
        candle: OHLCV,
    },
    /// Order book update.
    Depth(OrderBook),
    /// Funding rate update.
    Funding {
        pair: String,
        rate: Decimal,
        next_time: DateTime<Utc>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeframe_conversions() {
        assert_eq!(Timeframe::H1.as_seconds(), 3600);
        assert_eq!(Timeframe::H4.as_binance_interval(), "4h");
        assert_eq!(Timeframe::from_binance_interval("1d"), Some(Timeframe::D1));
        assert_eq!(Timeframe::from_binance_interval("1M"), Some(Timeframe::MO1));
        assert_eq!(Timeframe::from_chart_label("7d"), Some(Timeframe::W1));
        assert_eq!(Timeframe::from_chart_label("1mo"), Some(Timeframe::MO1));
        assert_eq!(Timeframe::from_binance_interval("invalid"), None);
    }

    #[test]
    fn test_candle_bullish_bearish() {
        let mut candle = OHLCV::new(Utc::now(), Decimal::from(100));
        candle.close = Decimal::from(110);
        assert!(candle.is_bullish());

        candle.close = Decimal::from(90);
        assert!(!candle.is_bullish());
    }

    #[test]
    fn test_candle_buffer() {
        let mut buffer = CandleBuffer::new("BTCUSDT".to_string(), Timeframe::H1, 3);

        for i in 1..=5 {
            buffer.push(OHLCV::new(Utc::now(), Decimal::from(i * 1000)));
        }

        assert_eq!(buffer.len(), 3);
        assert_eq!(buffer.latest().unwrap().open, Decimal::from(5000));
    }

    #[test]
    fn test_ticker_spread() {
        let mut ticker = Ticker::new("BTCUSDT".to_string());
        ticker.bid_price = Decimal::from(50000);
        ticker.ask_price = Decimal::from(50010);
        ticker.price = Decimal::from(50005);

        assert_eq!(ticker.spread(), Decimal::from(10));
    }

    #[test]
    fn test_order_book_mid_price() {
        let mut book = OrderBook::new("BTCUSDT".to_string());
        book.bids.push(OrderBookLevel {
            price: Decimal::from(50000),
            quantity: Decimal::ONE,
        });
        book.asks.push(OrderBookLevel {
            price: Decimal::from(50010),
            quantity: Decimal::ONE,
        });

        assert_eq!(book.mid_price(), Some(Decimal::from(50005)));
    }
}
