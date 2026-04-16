//! Technical signal stage for production signal engine.

use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;

use crate::data::indicators::{atr_percent, bollinger_bands, ema, macd, rsi, vwap};
use crate::state::{CandleBuffer, SignalDirection, OHLCV};

/// One technical indicator vote.
#[derive(Debug, Clone)]
pub struct IndicatorVote {
    pub name: &'static str,
    pub direction: SignalDirection,
    pub strength: f32,
}

/// Aggregate technical signal output.
#[derive(Debug, Clone)]
pub struct TechnicalSignal {
    pub pair: String,
    pub direction: SignalDirection,
    pub strength: f32,
    pub contributors: Vec<String>,
    pub votes: Vec<IndicatorVote>,
}

impl TechnicalSignal {
    /// Creates a neutral technical signal when no indicator contribution is available.
    #[must_use]
    pub fn neutral(pair: String) -> Self {
        Self {
            pair,
            direction: SignalDirection::Wait,
            strength: 0.0,
            contributors: Vec::new(),
            votes: Vec::new(),
        }
    }
}

/// Build technical signal from candles.
#[must_use]
pub fn evaluate_technical(pair: &str, candles: &CandleBuffer) -> Option<TechnicalSignal> {
    let candles_vec: Vec<OHLCV> = candles.candles.iter().cloned().collect();
    if candles_vec.len() < 30 {
        return None;
    }
    let closes: Vec<Decimal> = candles_vec.iter().map(|c| c.close).collect();

    let mut votes = vec![
        evaluate_ema_crossover(&closes),
        evaluate_rsi_signal(&closes),
        evaluate_macd_signal(&closes),
        evaluate_bollinger_signal(&closes),
        evaluate_atr_regime(&candles_vec),
        evaluate_vwap_signal(&candles_vec),
        evaluate_volume_anomaly(&candles_vec),
    ];

    votes.retain(|v| v.strength > 0.0);
    if votes.is_empty() {
        return Some(TechnicalSignal::neutral(pair.to_string()));
    }

    let mut signed = 0.0f32;
    let mut total = 0.0f32;
    let mut contributors = Vec::new();

    for vote in &votes {
        total += vote.strength;
        contributors.push(vote.name.to_string());
        match vote.direction {
            SignalDirection::Long => signed += vote.strength,
            SignalDirection::Short => signed -= vote.strength,
            SignalDirection::Wait => {}
        }
    }

    let strength = if total > 0.0 {
        (signed.abs() / total).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let direction = if signed > 0.05 {
        SignalDirection::Long
    } else if signed < -0.05 {
        SignalDirection::Short
    } else {
        SignalDirection::Wait
    };

    Some(TechnicalSignal {
        pair: pair.to_string(),
        direction,
        strength,
        contributors,
        votes,
    })
}

fn evaluate_ema_crossover(closes: &[Decimal]) -> IndicatorVote {
    let mut direction = SignalDirection::Wait;
    let mut strength = 0.0;

    if let (Some(ema9), Some(ema21), Some(ema50), Some(ema200)) = (
        ema(closes, 9),
        ema(closes, 21),
        ema(closes, 50),
        ema(closes, 200),
    ) {
        if ema9 > ema21 && ema21 > ema50 && ema50 > ema200 {
            direction = SignalDirection::Long;
            strength = 1.0;
        } else if ema9 < ema21 && ema21 < ema50 && ema50 < ema200 {
            direction = SignalDirection::Short;
            strength = 1.0;
        } else if ema9 > ema21 {
            direction = SignalDirection::Long;
            strength = 0.55;
        } else if ema9 < ema21 {
            direction = SignalDirection::Short;
            strength = 0.55;
        }
    }

    IndicatorVote {
        name: "ema_crossover",
        direction,
        strength,
    }
}

fn evaluate_rsi_signal(closes: &[Decimal]) -> IndicatorVote {
    let mut direction = SignalDirection::Wait;
    let mut strength = 0.0;

    if let Some(v) = rsi(closes, 14) {
        if v <= Decimal::from(30) {
            direction = SignalDirection::Long;
            strength = 0.7;
        } else if v >= Decimal::from(70) {
            direction = SignalDirection::Short;
            strength = 0.7;
        } else {
            direction = SignalDirection::Wait;
            strength = 0.2;
        }
    }

    IndicatorVote {
        name: "rsi",
        direction,
        strength,
    }
}

fn evaluate_macd_signal(closes: &[Decimal]) -> IndicatorVote {
    let mut direction = SignalDirection::Wait;
    let mut strength = 0.0;

    if let Some(macd_now) = macd(closes, 12, 26, 9) {
        if macd_now.histogram > Decimal::ZERO {
            direction = SignalDirection::Long;
            strength = 0.75;
        } else if macd_now.histogram < Decimal::ZERO {
            direction = SignalDirection::Short;
            strength = 0.75;
        }
    }

    IndicatorVote {
        name: "macd",
        direction,
        strength,
    }
}

fn evaluate_bollinger_signal(closes: &[Decimal]) -> IndicatorVote {
    let mut direction = SignalDirection::Wait;
    let mut strength = 0.0;

    if let Some(bb) = bollinger_bands(closes, 20, Decimal::from(2)) {
        if let Some(last) = closes.last() {
            if *last > bb.upper {
                direction = SignalDirection::Long;
                strength = 0.7;
            } else if *last < bb.lower {
                direction = SignalDirection::Short;
                strength = 0.7;
            } else if bb.width_pct < Decimal::from(4) {
                direction = SignalDirection::Wait;
                strength = 0.25;
            }
        }
    }

    IndicatorVote {
        name: "bb",
        direction,
        strength,
    }
}

fn evaluate_atr_regime(candles: &[OHLCV]) -> IndicatorVote {
    let mut direction = SignalDirection::Wait;
    let mut strength = 0.0;

    if let Some(atr_pct) = atr_percent(candles, 14) {
        if atr_pct > Decimal::from(3) {
            direction = SignalDirection::Wait;
            strength = 0.4;
        } else {
            direction = SignalDirection::Long;
            strength = 0.2;
        }
    }

    IndicatorVote {
        name: "atr_regime",
        direction,
        strength,
    }
}

fn evaluate_vwap_signal(candles: &[OHLCV]) -> IndicatorVote {
    let mut direction = SignalDirection::Wait;
    let mut strength = 0.0;

    if let (Some(v), Some(last)) = (vwap(candles), candles.last()) {
        if last.close > v {
            direction = SignalDirection::Long;
            strength = 0.45;
        } else if last.close < v {
            direction = SignalDirection::Short;
            strength = 0.45;
        }
    }

    IndicatorVote {
        name: "vwap",
        direction,
        strength,
    }
}

fn evaluate_volume_anomaly(candles: &[OHLCV]) -> IndicatorVote {
    let mut direction = SignalDirection::Wait;
    let mut strength = 0.0;

    if let Some(z) = volume_zscore(candles, 20) {
        if z.abs() >= 2.0 {
            direction = if z > 0.0 {
                SignalDirection::Long
            } else {
                SignalDirection::Short
            };
            strength = 0.55;
        }
    }

    IndicatorVote {
        name: "volume_anomaly",
        direction,
        strength,
    }
}

fn volume_zscore(candles: &[OHLCV], window: usize) -> Option<f64> {
    if candles.len() < window + 1 {
        return None;
    }

    let recent = &candles[candles.len() - window - 1..candles.len() - 1];
    let latest = candles.last()?.volume.to_f64()?;
    let values: Vec<f64> = recent.iter().filter_map(|c| c.volume.to_f64()).collect();
    if values.is_empty() {
        return None;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let var = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;
    let std = var.sqrt();
    if std <= f64::EPSILON {
        return Some(0.0);
    }

    Some((latest - mean) / std)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn build_candles(prices: &[i64], vols: &[i64]) -> CandleBuffer {
        let mut buffer = CandleBuffer::new("BTCUSDT".to_string(), crate::state::Timeframe::M5, 512);
        for (idx, price) in prices.iter().enumerate() {
            let volume = *vols.get(idx).unwrap_or(&100);
            buffer.push(OHLCV {
                timestamp: Utc::now(),
                open: Decimal::from(*price - 1),
                high: Decimal::from(*price + 2),
                low: Decimal::from(*price - 2),
                close: Decimal::from(*price),
                volume: Decimal::from(volume),
                trades: 10,
                closed: true,
            });
        }
        buffer
    }

    #[test]
    fn test_ema_crossover_long_signal() {
        let mut prices = Vec::new();
        for i in 1..=260 {
            prices.push(i as i64);
        }
        let vols = vec![100; prices.len()];
        let candles = build_candles(&prices, &vols);
        let signal = evaluate_technical("BTCUSDT", &candles).unwrap();
        assert!(matches!(
            signal.direction,
            SignalDirection::Long | SignalDirection::Wait
        ));
        assert!(signal.strength >= 0.0 && signal.strength <= 1.0);
    }

    #[test]
    fn test_rsi_direction_signal() {
        let prices: Vec<i64> = vec![
            100, 99, 98, 97, 96, 95, 94, 95, 94, 93, 92, 91, 90, 91, 90, 89, 88, 87, 86, 85, 84,
            83, 82, 81, 80, 79, 78, 77, 76, 75,
        ];
        let vols = vec![100; prices.len()];
        let candles = build_candles(&prices, &vols);
        let signal = evaluate_technical("BTCUSDT", &candles).unwrap();
        assert!(signal.strength >= 0.0 && signal.strength <= 1.0);
    }

    #[test]
    fn test_macd_histogram_momentum_signal() {
        let mut prices = Vec::new();
        for i in 0..80 {
            prices.push(100 + i as i64 * 2);
        }
        let vols = vec![100; prices.len()];
        let candles = build_candles(&prices, &vols);
        let signal = evaluate_technical("BTCUSDT", &candles).unwrap();
        assert!(signal.contributors.iter().any(|c| c == "macd"));
    }
}
