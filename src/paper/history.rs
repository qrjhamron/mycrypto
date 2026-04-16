//! Trade history tracking for paper trading.
//!
//! Records all closed trades with entry/exit details, PnL, and statistics.

use crate::state::ClosedTrade;
use rust_decimal::Decimal;
use std::collections::VecDeque;

/// Statistics for closed trades over a period.
#[derive(Debug, Clone, Copy)]
pub struct TradeStats {
    /// Total number of closed trades
    pub total_trades: usize,
    /// Number of winning trades
    pub winning_trades: usize,
    /// Number of losing trades
    pub losing_trades: usize,
    /// Win rate percentage
    pub win_rate_pct: Decimal,
    /// Total realized PnL
    pub total_pnl: Decimal,
    /// Average PnL per trade
    pub avg_pnl: Decimal,
    /// Largest winning trade
    pub largest_win: Decimal,
    /// Largest losing trade
    pub largest_loss: Decimal,
    /// Profit factor (gross wins / gross losses)
    pub profit_factor: Decimal,
}

/// Manages trade history and statistics.
pub struct TradeHistory {
    /// Circular buffer of closed trades
    trades: VecDeque<ClosedTrade>,
    /// Maximum number of trades to keep in history
    max_history: usize,
}

impl TradeHistory {
    /// Create a new trade history with specified max size.
    pub fn new(max_history: usize) -> Self {
        TradeHistory {
            trades: VecDeque::with_capacity(max_history),
            max_history,
        }
    }

    /// Record a closed trade.
    pub fn record_trade(&mut self, trade: ClosedTrade) {
        if self.trades.len() >= self.max_history {
            self.trades.pop_front();
        }
        self.trades.push_back(trade);
    }

    /// Get all trades in history.
    pub fn get_trades(&self) -> Vec<&ClosedTrade> {
        self.trades.iter().collect()
    }

    /// Get trades for a specific pair.
    pub fn get_trades_for_pair(&self, pair: &str) -> Vec<&ClosedTrade> {
        self.trades.iter().filter(|t| t.pair == pair).collect()
    }

    /// Get number of trades in history.
    pub fn trade_count(&self) -> usize {
        self.trades.len()
    }

    /// Calculate statistics for all trades.
    pub fn calculate_stats(&self) -> TradeStats {
        let total_trades = self.trades.len();

        if total_trades == 0 {
            return TradeStats {
                total_trades: 0,
                winning_trades: 0,
                losing_trades: 0,
                win_rate_pct: Decimal::ZERO,
                total_pnl: Decimal::ZERO,
                avg_pnl: Decimal::ZERO,
                largest_win: Decimal::ZERO,
                largest_loss: Decimal::ZERO,
                profit_factor: Decimal::ZERO,
            };
        }

        let mut winning_trades = 0;
        let mut losing_trades = 0;
        let mut total_pnl = Decimal::ZERO;
        let mut largest_win = Decimal::ZERO;
        let mut largest_loss = Decimal::ZERO;
        let mut gross_wins = Decimal::ZERO;
        let mut gross_losses = Decimal::ZERO;

        for trade in &self.trades {
            let pnl = trade.realized_pnl;
            total_pnl += pnl;

            if pnl > Decimal::ZERO {
                winning_trades += 1;
                gross_wins += pnl;
                if pnl > largest_win {
                    largest_win = pnl;
                }
            } else if pnl < Decimal::ZERO {
                losing_trades += 1;
                gross_losses += pnl.abs();
                if pnl < largest_loss {
                    largest_loss = pnl;
                }
            }
        }

        let win_rate_pct = if total_trades > 0 {
            (Decimal::new(winning_trades as i64, 0) / Decimal::new(total_trades as i64, 0))
                * Decimal::new(100, 0)
        } else {
            Decimal::ZERO
        };

        let avg_pnl = if total_trades > 0 {
            total_pnl / Decimal::new(total_trades as i64, 0)
        } else {
            Decimal::ZERO
        };

        let profit_factor = if gross_losses > Decimal::ZERO {
            gross_wins / gross_losses
        } else if gross_wins > Decimal::ZERO {
            Decimal::new(999999, 0)
        } else {
            Decimal::ZERO
        };

        TradeStats {
            total_trades,
            winning_trades,
            losing_trades,
            win_rate_pct,
            total_pnl,
            avg_pnl,
            largest_win,
            largest_loss,
            profit_factor,
        }
    }

    /// Calculate statistics for trades on a specific pair.
    pub fn calculate_stats_for_pair(&self, pair: &str) -> TradeStats {
        let trades: Vec<_> = self.trades.iter().filter(|t| t.pair == pair).collect();

        let total_trades = trades.len();

        if total_trades == 0 {
            return TradeStats {
                total_trades: 0,
                winning_trades: 0,
                losing_trades: 0,
                win_rate_pct: Decimal::ZERO,
                total_pnl: Decimal::ZERO,
                avg_pnl: Decimal::ZERO,
                largest_win: Decimal::ZERO,
                largest_loss: Decimal::ZERO,
                profit_factor: Decimal::ZERO,
            };
        }

        let mut winning_trades = 0;
        let mut losing_trades = 0;
        let mut total_pnl = Decimal::ZERO;
        let mut largest_win = Decimal::ZERO;
        let mut largest_loss = Decimal::ZERO;
        let mut gross_wins = Decimal::ZERO;
        let mut gross_losses = Decimal::ZERO;

        for trade in trades {
            let pnl = trade.realized_pnl;
            total_pnl += pnl;

            if pnl > Decimal::ZERO {
                winning_trades += 1;
                gross_wins += pnl;
                if pnl > largest_win {
                    largest_win = pnl;
                }
            } else if pnl < Decimal::ZERO {
                losing_trades += 1;
                gross_losses += pnl.abs();
                if pnl < largest_loss {
                    largest_loss = pnl;
                }
            }
        }

        let win_rate_pct = if total_trades > 0 {
            (Decimal::new(winning_trades as i64, 0) / Decimal::new(total_trades as i64, 0))
                * Decimal::new(100, 0)
        } else {
            Decimal::ZERO
        };

        let avg_pnl = if total_trades > 0 {
            total_pnl / Decimal::new(total_trades as i64, 0)
        } else {
            Decimal::ZERO
        };

        let profit_factor = if gross_losses > Decimal::ZERO {
            gross_wins / gross_losses
        } else if gross_wins > Decimal::ZERO {
            Decimal::new(999999, 0)
        } else {
            Decimal::ZERO
        };

        TradeStats {
            total_trades,
            winning_trades,
            losing_trades,
            win_rate_pct,
            total_pnl,
            avg_pnl,
            largest_win,
            largest_loss,
            profit_factor,
        }
    }

    /// Clear all trade history.
    pub fn clear(&mut self) {
        self.trades.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{CloseReason, PositionSide};
    use chrono::Utc;
    use uuid::Uuid;

    fn create_test_trade(pnl: i64, side: PositionSide) -> ClosedTrade {
        ClosedTrade {
            id: Uuid::new_v4(),
            pair: "BTCUSDT".to_string(),
            side,
            entry_price: Decimal::new(50000, 0),
            exit_price: Decimal::new(51000, 0),
            size: Decimal::new(1, 0),
            realized_pnl: Decimal::new(pnl, 0),
            realized_pnl_pct: Decimal::new(2, 0),
            close_reason: CloseReason::TakeProfit,
            signal_confidence: 75,
            opened_at: Utc::now(),
            closed_at: Utc::now(),
        }
    }

    #[test]
    fn test_record_trade() {
        let mut history = TradeHistory::new(100);
        let trade = create_test_trade(1000, PositionSide::Long);
        history.record_trade(trade);

        assert_eq!(history.trade_count(), 1);
    }

    #[test]
    fn test_max_history_respected() {
        let mut history = TradeHistory::new(2);
        history.record_trade(create_test_trade(100, PositionSide::Long));
        history.record_trade(create_test_trade(200, PositionSide::Long));
        history.record_trade(create_test_trade(300, PositionSide::Long));

        assert_eq!(history.trade_count(), 2);
    }

    #[test]
    fn test_calculate_stats_empty() {
        let history = TradeHistory::new(100);
        let stats = history.calculate_stats();

        assert_eq!(stats.total_trades, 0);
        assert_eq!(stats.total_pnl, Decimal::ZERO);
    }

    #[test]
    fn test_calculate_stats_single_win() {
        let mut history = TradeHistory::new(100);
        history.record_trade(create_test_trade(1000, PositionSide::Long));

        let stats = history.calculate_stats();
        assert_eq!(stats.total_trades, 1);
        assert_eq!(stats.winning_trades, 1);
        assert_eq!(stats.losing_trades, 0);
        assert_eq!(stats.total_pnl, Decimal::new(1000, 0));
        assert_eq!(stats.win_rate_pct, Decimal::new(100, 0));
    }

    #[test]
    fn test_calculate_stats_mixed() {
        let mut history = TradeHistory::new(100);
        history.record_trade(create_test_trade(1000, PositionSide::Long));
        history.record_trade(create_test_trade(-500, PositionSide::Long));
        history.record_trade(create_test_trade(800, PositionSide::Short));

        let stats = history.calculate_stats();
        assert_eq!(stats.total_trades, 3);
        assert_eq!(stats.winning_trades, 2);
        assert_eq!(stats.losing_trades, 1);
        assert_eq!(stats.total_pnl, Decimal::new(1300, 0));
        assert_eq!(stats.largest_win, Decimal::new(1000, 0));
        assert_eq!(stats.largest_loss, Decimal::new(-500, 0));
    }

    #[test]
    fn test_calculate_stats_profit_factor() {
        let mut history = TradeHistory::new(100);
        history.record_trade(create_test_trade(1000, PositionSide::Long));
        history.record_trade(create_test_trade(2000, PositionSide::Long));
        history.record_trade(create_test_trade(-1000, PositionSide::Short));

        let stats = history.calculate_stats();
        assert_eq!(stats.profit_factor, Decimal::new(3, 0));
    }

    #[test]
    fn test_get_trades_for_pair() {
        let mut history = TradeHistory::new(100);
        let mut trade1 = create_test_trade(1000, PositionSide::Long);
        trade1.pair = "BTCUSDT".to_string();
        let mut trade2 = create_test_trade(500, PositionSide::Long);
        trade2.pair = "ETHUSDT".to_string();
        let mut trade3 = create_test_trade(800, PositionSide::Long);
        trade3.pair = "BTCUSDT".to_string();

        history.record_trade(trade1);
        history.record_trade(trade2);
        history.record_trade(trade3);

        let btc_trades = history.get_trades_for_pair("BTCUSDT");
        assert_eq!(btc_trades.len(), 2);
    }
}
