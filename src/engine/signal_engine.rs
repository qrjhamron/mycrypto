//! Signal engine orchestration (technical -> sentiment -> confluence -> risk).

use rust_decimal::Decimal;

use crate::engine::confluence::{merge_signals, ConfluenceSignal};
use crate::engine::risk::{evaluate_risk, RiskAssessment};
use crate::engine::sentiment::{evaluate_sentiment, SentimentTracker};
use crate::engine::technical::evaluate_technical;
use crate::error::{MycryptoError, Result};
use crate::state::{
    AnalysisType, AppState, ConfidenceBreakdown, ReasonEntry, Signal, SignalAction,
    SignalDirection, Timeframe,
};

/// Complete pipeline output for a single pair.
#[derive(Debug, Clone)]
pub struct PipelineOutcome {
    pub pair: String,
    pub confluence: ConfluenceSignal,
    pub risk: RiskAssessment,
    pub signal: Signal,
}

/// Parses configured timeframe label into a known [`Timeframe`].
///
/// Falls back to [`Timeframe::M5`] when parsing fails.
pub fn timeframe_from_config(label: &str) -> Timeframe {
    Timeframe::from_binance_interval(label).unwrap_or(Timeframe::M5)
}

/// Run full pipeline for one pair using snapshot state.
#[must_use = "handle pipeline errors and skipped outcomes"]
pub fn run_pipeline_for_pair(
    state: &AppState,
    pair: &str,
    tracker: &mut SentimentTracker,
) -> Result<Option<PipelineOutcome>> {
    let timeframe = timeframe_from_config(&state.config.engine.timeframe);
    let candles = match state.get_candles(pair, timeframe) {
        Some(v) => v,
        None => return Ok(None),
    };
    let technical = match evaluate_technical(pair, candles) {
        Some(v) => v,
        None => return Ok(None),
    };
    let sentiment = evaluate_sentiment(state, pair, tracker);
    let confluence = merge_signals(
        pair,
        &technical,
        &sentiment,
        &state.config.engine.weights,
        state.config.engine.min_confidence,
    );

    let price = state
        .get_ticker(pair)
        .map(|t| t.price)
        .or_else(|| candles.latest().map(|c| c.close))
        .ok_or_else(|| MycryptoError::SignalGeneration {
            pair: pair.to_string(),
            reason: "missing market price".to_string(),
        })?;

    let risk = evaluate_risk(state, &confluence, price);
    let signal = build_state_signal(state, pair, price, &confluence, &risk);

    Ok(Some(PipelineOutcome {
        pair: pair.to_string(),
        confluence,
        risk,
        signal,
    }))
}

fn build_state_signal(
    state: &AppState,
    pair: &str,
    price: Decimal,
    confluence: &ConfluenceSignal,
    risk: &RiskAssessment,
) -> Signal {
    let breakdown = ConfidenceBreakdown {
        trend: ((confluence.composite_score * 30.0).round() as u8).min(30),
        momentum: ((confluence.composite_score * 25.0).round() as u8).min(25),
        volume: ((confluence.composite_score * 20.0).round() as u8).min(20),
        sentiment: ((confluence.composite_score * 15.0).round() as u8).min(15),
        macro_score: ((confluence.composite_score * 10.0).round() as u8).min(10),
    };

    let mut builder = Signal::builder(pair.to_string())
        .direction(match confluence.direction {
            SignalDirection::Long => SignalDirection::Long,
            SignalDirection::Short => SignalDirection::Short,
            SignalDirection::Wait => SignalDirection::Wait,
        })
        .confidence(((confluence.composite_score * 100.0).round() as u8).min(100))
        .confidence_breakdown(breakdown)
        .entry_price(price)
        .stop_loss(calc_stop_loss(price, confluence.direction))
        .take_profit(calc_take_profit(price, confluence.direction))
        .add_reason(ReasonEntry::new(
            AnalysisType::Trend,
            "Confluence score",
            (confluence.composite_score * 100.0).round() as i8,
            format!(
                "score={:.2}, agreed={}, disagreed={}",
                confluence.composite_score,
                confluence.agreed.join(","),
                confluence.disagreed.join(",")
            ),
        ))
        .add_reason(ReasonEntry::new(
            AnalysisType::Levels,
            "Risk assessment",
            if risk.approved { 20 } else { -40 },
            risk.rejection_reason
                .clone()
                .unwrap_or_else(|| format!("approved, suggested size {}", risk.suggested_size)),
        ));

    if !confluence.actionable {
        builder = builder.action(SignalAction::Watch);
    } else if !risk.approved {
        builder = builder.action(SignalAction::Skip).skip_reason(
            risk.rejection_reason
                .clone()
                .unwrap_or_else(|| "blocked by risk policy".to_string()),
        );
    } else {
        builder = builder.action(SignalAction::Execute);
    }

    if confluence.actionable
        && risk.approved
        && ((confluence.composite_score * 100.0) as u8) < state.config.agent.min_confidence
    {
        builder = builder.action(SignalAction::Watch);
    }

    builder.build()
}

fn calc_stop_loss(price: Decimal, direction: SignalDirection) -> Decimal {
    let pct = Decimal::new(15, 3); // 1.5%
    match direction {
        SignalDirection::Long => price * (Decimal::ONE - pct),
        SignalDirection::Short => price * (Decimal::ONE + pct),
        SignalDirection::Wait => price,
    }
}

fn calc_take_profit(price: Decimal, direction: SignalDirection) -> Decimal {
    let pct = Decimal::new(3, 2); // 3.0%
    match direction {
        SignalDirection::Long => price * (Decimal::ONE + pct),
        SignalDirection::Short => price * (Decimal::ONE - pct),
        SignalDirection::Wait => price,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Timeframe, OHLCV};
    use chrono::Utc;

    #[test]
    fn test_pipeline_builds_state_signal_from_stage_outputs() {
        let mut state = AppState::new(crate::config::Config::default());
        let pair = state.config.pairs.watchlist[0].clone();
        if let Some(market) = state.market_data.get_mut(&pair) {
            if let Some(buffer) = market.get_candles_mut(Timeframe::M5) {
                for i in 0..260 {
                    let p = Decimal::from(100 + i);
                    buffer.push(OHLCV {
                        timestamp: Utc::now(),
                        open: p - Decimal::ONE,
                        high: p + Decimal::from(2),
                        low: p - Decimal::from(2),
                        close: p,
                        volume: Decimal::from(100),
                        trades: 10,
                        closed: true,
                    });
                }
            }
        }
        if let Some(ticker) = state.market_data.get_mut(&pair).map(|m| &mut m.ticker) {
            ticker.price = Decimal::from(360);
        }
        let mut tracker = SentimentTracker::default();
        let out = run_pipeline_for_pair(&state, &pair, &mut tracker)
            .unwrap()
            .unwrap();
        assert_eq!(out.signal.pair, pair);
        assert!(out.signal.confidence <= 100);
    }
}
