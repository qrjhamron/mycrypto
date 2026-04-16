//! Confluence stage: merge technical and sentiment votes.

use crate::config::EngineWeights;
use crate::engine::sentiment::SentimentSignal;
use crate::engine::technical::TechnicalSignal;
use crate::state::SignalDirection;

/// Final merged signal before risk checks.
#[derive(Debug, Clone)]
pub struct ConfluenceSignal {
    pub pair: String,
    pub direction: SignalDirection,
    pub composite_score: f32,
    pub agreed: Vec<String>,
    pub disagreed: Vec<String>,
    pub actionable: bool,
}

/// Merges technical and sentiment stages into a confluence decision.
#[must_use]
pub fn merge_signals(
    pair: &str,
    technical: &TechnicalSignal,
    sentiment: &SentimentSignal,
    weights: &EngineWeights,
    min_confidence: f32,
) -> ConfluenceSignal {
    let mut signed = 0.0f32;
    let mut total = 0.0f32;
    let mut agreed = Vec::with_capacity(technical.votes.len() + 1);
    let mut disagreed = Vec::with_capacity(technical.votes.len() + 1);

    let sentiment_weight = sanitize_f32(weights.sentiment);

    if !min_confidence.is_finite() {
        return ConfluenceSignal {
            pair: pair.to_string(),
            direction: SignalDirection::Wait,
            composite_score: 0.0,
            agreed,
            disagreed,
            actionable: false,
        };
    }

    for vote in &technical.votes {
        let weight = match vote.name {
            "ema_crossover" => weights.ema_crossover,
            "rsi" => weights.rsi,
            "macd" => weights.macd,
            "bb" => weights.bb,
            "atr_regime" => weights.atr_regime,
            "vwap" => weights.vwap,
            "volume_anomaly" => weights.volume_anomaly,
            _ => 0.0,
        };
        let weight = sanitize_f32(weight);
        let vote_strength = sanitize_f32(vote.strength).clamp(0.0, 1.0);
        if weight <= 0.0 {
            continue;
        }
        let contribution = (weight * vote_strength).clamp(0.0, 1.0);
        total += contribution;
        match vote.direction {
            SignalDirection::Long => {
                signed += contribution;
                agreed.push(vote.name.to_string());
            }
            SignalDirection::Short => {
                signed -= contribution;
                disagreed.push(vote.name.to_string());
            }
            SignalDirection::Wait => {}
        }
    }

    if sentiment_weight > 0.0 {
        let sentiment_strength = sanitize_f32(sentiment.score).abs().min(1.0);
        let contribution = (sentiment_weight * sentiment_strength).clamp(0.0, 1.0);
        total += contribution;
        match sentiment.direction {
            SignalDirection::Long => {
                signed += contribution;
                agreed.push("sentiment".to_string());
            }
            SignalDirection::Short => {
                signed -= contribution;
                disagreed.push("sentiment".to_string());
            }
            SignalDirection::Wait => {}
        }
    }

    let composite_score = if total > 0.0 && signed.is_finite() && total.is_finite() {
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

    ConfluenceSignal {
        pair: pair.to_string(),
        direction,
        composite_score: if composite_score.is_finite() {
            composite_score
        } else {
            0.0
        },
        agreed,
        disagreed,
        actionable: composite_score >= min_confidence.clamp(0.0, 1.0)
            && direction != SignalDirection::Wait,
    }
}

fn sanitize_f32(value: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::technical::IndicatorVote;

    #[test]
    fn test_confluence_weighted_vote_prefers_higher_weight_direction() {
        let technical = TechnicalSignal {
            pair: "BTCUSDT".to_string(),
            direction: SignalDirection::Long,
            strength: 0.8,
            contributors: vec!["ema_crossover".to_string(), "bb".to_string()],
            votes: vec![
                IndicatorVote {
                    name: "ema_crossover",
                    direction: SignalDirection::Long,
                    strength: 0.9,
                },
                IndicatorVote {
                    name: "bb",
                    direction: SignalDirection::Short,
                    strength: 0.4,
                },
            ],
        };

        let sentiment = SentimentSignal {
            pair: "BTCUSDT".to_string(),
            score: 0.20,
            delta_3tick: 0.05,
            direction: SignalDirection::Long,
        };

        let weights = EngineWeights::default();
        let result = merge_signals("BTCUSDT", &technical, &sentiment, &weights, 0.65);
        assert_eq!(result.direction, SignalDirection::Long);
        assert!(result.composite_score >= 0.0 && result.composite_score <= 1.0);
    }

    #[test]
    fn test_confluence_handles_non_finite_weights_and_strengths() {
        let technical = TechnicalSignal {
            pair: "BTCUSDT".to_string(),
            direction: SignalDirection::Long,
            strength: 0.8,
            contributors: vec!["ema_crossover".to_string()],
            votes: vec![IndicatorVote {
                name: "ema_crossover",
                direction: SignalDirection::Long,
                strength: f32::NAN,
            }],
        };

        let sentiment = SentimentSignal {
            pair: "BTCUSDT".to_string(),
            score: f32::INFINITY,
            delta_3tick: 0.0,
            direction: SignalDirection::Long,
        };

        let mut weights = EngineWeights::default();
        weights.ema_crossover = f32::NAN;
        weights.sentiment = f32::INFINITY;

        let result = merge_signals("BTCUSDT", &technical, &sentiment, &weights, 0.65);
        assert!(result.composite_score.is_finite());
        assert!(result.composite_score >= 0.0 && result.composite_score <= 1.0);
        assert_eq!(result.direction, SignalDirection::Wait);
    }

    #[test]
    fn test_confluence_zero_weights_results_in_non_actionable_wait() {
        let technical = TechnicalSignal {
            pair: "BTCUSDT".to_string(),
            direction: SignalDirection::Long,
            strength: 1.0,
            contributors: vec!["ema_crossover".to_string()],
            votes: vec![IndicatorVote {
                name: "ema_crossover",
                direction: SignalDirection::Long,
                strength: 1.0,
            }],
        };
        let sentiment = SentimentSignal {
            pair: "BTCUSDT".to_string(),
            score: 1.0,
            delta_3tick: 0.1,
            direction: SignalDirection::Long,
        };

        let mut weights = EngineWeights::default();
        weights.ema_crossover = 0.0;
        weights.sentiment = 0.0;

        let result = merge_signals("BTCUSDT", &technical, &sentiment, &weights, 0.3);
        assert_eq!(result.direction, SignalDirection::Wait);
        assert_eq!(result.composite_score, 0.0);
        assert!(!result.actionable);
    }
}
