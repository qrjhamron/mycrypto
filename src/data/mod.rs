//! Data ingestion and aggregation module.
//!
//! This module handles:
//! - WebSocket connection to Binance for market data
//! - Raw tick aggregation into OHLCV candles
//! - Technical indicator calculations
//!
//! # Architecture
//!
//! The data module follows an actor-like pattern:
//! - `MarketFeed` runs as a background task, connecting to Binance WebSocket
//! - It sends `StateUpdate` messages through channels to the main thread
//! - The main thread owns `AppState` and applies updates atomically
//!
//! # Usage
//!
//! ```ignore
//! use mycrypto::data::{spawn_market_feed, IndicatorSnapshot};
//!
//! // Spawn the market feed
//! let feed_handle = spawn_market_feed(&config.data, pairs, state_tx);
//!
//! // Calculate indicators from candles
//! let snapshot = IndicatorSnapshot::calculate(&candles);
//! ```

pub mod aggregator;
pub mod feed;
pub mod indicators;
pub mod sources;

// Re-export commonly used items
pub use aggregator::{align_to_timeframe, resample_candles, CandleAggregator};
pub use feed::{spawn_market_feed, spawn_market_feed_on};
pub use indicators::{
    atr, atr_percent, bollinger_bands, ema, ema_series, macd, momentum, obv, rsi, sma,
    standard_deviation, stochastic, support_resistance, vwap, BollingerBands, IndicatorSnapshot,
    MacdResult, MacdSignal, RsiZone, StochasticResult, SupportResistance,
};
pub use sources::{spawn_sources_aggregator, spawn_sources_aggregator_on};
