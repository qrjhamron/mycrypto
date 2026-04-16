//! Position simulation and PnL calculation for paper trading.
//!
//! Handles real-time PnL updates, SL/TP trigger detection, and position metrics.
//! Delegates to the existing Position struct methods from state/portfolio.rs.

use crate::state::Position;
use rust_decimal::Decimal;

/// Real-time metrics for an open position.
#[derive(Debug, Clone, Copy)]
pub struct PositionMetrics {
    /// Unrealized P&L in quote currency
    pub unrealized_pnl: Decimal,
    /// Unrealized P&L percentage
    pub unrealized_pnl_pct: Decimal,
    /// Whether SL has been triggered
    pub sl_triggered: bool,
    /// Whether TP has been triggered
    pub tp_triggered: bool,
}

/// Simulates position changes and calculates PnL.
pub struct PositionSimulator;

impl PositionSimulator {
    /// Get comprehensive metrics for a position at its current price.
    pub fn calculate_metrics(position: &Position) -> PositionMetrics {
        let sl_triggered = position.is_stop_loss_hit();
        let tp_triggered = position.is_take_profit_hit();

        PositionMetrics {
            unrealized_pnl: position.unrealized_pnl,
            unrealized_pnl_pct: position.unrealized_pnl_pct,
            sl_triggered,
            tp_triggered,
        }
    }

    /// Update position's current price and state based on market tick.
    /// Returns true if any stop condition was triggered.
    pub fn update_position_on_tick(position: &mut Position, current_price: Decimal) -> bool {
        position.update_price(current_price);

        // Check for SL/TP triggers
        position.is_stop_loss_hit() || position.is_take_profit_hit()
    }

    /// Check if stop loss has been triggered.
    pub fn is_sl_triggered(position: &Position) -> bool {
        position.is_stop_loss_hit()
    }

    /// Check if take profit has been triggered.
    pub fn is_tp_triggered(position: &Position) -> bool {
        position.is_take_profit_hit()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::PositionSide;

    fn create_test_position(side: PositionSide, entry: i64, sl: i64, tp: i64) -> Position {
        Position::new(
            "BTCUSDT".to_string(),
            side,
            Decimal::new(entry, 0),
            Decimal::new(1, 0),
            Decimal::new(sl, 0),
            Decimal::new(tp, 0),
            75,
        )
    }

    #[test]
    fn test_calculate_metrics_long_profit() {
        let mut pos = create_test_position(PositionSide::Long, 50000, 49000, 51000);
        pos.update_price(Decimal::new(50500, 0));
        let metrics = PositionSimulator::calculate_metrics(&pos);

        assert_eq!(metrics.unrealized_pnl, Decimal::new(500, 0));
        assert!(!metrics.sl_triggered);
        assert!(!metrics.tp_triggered);
    }

    #[test]
    fn test_calculate_metrics_short_profit() {
        let mut pos = create_test_position(PositionSide::Short, 50000, 51000, 49000);
        pos.update_price(Decimal::new(49500, 0));
        let metrics = PositionSimulator::calculate_metrics(&pos);

        assert_eq!(metrics.unrealized_pnl, Decimal::new(500, 0));
        assert!(!metrics.sl_triggered);
        assert!(!metrics.tp_triggered);
    }

    #[test]
    fn test_is_sl_triggered_long() {
        let mut pos = create_test_position(PositionSide::Long, 50000, 49000, 51000);

        pos.update_price(Decimal::new(49500, 0));
        assert!(!PositionSimulator::is_sl_triggered(&pos));

        pos.update_price(Decimal::new(49000, 0));
        assert!(PositionSimulator::is_sl_triggered(&pos));
    }

    #[test]
    fn test_is_tp_triggered_long() {
        let mut pos = create_test_position(PositionSide::Long, 50000, 49000, 51000);

        pos.update_price(Decimal::new(50500, 0));
        assert!(!PositionSimulator::is_tp_triggered(&pos));

        pos.update_price(Decimal::new(51000, 0));
        assert!(PositionSimulator::is_tp_triggered(&pos));
    }

    #[test]
    fn test_is_sl_triggered_short() {
        let mut pos = create_test_position(PositionSide::Short, 50000, 51000, 49000);

        pos.update_price(Decimal::new(50500, 0));
        assert!(!PositionSimulator::is_sl_triggered(&pos));

        pos.update_price(Decimal::new(51000, 0));
        assert!(PositionSimulator::is_sl_triggered(&pos));
    }

    #[test]
    fn test_update_position_on_tick() {
        let mut pos = create_test_position(PositionSide::Long, 50000, 49000, 51000);

        let triggered =
            PositionSimulator::update_position_on_tick(&mut pos, Decimal::new(50500, 0));
        assert!(!triggered);
        assert_eq!(pos.current_price, Decimal::new(50500, 0));
    }

    #[test]
    fn test_update_position_triggers_tp() {
        let mut pos = create_test_position(PositionSide::Long, 50000, 49000, 51000);

        let triggered =
            PositionSimulator::update_position_on_tick(&mut pos, Decimal::new(51000, 0));
        assert!(triggered);
    }
}
