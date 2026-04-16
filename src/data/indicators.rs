//! Technical indicator calculations.
//!
//! This module provides common technical analysis indicators used for
//! signal generation:
//!
//! - Moving Averages (SMA, EMA)
//! - Momentum indicators (RSI, MACD)
//! - Volatility indicators (Bollinger Bands, ATR)
//! - Volume indicators (OBV, VWAP)
//!
//! All calculations use `Decimal` for precision in financial math.
//! Indicators work with slices of prices/candles and return computed values.

use rust_decimal::Decimal;

use crate::state::OHLCV;

fn dec(n: i64) -> Decimal {
    Decimal::from(n)
}

/// Simple Moving Average (SMA).
///
/// Calculates the arithmetic mean of the last `period` values.
/// Returns None if there aren't enough values.
pub fn sma(values: &[Decimal], period: usize) -> Option<Decimal> {
    if values.len() < period || period == 0 {
        return None;
    }

    let sum: Decimal = values.iter().rev().take(period).sum();
    Some(sum / Decimal::from(period))
}

/// Exponential Moving Average (EMA).
///
/// Uses the standard smoothing formula: EMA = price * k + EMA_prev * (1 - k)
/// where k = 2 / (period + 1)
pub fn ema(values: &[Decimal], period: usize) -> Option<Decimal> {
    if values.len() < period || period == 0 {
        return None;
    }

    let k = Decimal::from(2) / Decimal::from(period + 1);
    let one_minus_k = Decimal::ONE - k;

    // Start with SMA for the first period values
    let initial_sma = sma(&values[..period], period)?;

    // Apply EMA formula to remaining values
    let mut ema_value = initial_sma;
    for value in values.iter().skip(period) {
        ema_value = *value * k + ema_value * one_minus_k;
    }

    Some(ema_value)
}

/// Calculates all EMAs in one pass, returning the final value plus the full series.
pub fn ema_series(values: &[Decimal], period: usize) -> Option<Vec<Decimal>> {
    if values.len() < period || period == 0 {
        return None;
    }

    let k = Decimal::from(2) / Decimal::from(period + 1);
    let one_minus_k = Decimal::ONE - k;

    let mut result = Vec::with_capacity(values.len() - period + 1);

    // Start with SMA for the first period values
    let initial_sma = sma(&values[..period], period)?;
    result.push(initial_sma);

    // Apply EMA formula to remaining values
    let mut ema_value = initial_sma;
    for value in values.iter().skip(period) {
        ema_value = *value * k + ema_value * one_minus_k;
        result.push(ema_value);
    }

    Some(result)
}

/// Relative Strength Index (RSI).
///
/// Measures momentum by comparing average gains vs average losses over a period.
/// Returns a value between 0 and 100.
///
/// Standard period is 14.
pub fn rsi(closes: &[Decimal], period: usize) -> Option<Decimal> {
    if closes.len() < period + 1 || period == 0 {
        return None;
    }

    let mut gains = Vec::new();
    let mut losses = Vec::new();

    // Calculate price changes
    for window in closes.windows(2) {
        let change = window[1] - window[0];
        if change > Decimal::ZERO {
            gains.push(change);
            losses.push(Decimal::ZERO);
        } else {
            gains.push(Decimal::ZERO);
            losses.push(change.abs());
        }
    }

    if gains.len() < period {
        return None;
    }

    // Initial average gain/loss (SMA)
    let avg_gain: Decimal = gains.iter().take(period).sum::<Decimal>() / Decimal::from(period);
    let avg_loss: Decimal = losses.iter().take(period).sum::<Decimal>() / Decimal::from(period);

    // Apply smoothed average for remaining values
    let mut smoothed_gain = avg_gain;
    let mut smoothed_loss = avg_loss;

    for i in period..gains.len() {
        smoothed_gain =
            (smoothed_gain * Decimal::from(period - 1) + gains[i]) / Decimal::from(period);
        smoothed_loss =
            (smoothed_loss * Decimal::from(period - 1) + losses[i]) / Decimal::from(period);
    }

    // Calculate RSI
    if smoothed_loss == Decimal::ZERO {
        return Some(dec(100));
    }

    let rs = smoothed_gain / smoothed_loss;
    let rsi = dec(100) - (dec(100) / (Decimal::ONE + rs));

    Some(rsi)
}

/// RSI interpretation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RsiZone {
    /// RSI > 70 - potentially overbought
    Overbought,
    /// RSI < 30 - potentially oversold
    Oversold,
    /// RSI between 30-70 - neutral
    Neutral,
}

impl RsiZone {
    /// Determines the RSI zone from a value.
    pub fn from_value(rsi: Decimal) -> Self {
        if rsi >= dec(70) {
            RsiZone::Overbought
        } else if rsi <= dec(30) {
            RsiZone::Oversold
        } else {
            RsiZone::Neutral
        }
    }
}

/// MACD result containing line values.
#[derive(Debug, Clone)]
pub struct MacdResult {
    /// MACD line (fast EMA - slow EMA)
    pub macd_line: Decimal,
    /// Signal line (EMA of MACD line)
    pub signal_line: Decimal,
    /// Histogram (MACD line - signal line)
    pub histogram: Decimal,
}

/// Moving Average Convergence Divergence (MACD).
///
/// Standard parameters: fast=12, slow=26, signal=9
pub fn macd(
    closes: &[Decimal],
    fast_period: usize,
    slow_period: usize,
    signal_period: usize,
) -> Option<MacdResult> {
    if closes.len() < slow_period + signal_period {
        return None;
    }

    // Calculate fast and slow EMAs
    let fast_ema_series = ema_series(closes, fast_period)?;
    let slow_ema_series = ema_series(closes, slow_period)?;

    // MACD line = fast EMA - slow EMA
    // We need to align the series (slow EMA starts later)
    let macd_values: Vec<Decimal> = fast_ema_series
        .iter()
        .skip(slow_period - fast_period)
        .zip(slow_ema_series.iter())
        .map(|(fast, slow)| *fast - *slow)
        .collect();

    if macd_values.len() < signal_period {
        return None;
    }

    // Signal line = EMA of MACD line
    let signal_ema = ema(&macd_values, signal_period)?;

    let macd_line = *macd_values.last()?;
    let histogram = macd_line - signal_ema;

    Some(MacdResult {
        macd_line,
        signal_line: signal_ema,
        histogram,
    })
}

/// MACD signal interpretation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacdSignal {
    /// MACD line crosses above signal line
    BullishCross,
    /// MACD line crosses below signal line
    BearishCross,
    /// MACD above signal, histogram positive
    Bullish,
    /// MACD below signal, histogram negative
    Bearish,
}

impl MacdResult {
    /// Interprets the MACD result.
    pub fn signal(&self) -> MacdSignal {
        if self.histogram > Decimal::ZERO {
            MacdSignal::Bullish
        } else {
            MacdSignal::Bearish
        }
    }
}

/// Bollinger Bands result.
#[derive(Debug, Clone)]
pub struct BollingerBands {
    /// Middle band (SMA)
    pub middle: Decimal,
    /// Upper band (middle + k * stddev)
    pub upper: Decimal,
    /// Lower band (middle - k * stddev)
    pub lower: Decimal,
    /// Band width as percentage
    pub width_pct: Decimal,
}

/// Calculates Bollinger Bands.
///
/// Standard parameters: period=20, std_dev=2.0
pub fn bollinger_bands(
    closes: &[Decimal],
    period: usize,
    std_dev_multiplier: Decimal,
) -> Option<BollingerBands> {
    if closes.len() < period {
        return None;
    }

    let middle = sma(closes, period)?;
    let std_dev = standard_deviation(closes, period)?;

    let band_width = std_dev * std_dev_multiplier;
    let upper = middle + band_width;
    let lower = middle - band_width;

    let width_pct = if middle > Decimal::ZERO {
        (upper - lower) / middle * dec(100)
    } else {
        Decimal::ZERO
    };

    Some(BollingerBands {
        middle,
        upper,
        lower,
        width_pct,
    })
}

/// Standard deviation calculation.
pub fn standard_deviation(values: &[Decimal], period: usize) -> Option<Decimal> {
    if values.len() < period || period == 0 {
        return None;
    }

    let recent: Vec<_> = values.iter().rev().take(period).cloned().collect();
    let mean = sma(values, period)?;

    let variance: Decimal = recent
        .iter()
        .map(|v| {
            let diff = *v - mean;
            diff * diff
        })
        .sum::<Decimal>()
        / Decimal::from(period);

    // Approximate square root using Newton's method
    Some(decimal_sqrt(variance))
}

/// Average True Range (ATR).
///
/// Measures volatility based on the true range of price movement.
/// Standard period is 14.
pub fn atr(candles: &[OHLCV], period: usize) -> Option<Decimal> {
    if candles.len() < period + 1 {
        return None;
    }

    let mut true_ranges = Vec::with_capacity(candles.len() - 1);

    for i in 1..candles.len() {
        let high = candles[i].high;
        let low = candles[i].low;
        let prev_close = candles[i - 1].close;

        // True range is the greatest of:
        // 1. Current high - current low
        // 2. Abs(current high - previous close)
        // 3. Abs(current low - previous close)
        let tr = (high - low)
            .max((high - prev_close).abs())
            .max((low - prev_close).abs());

        true_ranges.push(tr);
    }

    // Use smoothed average (like RSI)
    if true_ranges.len() < period {
        return None;
    }

    let initial_atr: Decimal =
        true_ranges.iter().take(period).sum::<Decimal>() / Decimal::from(period);

    let mut smoothed_atr = initial_atr;
    for tr in true_ranges.iter().skip(period) {
        smoothed_atr = (smoothed_atr * Decimal::from(period - 1) + *tr) / Decimal::from(period);
    }

    Some(smoothed_atr)
}

/// ATR as percentage of price.
pub fn atr_percent(candles: &[OHLCV], period: usize) -> Option<Decimal> {
    let atr_value = atr(candles, period)?;
    let current_price = candles.last()?.close;

    if current_price > Decimal::ZERO {
        Some(atr_value / current_price * dec(100))
    } else {
        None
    }
}

/// On-Balance Volume (OBV).
///
/// Cumulative volume indicator that adds volume on up days and subtracts on down days.
pub fn obv(candles: &[OHLCV]) -> Option<Decimal> {
    if candles.len() < 2 {
        return None;
    }

    let mut obv_value = Decimal::ZERO;

    for i in 1..candles.len() {
        if candles[i].close > candles[i - 1].close {
            obv_value += candles[i].volume;
        } else if candles[i].close < candles[i - 1].close {
            obv_value -= candles[i].volume;
        }
        // Volume unchanged if close prices are equal
    }

    Some(obv_value)
}

/// Volume Weighted Average Price (VWAP).
///
/// Calculates the average price weighted by volume.
pub fn vwap(candles: &[OHLCV]) -> Option<Decimal> {
    if candles.is_empty() {
        return None;
    }

    let mut cumulative_tp_vol = Decimal::ZERO;
    let mut cumulative_vol = Decimal::ZERO;

    for candle in candles {
        // Typical price = (high + low + close) / 3
        let typical_price = (candle.high + candle.low + candle.close) / Decimal::from(3);
        cumulative_tp_vol += typical_price * candle.volume;
        cumulative_vol += candle.volume;
    }

    if cumulative_vol > Decimal::ZERO {
        Some(cumulative_tp_vol / cumulative_vol)
    } else {
        None
    }
}

/// Stochastic Oscillator result.
#[derive(Debug, Clone)]
pub struct StochasticResult {
    /// %K line (fast stochastic)
    pub k: Decimal,
    /// %D line (slow stochastic, SMA of %K)
    pub d: Decimal,
}

/// Stochastic Oscillator.
///
/// Standard parameters: k_period=14, d_period=3
pub fn stochastic(candles: &[OHLCV], k_period: usize, d_period: usize) -> Option<StochasticResult> {
    if candles.len() < k_period + d_period {
        return None;
    }

    let mut k_values = Vec::new();

    for i in k_period..=candles.len() {
        let window = &candles[i - k_period..i];

        let highest_high = window.iter().map(|c| c.high).max()?;
        let lowest_low = window.iter().map(|c| c.low).min()?;
        let current_close = window.last()?.close;

        let range = highest_high - lowest_low;
        let k = if range > Decimal::ZERO {
            (current_close - lowest_low) / range * dec(100)
        } else {
            dec(50) // Neutral if no range
        };

        k_values.push(k);
    }

    // %D is SMA of %K
    let d = sma(&k_values, d_period)?;
    let k = *k_values.last()?;

    Some(StochasticResult { k, d })
}

/// Price momentum (rate of change).
///
/// Returns the percentage change over the given period.
pub fn momentum(closes: &[Decimal], period: usize) -> Option<Decimal> {
    if closes.len() < period + 1 {
        return None;
    }

    let current = closes.last()?;
    let past = closes.get(closes.len() - 1 - period)?;

    if *past != Decimal::ZERO {
        Some((*current - *past) / *past * dec(100))
    } else {
        None
    }
}

/// Calculates support and resistance levels from recent highs/lows.
#[derive(Debug, Clone)]
pub struct SupportResistance {
    /// Nearest resistance level above current price.
    pub resistance: Option<Decimal>,
    /// Nearest support level below current price.
    pub support: Option<Decimal>,
}

/// Finds basic support and resistance levels.
pub fn support_resistance(candles: &[OHLCV], current_price: Decimal) -> SupportResistance {
    let highs: Vec<Decimal> = candles.iter().map(|c| c.high).collect();
    let lows: Vec<Decimal> = candles.iter().map(|c| c.low).collect();

    // Find recent swing highs as resistance
    let resistance = highs.iter().filter(|h| **h > current_price).min().cloned();

    // Find recent swing lows as support
    let support = lows.iter().filter(|l| **l < current_price).max().cloned();

    SupportResistance {
        resistance,
        support,
    }
}

/// Approximate square root using Newton's method.
fn decimal_sqrt(n: Decimal) -> Decimal {
    if n <= Decimal::ZERO {
        return Decimal::ZERO;
    }

    let mut guess = n / Decimal::from(2);
    let tolerance = Decimal::new(1, 8); // 0.00000001

    for _ in 0..50 {
        let new_guess = (guess + n / guess) / Decimal::from(2);
        if (new_guess - guess).abs() < tolerance {
            return new_guess;
        }
        guess = new_guess;
    }

    guess
}

/// Comprehensive indicator snapshot for a trading pair.
#[derive(Debug, Clone)]
pub struct IndicatorSnapshot {
    /// Current RSI value (0-100)
    pub rsi: Option<Decimal>,
    /// RSI zone interpretation
    pub rsi_zone: Option<RsiZone>,
    /// MACD result
    pub macd: Option<MacdResult>,
    /// Bollinger Bands
    pub bollinger: Option<BollingerBands>,
    /// ATR (absolute)
    pub atr: Option<Decimal>,
    /// ATR as percentage of price
    pub atr_pct: Option<Decimal>,
    /// Short-term EMA (e.g., 9-period)
    pub ema_short: Option<Decimal>,
    /// Medium-term EMA (e.g., 21-period)
    pub ema_medium: Option<Decimal>,
    /// Long-term EMA (e.g., 50-period)
    pub ema_long: Option<Decimal>,
    /// Price momentum
    pub momentum: Option<Decimal>,
    /// VWAP
    pub vwap: Option<Decimal>,
}

impl IndicatorSnapshot {
    /// Calculates all indicators from candles.
    pub fn calculate(candles: &[OHLCV]) -> Self {
        let closes: Vec<Decimal> = candles.iter().map(|c| c.close).collect();

        let rsi_value = rsi(&closes, 14);
        let rsi_zone = rsi_value.map(RsiZone::from_value);

        Self {
            rsi: rsi_value,
            rsi_zone,
            macd: macd(&closes, 12, 26, 9),
            bollinger: bollinger_bands(&closes, 20, dec(2)),
            atr: atr(candles, 14),
            atr_pct: atr_percent(candles, 14),
            ema_short: ema(&closes, 9),
            ema_medium: ema(&closes, 21),
            ema_long: ema(&closes, 50),
            momentum: momentum(&closes, 10),
            vwap: vwap(candles),
        }
    }

    /// Returns true if EMAs are in bullish alignment (short > medium > long).
    pub fn ema_bullish_alignment(&self) -> bool {
        match (self.ema_short, self.ema_medium, self.ema_long) {
            (Some(short), Some(medium), Some(long)) => short > medium && medium > long,
            _ => false,
        }
    }

    /// Returns true if EMAs are in bearish alignment (short < medium < long).
    pub fn ema_bearish_alignment(&self) -> bool {
        match (self.ema_short, self.ema_medium, self.ema_long) {
            (Some(short), Some(medium), Some(long)) => short < medium && medium < long,
            _ => false,
        }
    }

    /// Returns true if price is near Bollinger upper band (potential resistance).
    pub fn near_bollinger_upper(&self, price: Decimal, threshold_pct: Decimal) -> bool {
        if let Some(ref bb) = self.bollinger {
            let distance = (bb.upper - price).abs();
            let threshold = price * threshold_pct / dec(100);
            distance <= threshold
        } else {
            false
        }
    }

    /// Returns true if price is near Bollinger lower band (potential support).
    pub fn near_bollinger_lower(&self, price: Decimal, threshold_pct: Decimal) -> bool {
        if let Some(ref bb) = self.bollinger {
            let distance = (price - bb.lower).abs();
            let threshold = price * threshold_pct / dec(100);
            distance <= threshold
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn sample_closes() -> Vec<Decimal> {
        vec![
            dec(100),
            dec(102),
            dec(101),
            dec(103),
            dec(105),
            dec(104),
            dec(106),
            dec(108),
            dec(107),
            dec(109),
            dec(111),
            dec(110),
            dec(112),
            dec(114),
            dec(113),
            dec(115),
            dec(117),
            dec(116),
            dec(118),
            dec(120),
        ]
    }

    fn sample_candles() -> Vec<OHLCV> {
        sample_closes()
            .iter()
            .map(|close| OHLCV {
                timestamp: Utc::now(),
                open: *close - dec(1),
                high: *close + dec(2),
                low: *close - dec(2),
                close: *close,
                volume: dec(1000),
                trades: 100,
                closed: true,
            })
            .collect()
    }

    #[test]
    fn test_sma() {
        let values = sample_closes();
        let result = sma(&values, 5).unwrap();

        // SMA of last 5: (116 + 118 + 120 + 117 + 115) / 5 = 117.2
        // Wait, let me recalculate with actual last 5 values
        let last_5: Vec<_> = values.iter().rev().take(5).collect();
        let expected: Decimal = last_5.iter().map(|v| **v).sum::<Decimal>() / dec(5);

        assert!((result - expected).abs() < Decimal::new(1, 2)); // 0.01
    }

    #[test]
    fn test_sma_insufficient_data() {
        let values = vec![dec(100), dec(101)];
        assert!(sma(&values, 5).is_none());
    }

    #[test]
    fn test_ema() {
        let values = sample_closes();
        let result = ema(&values, 5).unwrap();

        // EMA should exist and be reasonable
        assert!(result > dec(100) && result < dec(130));
    }

    #[test]
    fn test_rsi() {
        let values = sample_closes();
        let result = rsi(&values, 14).unwrap();

        // RSI should be between 0 and 100
        assert!(result >= dec(0) && result <= dec(100));

        // With mostly up moves, RSI should be high
        assert!(result > dec(50));
    }

    #[test]
    fn test_rsi_zones() {
        assert_eq!(RsiZone::from_value(dec(75)), RsiZone::Overbought);
        assert_eq!(RsiZone::from_value(dec(25)), RsiZone::Oversold);
        assert_eq!(RsiZone::from_value(dec(50)), RsiZone::Neutral);
    }

    #[test]
    fn test_macd() {
        // Need more data for MACD
        let values: Vec<Decimal> = (0..50).map(|i| dec(100) + Decimal::from(i)).collect();

        let result = macd(&values, 12, 26, 9);
        assert!(result.is_some());

        let macd_result = result.unwrap();
        // In an uptrend, MACD should be positive
        assert!(macd_result.macd_line > Decimal::ZERO);
    }

    #[test]
    fn test_bollinger_bands() {
        let values = sample_closes();
        let result = bollinger_bands(&values, 10, dec(2)).unwrap();

        // Upper should be above middle, lower should be below
        assert!(result.upper > result.middle);
        assert!(result.lower < result.middle);
        assert!(result.width_pct > Decimal::ZERO);
    }

    #[test]
    fn test_atr() {
        let candles = sample_candles();
        let result = atr(&candles, 14).unwrap();

        // ATR should be positive
        assert!(result > Decimal::ZERO);
        // With our sample data, ATR should be around 4 (high-low range)
        assert!(result < dec(10));
    }

    #[test]
    fn test_vwap() {
        let candles = sample_candles();
        let result = vwap(&candles).unwrap();

        // VWAP should be in the range of our prices
        assert!(result > dec(100) && result < dec(125));
    }

    #[test]
    fn test_momentum() {
        let values = sample_closes();
        let result = momentum(&values, 10).unwrap();

        // With increasing prices, momentum should be positive
        assert!(result > Decimal::ZERO);
    }

    #[test]
    fn test_indicator_snapshot() {
        let candles: Vec<OHLCV> = (0..100)
            .map(|i| OHLCV {
                timestamp: Utc::now(),
                open: dec(100) + Decimal::from(i),
                high: dec(102) + Decimal::from(i),
                low: dec(98) + Decimal::from(i),
                close: dec(101) + Decimal::from(i),
                volume: dec(1000),
                trades: 100,
                closed: true,
            })
            .collect();

        let snapshot = IndicatorSnapshot::calculate(&candles);

        assert!(snapshot.rsi.is_some());
        assert!(snapshot.macd.is_some());
        assert!(snapshot.bollinger.is_some());
        assert!(snapshot.atr.is_some());
        assert!(snapshot.ema_short.is_some());
        assert!(snapshot.ema_medium.is_some());
        assert!(snapshot.ema_long.is_some());
    }

    #[test]
    fn test_decimal_sqrt() {
        let result = decimal_sqrt(dec(4));
        assert!((result - dec(2)).abs() < Decimal::new(1, 4)); // 0.0001

        let result = decimal_sqrt(dec(2));
        // sqrt(2) ≈ 1.4142
        let expected = Decimal::new(14142, 4); // 1.4142
        assert!((result - expected).abs() < Decimal::new(1, 3)); // 0.001
    }

    #[test]
    fn test_stochastic() {
        let candles = sample_candles();
        let result = stochastic(&candles, 14, 3);

        assert!(result.is_some());
        let stoch = result.unwrap();

        // %K and %D should be between 0 and 100
        assert!(stoch.k >= dec(0) && stoch.k <= dec(100));
        assert!(stoch.d >= dec(0) && stoch.d <= dec(100));
    }
}
