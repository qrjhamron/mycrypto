//! Order execution simulation for paper trading.
//!
//! Executes market orders and manages position entry/exit for paper trading.
//! Works with the existing Position struct from state/portfolio.rs.

use crate::state::{AppState, Position, PositionSide};
use rust_decimal::Decimal;

/// Market order specification for execution.
#[derive(Debug, Clone)]
pub struct MarketOrder {
    pub pair: String,
    pub side: PositionSide,
    pub size: Decimal,
    pub entry_price: Decimal,
    pub stop_loss: Decimal,
    pub take_profit: Decimal,
    pub signal_confidence: u8,
}

impl MarketOrder {
    /// Create a new market order.
    /// All parameters are assumed pre-validated by the caller.
    pub fn new(
        pair: String,
        side: PositionSide,
        size: Decimal,
        entry_price: Decimal,
        stop_loss: Decimal,
        take_profit: Decimal,
        signal_confidence: u8,
    ) -> Self {
        MarketOrder {
            pair,
            side,
            size,
            entry_price,
            stop_loss,
            take_profit,
            signal_confidence,
        }
    }
}

/// Executes market orders for paper trading.
pub struct OrderExecutor;

impl OrderExecutor {
    /// Execute a market order and add the position to the portfolio.
    pub fn execute_order(
        state: &mut AppState,
        order: MarketOrder,
    ) -> crate::error::Result<Position> {
        // Create new position using the Position::new constructor
        let position = Position::new(
            order.pair,
            order.side,
            order.entry_price,
            order.size,
            order.stop_loss,
            order.take_profit,
            order.signal_confidence,
        );

        // Delegate portfolio mutation to the source-of-truth API.
        state.portfolio.open_position(position.clone())?;

        Ok(position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn test_market_order_new_long() {
        let order = MarketOrder::new(
            "BTCUSDT".to_string(),
            PositionSide::Long,
            Decimal::new(1, 0),
            Decimal::new(50000, 0),
            Decimal::new(49000, 0),
            Decimal::new(51000, 0),
            75,
        );

        assert_eq!(order.side, PositionSide::Long);
        assert_eq!(order.size, Decimal::new(1, 0));
        assert_eq!(order.signal_confidence, 75);
    }

    #[test]
    fn test_market_order_new_short() {
        let order = MarketOrder::new(
            "ETHUSDT".to_string(),
            PositionSide::Short,
            Decimal::new(10, 0),
            Decimal::new(3000, 0),
            Decimal::new(3100, 0),
            Decimal::new(2900, 0),
            60,
        );

        assert_eq!(order.side, PositionSide::Short);
        assert_eq!(order.size, Decimal::new(10, 0));
    }

    #[test]
    fn test_market_order_fields() {
        let order = MarketOrder::new(
            "BTCUSDT".to_string(),
            PositionSide::Long,
            Decimal::new(1, 0),
            Decimal::new(50000, 0),
            Decimal::new(49000, 0),
            Decimal::new(51000, 0),
            75,
        );

        assert_eq!(order.entry_price, Decimal::new(50000, 0));
        assert_eq!(order.stop_loss, Decimal::new(49000, 0));
        assert_eq!(order.take_profit, Decimal::new(51000, 0));
    }

    #[test]
    fn test_market_order_confidence() {
        let order = MarketOrder::new(
            "BTCUSDT".to_string(),
            PositionSide::Long,
            Decimal::new(1, 0),
            Decimal::new(50000, 0),
            Decimal::new(49000, 0),
            Decimal::new(51000, 0),
            100,
        );

        assert_eq!(order.signal_confidence, 100);
    }

    #[test]
    fn test_market_order_zero_confidence() {
        let order = MarketOrder::new(
            "BTCUSDT".to_string(),
            PositionSide::Long,
            Decimal::new(1, 0),
            Decimal::new(50000, 0),
            Decimal::new(49000, 0),
            Decimal::new(51000, 0),
            0,
        );

        assert_eq!(order.signal_confidence, 0);
    }

    #[test]
    fn test_execute_order_uses_portfolio_api() {
        let mut state = AppState::new(Config::default());
        let starting_cash = state.portfolio.cash;

        let order = MarketOrder::new(
            "BTCUSDT".to_string(),
            PositionSide::Long,
            Decimal::new(1, 1),
            Decimal::new(50000, 0),
            Decimal::new(49000, 0),
            Decimal::new(51000, 0),
            75,
        );

        let position = OrderExecutor::execute_order(&mut state, order).unwrap();

        assert_eq!(state.portfolio.positions.len(), 1);
        assert_eq!(state.portfolio.positions[0].id, position.id);
        assert!(state.portfolio.cash < starting_cash);
        assert!(state.portfolio.invested > Decimal::ZERO);
    }

    #[test]
    fn test_execute_order_fails_on_insufficient_balance() {
        let mut state = AppState::new(Config::default());

        let order = MarketOrder::new(
            "BTCUSDT".to_string(),
            PositionSide::Long,
            Decimal::new(1, 0),
            Decimal::new(500000, 0),
            Decimal::new(490000, 0),
            Decimal::new(510000, 0),
            75,
        );

        let result = OrderExecutor::execute_order(&mut state, order);
        assert!(result.is_err());
        assert!(state.portfolio.positions.is_empty());
    }
}
