//! OHLCV candle aggregation from raw trade ticks.
//!
//! This module provides functionality to aggregate raw trade data into
//! OHLCV candlesticks at various timeframes. It handles:
//!
//! - Tick aggregation into 1-minute base candles
//! - Resampling 1-minute candles into higher timeframes
//! - Maintaining rolling buffers of candles per timeframe
//!
//! # Note
//!
//! When using Binance WebSocket, we typically receive pre-computed klines
//! directly, so this aggregator is primarily useful for:
//! 1. Aggregating from trade streams (if subscribed)
//! 2. Computing custom timeframes not provided by Binance
//! 3. Local aggregation when doing backtesting

use chrono::{DateTime, Datelike, Duration, DurationRound, TimeZone, Utc};
use rust_decimal::Decimal;
use std::collections::HashMap;
use tracing::debug;

use crate::state::{CandleBuffer, Timeframe, OHLCV};

/// Aggregates raw ticks into OHLCV candles.
///
/// This struct maintains partial candles and emits completed candles
/// when a timeframe boundary is crossed.
#[derive(Debug)]
pub struct CandleAggregator {
    /// Trading pair this aggregator handles.
    pair: String,

    /// Current partial candles per timeframe.
    partial_candles: HashMap<Timeframe, OHLCV>,

    /// Completed candle buffers per timeframe.
    buffers: HashMap<Timeframe, CandleBuffer>,
}

impl CandleAggregator {
    /// Creates a new candle aggregator for a trading pair.
    pub fn new(pair: String, buffer_size: usize) -> Self {
        let timeframes = [
            Timeframe::M1,
            Timeframe::M5,
            Timeframe::M15,
            Timeframe::H1,
            Timeframe::H4,
            Timeframe::D1,
            Timeframe::W1,
            Timeframe::MO1,
        ];

        let mut partial_candles = HashMap::new();
        let mut buffers = HashMap::new();

        for tf in timeframes {
            buffers.insert(tf, CandleBuffer::new(pair.clone(), tf, buffer_size));
            // Partial candles are created on first tick
            partial_candles.insert(tf, OHLCV::new(Utc::now(), Decimal::ZERO));
        }

        Self {
            pair,
            partial_candles,
            buffers,
        }
    }

    /// Processes a raw trade tick.
    ///
    /// Returns a list of newly completed candles (if any timeframe boundary was crossed).
    pub fn process_tick(
        &mut self,
        timestamp: DateTime<Utc>,
        price: Decimal,
        volume: Decimal,
    ) -> Vec<(Timeframe, OHLCV)> {
        let mut completed = Vec::new();

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
            let candle_start = align_to_timeframe(timestamp, tf);

            if let Some(partial) = self.partial_candles.get_mut(&tf) {
                // Check if we've crossed into a new candle period
                if partial.timestamp != candle_start && partial.trades > 0 {
                    // Complete the current candle
                    let mut completed_candle = partial.clone();
                    completed_candle.closed = true;

                    // Store in buffer
                    if let Some(buffer) = self.buffers.get_mut(&tf) {
                        buffer.push(completed_candle.clone());
                    }

                    completed.push((tf, completed_candle));

                    // Start new candle
                    *partial = OHLCV::new(candle_start, price);
                    partial.update(price, volume);
                } else if partial.trades == 0 {
                    // First tick for this candle
                    *partial = OHLCV::new(candle_start, price);
                    partial.update(price, volume);
                } else {
                    // Update existing candle
                    partial.update(price, volume);
                }
            }
        }

        if !completed.is_empty() {
            debug!("Completed {} candles for {}", completed.len(), self.pair);
        }

        completed
    }

    /// Updates a candle directly (used when receiving pre-computed klines from Binance).
    ///
    /// If the candle is closed, it's added to the buffer.
    /// If not closed, it updates the partial candle.
    pub fn update_candle(&mut self, tf: Timeframe, candle: OHLCV) {
        if candle.closed {
            // Add to buffer
            if let Some(buffer) = self.buffers.get_mut(&tf) {
                // Avoid duplicates by checking timestamp
                if let Some(latest) = buffer.latest() {
                    if latest.timestamp == candle.timestamp {
                        return; // Already have this candle
                    }
                }
                buffer.push(candle.clone());
            }
        }

        // Update partial candle
        if let Some(partial) = self.partial_candles.get_mut(&tf) {
            *partial = candle;
        }
    }

    /// Gets the candle buffer for a timeframe.
    pub fn get_buffer(&self, tf: Timeframe) -> Option<&CandleBuffer> {
        self.buffers.get(&tf)
    }

    /// Gets all closing prices for a timeframe.
    pub fn closes(&self, tf: Timeframe) -> Vec<Decimal> {
        self.buffers
            .get(&tf)
            .map(|b| b.closes())
            .unwrap_or_default()
    }

    /// Gets the current partial candle for a timeframe.
    pub fn partial(&self, tf: Timeframe) -> Option<&OHLCV> {
        self.partial_candles.get(&tf)
    }

    /// Gets the trading pair.
    pub fn pair(&self) -> &str {
        &self.pair
    }

    /// Loads historical candles into the buffer.
    ///
    /// Used for initializing the aggregator with historical data.
    pub fn load_historical(&mut self, tf: Timeframe, candles: Vec<OHLCV>) {
        if let Some(buffer) = self.buffers.get_mut(&tf) {
            for candle in candles {
                buffer.push(candle);
            }
        }
    }

    /// Gets the number of candles in a buffer.
    pub fn candle_count(&self, tf: Timeframe) -> usize {
        self.buffers.get(&tf).map(|b| b.len()).unwrap_or(0)
    }

    /// Checks if we have enough data for indicator calculations.
    pub fn has_min_candles(&self, tf: Timeframe, min: usize) -> bool {
        self.candle_count(tf) >= min
    }
}

/// Aligns a timestamp to the start of its timeframe period.
pub fn align_to_timeframe(ts: DateTime<Utc>, tf: Timeframe) -> DateTime<Utc> {
    match tf {
        Timeframe::M1 => ts.duration_trunc(Duration::minutes(1)).unwrap_or(ts),
        Timeframe::M5 => ts.duration_trunc(Duration::minutes(5)).unwrap_or(ts),
        Timeframe::M15 => ts.duration_trunc(Duration::minutes(15)).unwrap_or(ts),
        Timeframe::H1 => ts.duration_trunc(Duration::hours(1)).unwrap_or(ts),
        Timeframe::H4 => ts.duration_trunc(Duration::hours(4)).unwrap_or(ts),
        Timeframe::D1 => ts.duration_trunc(Duration::days(1)).unwrap_or(ts),
        Timeframe::W1 => ts.duration_trunc(Duration::weeks(1)).unwrap_or(ts),
        Timeframe::MO1 => Utc
            .with_ymd_and_hms(ts.year(), ts.month(), 1, 0, 0, 0)
            .single()
            .unwrap_or(ts),
    }
}

/// Resamples candles from a lower timeframe to a higher timeframe.
///
/// For example, resampling 5 M1 candles into 1 M5 candle.
pub fn resample_candles(candles: &[OHLCV], target_tf: Timeframe) -> Option<OHLCV> {
    if candles.is_empty() {
        return None;
    }

    let first = candles.first()?;
    let last = candles.last()?;

    let high = candles.iter().map(|c| c.high).max()?;
    let low = candles.iter().map(|c| c.low).min()?;
    let volume: Decimal = candles.iter().map(|c| c.volume).sum();
    let trades: u64 = candles
        .iter()
        .fold(0u64, |acc, candle| acc.saturating_add(candle.trades));

    Some(OHLCV {
        timestamp: align_to_timeframe(first.timestamp, target_tf),
        open: first.open,
        high,
        low,
        close: last.close,
        volume,
        trades,
        closed: last.closed,
    })
}

/// Groups candles by their target timeframe period.
pub fn group_by_period(
    candles: &[OHLCV],
    target_tf: Timeframe,
) -> HashMap<DateTime<Utc>, Vec<&OHLCV>> {
    let mut groups: HashMap<DateTime<Utc>, Vec<&OHLCV>> = HashMap::new();

    for candle in candles {
        let period_start = align_to_timeframe(candle.timestamp, target_tf);
        groups.entry(period_start).or_default().push(candle);
    }

    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;

    fn create_test_candle(timestamp: DateTime<Utc>, price: Decimal) -> OHLCV {
        OHLCV {
            timestamp,
            open: price,
            high: price + Decimal::from(10),
            low: price - Decimal::from(10),
            close: price + Decimal::from(5),
            volume: Decimal::from(100),
            trades: 50,
            closed: true,
        }
    }

    #[test]
    fn test_align_to_timeframe() {
        use chrono::TimeZone;

        let ts = Utc.with_ymd_and_hms(2024, 1, 1, 10, 37, 45).unwrap();

        // M1 should align to 10:37:00
        let m1_aligned = align_to_timeframe(ts, Timeframe::M1);
        assert_eq!(m1_aligned.minute(), 37);
        assert_eq!(m1_aligned.second(), 0);

        // M5 should align to 10:35:00
        let m5_aligned = align_to_timeframe(ts, Timeframe::M5);
        assert_eq!(m5_aligned.minute(), 35);

        // H1 should align to 10:00:00
        let h1_aligned = align_to_timeframe(ts, Timeframe::H1);
        assert_eq!(h1_aligned.hour(), 10);
        assert_eq!(h1_aligned.minute(), 0);

        // H4 should align to 08:00:00 (4-hour boundaries: 0, 4, 8, 12, 16, 20)
        let h4_aligned = align_to_timeframe(ts, Timeframe::H4);
        assert_eq!(h4_aligned.hour(), 8);

        // MO1 should align to first day of month at midnight
        let mo1_aligned = align_to_timeframe(ts, Timeframe::MO1);
        assert_eq!(mo1_aligned.day(), 1);
        assert_eq!(mo1_aligned.hour(), 0);
        assert_eq!(mo1_aligned.minute(), 0);
    }

    #[test]
    fn test_resample_candles() {
        use chrono::TimeZone;

        let base_ts = Utc.with_ymd_and_hms(2024, 1, 1, 10, 0, 0).unwrap();

        let candles: Vec<OHLCV> = (0..5)
            .map(|i| {
                let ts = base_ts + Duration::minutes(i);
                create_test_candle(ts, Decimal::from(50000 + i * 100))
            })
            .collect();

        let resampled = resample_candles(&candles, Timeframe::M5).unwrap();

        // Open should be from first candle
        assert_eq!(resampled.open, Decimal::from(50000));
        // Close should be from last candle
        assert_eq!(resampled.close, Decimal::from(50000 + 400 + 5));
        // Volume should be sum
        assert_eq!(resampled.volume, Decimal::from(500));
        // Trades should be sum
        assert_eq!(resampled.trades, 250);
    }

    #[test]
    fn test_aggregator_tick_processing() {
        use chrono::TimeZone;

        let mut aggregator = CandleAggregator::new("BTCUSDT".to_string(), 100);

        let base_ts = Utc.with_ymd_and_hms(2024, 1, 1, 10, 0, 0).unwrap();

        // Process ticks within the same minute
        for i in 0..30 {
            let ts = base_ts + Duration::seconds(i * 2);
            let price = Decimal::from(50000) + Decimal::from(i);
            let volume = Decimal::from(1);
            aggregator.process_tick(ts, price, volume);
        }

        // Should have partial candle but no completed candles yet
        let partial = aggregator.partial(Timeframe::M1).unwrap();
        assert!(!partial.closed);
        assert_eq!(partial.trades, 30);

        // Cross minute boundary
        let next_minute = base_ts + Duration::minutes(1);
        let completed = aggregator.process_tick(next_minute, Decimal::from(50100), Decimal::ONE);

        // Should have completed candles for M1 (and possibly M5 if aligned)
        assert!(!completed.is_empty());
        let (tf, candle) = &completed[0];
        assert_eq!(*tf, Timeframe::M1);
        assert!(candle.closed);
    }

    #[test]
    fn test_load_historical() {
        use chrono::TimeZone;

        let mut aggregator = CandleAggregator::new("BTCUSDT".to_string(), 100);

        let base_ts = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let candles: Vec<OHLCV> = (0..50)
            .map(|i| {
                let ts = base_ts + Duration::minutes(i);
                create_test_candle(ts, Decimal::from(50000 + i * 10))
            })
            .collect();

        aggregator.load_historical(Timeframe::M1, candles);

        assert_eq!(aggregator.candle_count(Timeframe::M1), 50);
        assert!(aggregator.has_min_candles(Timeframe::M1, 20));
    }

    #[test]
    fn test_group_by_period() {
        use chrono::TimeZone;

        let base_ts = Utc.with_ymd_and_hms(2024, 1, 1, 10, 0, 0).unwrap();

        let candles: Vec<OHLCV> = (0..10)
            .map(|i| {
                let ts = base_ts + Duration::minutes(i);
                create_test_candle(ts, Decimal::from(50000))
            })
            .collect();

        let groups = group_by_period(&candles, Timeframe::M5);

        // 10 M1 candles should form 2 M5 groups (0-4, 5-9)
        assert_eq!(groups.len(), 2);
    }
}
