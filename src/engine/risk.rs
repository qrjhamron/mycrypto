//! Risk stage for production signal engine.

use rust_decimal::Decimal;

use crate::config::Config;
use crate::engine::confluence::ConfluenceSignal;
use crate::state::{AppState, PositionSide, SignalDirection};

/// Risk evaluation output for one confluence signal.
#[derive(Debug, Clone)]
pub struct RiskAssessment {
    pub approved: bool,
    pub suggested_size: Decimal,
    pub rejection_reason: Option<String>,
    pub kelly_fraction: Decimal,
}

impl RiskAssessment {
    fn reject(reason: impl Into<String>) -> Self {
        Self {
            approved: false,
            suggested_size: Decimal::ZERO,
            rejection_reason: Some(reason.into()),
            kelly_fraction: Decimal::ZERO,
        }
    }
}

/// Evaluates risk constraints for a confluence signal at the current price.
#[must_use]
pub fn evaluate_risk(
    state: &AppState,
    signal: &ConfluenceSignal,
    price: Decimal,
) -> RiskAssessment {
    if price <= Decimal::ZERO {
        return RiskAssessment::reject("invalid market price");
    }

    if state.portfolio.current_drawdown_pct > state.config.risk.max_drawdown_pct {
        return RiskAssessment::reject(format!(
            "drawdown {:.2}% exceeds {:.2}%",
            state.portfolio.current_drawdown_pct, state.config.risk.max_drawdown_pct
        ));
    }

    let equity = state.portfolio.total_value();
    if equity <= Decimal::ZERO {
        return RiskAssessment::reject("portfolio equity is zero");
    }

    let max_position_fraction =
        (state.config.risk.max_position_pct / Decimal::from(100)).max(Decimal::ZERO);
    let max_position_value = equity * max_position_fraction;
    let total_exposure_limit =
        equity * (state.config.engine.total_exposure_limit_pct / Decimal::from(100));

    if total_exposure_limit <= Decimal::ZERO {
        return RiskAssessment::reject("total exposure limit is zero");
    }

    let current_exposure = state.portfolio.total_position_value();
    if current_exposure < Decimal::ZERO {
        return RiskAssessment::reject("portfolio exposure is negative");
    }

    if current_exposure >= total_exposure_limit {
        return RiskAssessment::reject("total exposure limit reached");
    }

    if is_correlated_overexposed(state, &signal.pair, signal.direction, &state.config) {
        return RiskAssessment::reject("correlated exposure limit hit");
    }

    let kelly_fraction = kelly_fraction_from_history(state);
    let capped_fraction = kelly_fraction.min(max_position_fraction).max(Decimal::ZERO);
    let remaining_exposure = (total_exposure_limit - current_exposure).max(Decimal::ZERO);
    let suggested_value = (equity * capped_fraction)
        .min(max_position_value)
        .min(remaining_exposure);
    if suggested_value <= Decimal::ZERO {
        return RiskAssessment::reject("kelly sizing produced zero allocation");
    }

    let suggested_size = suggested_value / price;
    if suggested_size <= Decimal::ZERO {
        return RiskAssessment::reject("suggested size too small");
    }

    RiskAssessment {
        approved: true,
        suggested_size,
        rejection_reason: None,
        kelly_fraction: capped_fraction,
    }
}

/// Estimates Kelly fraction from portfolio trade history with conservative caps.
#[must_use]
pub fn kelly_fraction_from_history(state: &AppState) -> Decimal {
    let trades = &state.portfolio.trade_history;
    if trades.len() < 5 {
        return Decimal::new(1, 2); // 1% fallback
    }

    let wins: Vec<Decimal> = trades
        .iter()
        .filter(|t| t.realized_pnl > Decimal::ZERO)
        .map(|t| t.realized_pnl)
        .collect();
    let losses: Vec<Decimal> = trades
        .iter()
        .filter(|t| t.realized_pnl < Decimal::ZERO)
        .map(|t| t.realized_pnl.abs())
        .collect();

    if wins.is_empty() || losses.is_empty() {
        return Decimal::new(1, 2);
    }

    let Some(wins_count) = u32::try_from(wins.len()).ok() else {
        return Decimal::new(1, 2);
    };
    let Some(trades_count) = u32::try_from(trades.len()).ok() else {
        return Decimal::new(1, 2);
    };
    if trades_count == 0 {
        return Decimal::new(1, 2);
    }

    let p = (Decimal::from(wins_count) / Decimal::from(trades_count))
        .clamp(Decimal::ZERO, Decimal::ONE);
    let q = Decimal::ONE - p;
    let Some(wins_len_u32) = u32::try_from(wins.len()).ok() else {
        return Decimal::new(1, 2);
    };
    let Some(losses_len_u32) = u32::try_from(losses.len()).ok() else {
        return Decimal::new(1, 2);
    };
    if wins_len_u32 == 0 || losses_len_u32 == 0 {
        return Decimal::new(1, 2);
    }

    let avg_win: Decimal = wins.iter().sum::<Decimal>() / Decimal::from(wins_len_u32);
    let avg_loss: Decimal = losses.iter().sum::<Decimal>() / Decimal::from(losses_len_u32);
    if avg_loss <= Decimal::ZERO {
        return Decimal::new(1, 2);
    }
    let b = avg_win / avg_loss;
    if b <= Decimal::ZERO {
        return Decimal::new(1, 2);
    }

    let raw = p - (q / b);
    if raw > Decimal::ZERO {
        raw.min(Decimal::new(25, 2)).max(Decimal::ZERO)
    } else {
        Decimal::new(1, 2)
    }
}

fn is_correlated_overexposed(
    state: &AppState,
    pair: &str,
    direction: SignalDirection,
    config: &Config,
) -> bool {
    let threshold = config.engine.correlation_threshold.clamp(0.0, 1.0);
    if threshold <= 0.0 {
        return false;
    }

    for position in &state.portfolio.positions {
        if position.pair == pair {
            continue;
        }
        let same_direction = matches!(
            (direction, position.side),
            (SignalDirection::Long, PositionSide::Long)
                | (SignalDirection::Short, PositionSide::Short)
        );
        if !same_direction {
            continue;
        }
        let corr = lookup_correlation(config, pair, &position.pair);
        if corr >= threshold {
            return true;
        }
    }

    false
}

fn lookup_correlation(config: &Config, a: &str, b: &str) -> f32 {
    if let Some(row) = config.engine.pair_correlation.get(a) {
        if let Some(v) = row.get(b) {
            return sanitize_corr(*v);
        }
    }
    if let Some(row) = config.engine.pair_correlation.get(b) {
        if let Some(v) = row.get(a) {
            return sanitize_corr(*v);
        }
    }

    if (a.starts_with("BTC") && b.starts_with("ETH"))
        || (a.starts_with("ETH") && b.starts_with("BTC"))
    {
        0.85
    } else if (a.starts_with("BTC") && b.starts_with("SOL"))
        || (a.starts_with("SOL") && b.starts_with("BTC"))
    {
        0.78
    } else {
        0.35
    }
}

fn sanitize_corr(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{AppState, CloseReason, ClosedTrade, PositionSide};
    use chrono::Utc;
    use uuid::Uuid;

    #[test]
    fn test_kelly_sizing_returns_positive_fraction_for_profitable_history() {
        let mut state = AppState::new(crate::config::Config::default());
        for pnl in [200, 150, -100, 120, 180, -90, 160] {
            state.portfolio.trade_history.push(ClosedTrade {
                id: Uuid::new_v4(),
                pair: "BTCUSDT".to_string(),
                side: PositionSide::Long,
                entry_price: Decimal::from(100),
                exit_price: Decimal::from(110),
                size: Decimal::ONE,
                realized_pnl: Decimal::from(pnl),
                realized_pnl_pct: Decimal::from(1),
                close_reason: CloseReason::Manual,
                signal_confidence: 70,
                opened_at: Utc::now(),
                closed_at: Utc::now(),
            });
        }

        let fraction = kelly_fraction_from_history(&state);
        assert!(fraction > Decimal::ZERO);
        assert!(fraction <= Decimal::new(25, 2));
    }

    #[test]
    fn test_kelly_fallback_when_history_too_short_or_unbalanced() {
        let mut state = AppState::new(crate::config::Config::default());
        assert_eq!(kelly_fraction_from_history(&state), Decimal::new(1, 2));

        for _ in 0..6 {
            state.portfolio.trade_history.push(ClosedTrade {
                id: Uuid::new_v4(),
                pair: "BTCUSDT".to_string(),
                side: PositionSide::Long,
                entry_price: Decimal::from(100),
                exit_price: Decimal::from(110),
                size: Decimal::ONE,
                realized_pnl: Decimal::from(10),
                realized_pnl_pct: Decimal::from(1),
                close_reason: CloseReason::Manual,
                signal_confidence: 70,
                opened_at: Utc::now(),
                closed_at: Utc::now(),
            });
        }

        assert_eq!(kelly_fraction_from_history(&state), Decimal::new(1, 2));
    }
}
