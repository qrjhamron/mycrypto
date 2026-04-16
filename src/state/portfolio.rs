//! Portfolio and position domain types.
//!
//! This module defines the core financial structures for paper trading:
//! - `Portfolio` - the virtual trading account
//! - `Position` - an open paper trade
//! - `ClosedTrade` - a completed trade with realized PnL
//!
//! # Financial Precision
//!
//! All monetary values use `rust_decimal::Decimal` to avoid floating-point
//! errors in financial calculations. Never use `f64` for money.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Direction of a trade position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PositionSide {
    /// Long position - profit when price goes up.
    Long,
    /// Short position - profit when price goes down.
    Short,
}

impl std::fmt::Display for PositionSide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PositionSide::Long => write!(f, "LONG"),
            PositionSide::Short => write!(f, "SHORT"),
        }
    }
}

/// Reason a position was closed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloseReason {
    /// Stop loss was triggered.
    StopLoss,
    /// Take profit target was hit.
    TakeProfit,
    /// Trailing stop was triggered.
    TrailingStop,
    /// Manually closed by user.
    Manual,
    /// Position expired.
    Expired,
    /// System shutdown.
    Shutdown,
}

impl std::fmt::Display for CloseReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CloseReason::StopLoss => write!(f, "Stop Loss"),
            CloseReason::TakeProfit => write!(f, "Take Profit"),
            CloseReason::TrailingStop => write!(f, "Trailing Stop"),
            CloseReason::Manual => write!(f, "Manual Close"),
            CloseReason::Expired => write!(f, "Expired"),
            CloseReason::Shutdown => write!(f, "Shutdown"),
        }
    }
}

/// An open paper trading position.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    /// Unique identifier for this position.
    pub id: Uuid,

    /// Trading pair symbol (e.g., "BTCUSDT").
    pub pair: String,

    /// Long or short.
    pub side: PositionSide,

    /// Entry price at which the position was opened.
    pub entry_price: Decimal,

    /// Current market price (updated on each tick).
    pub current_price: Decimal,

    /// Position size in base currency units.
    pub size: Decimal,

    /// Value invested at entry (entry_price * size).
    pub notional_value: Decimal,

    /// Stop loss price level.
    pub stop_loss: Decimal,

    /// Take profit price level.
    pub take_profit: Decimal,

    /// Whether trailing stop is active.
    pub trailing_stop_active: bool,

    /// Current trailing stop level (if active).
    pub trailing_stop_price: Option<Decimal>,

    /// Unrealized PnL in quote currency.
    pub unrealized_pnl: Decimal,

    /// Unrealized PnL as percentage.
    pub unrealized_pnl_pct: Decimal,

    /// Confidence score of the signal that opened this position.
    pub signal_confidence: u8,

    /// When the position was opened.
    pub opened_at: DateTime<Utc>,
}

impl Position {
    /// Creates a new position with the given parameters.
    ///
    /// # Arguments
    /// * `pair` - Trading pair symbol
    /// * `side` - Long or short
    /// * `entry_price` - Entry price
    /// * `size` - Position size in base currency
    /// * `stop_loss` - Stop loss price
    /// * `take_profit` - Take profit price
    /// * `signal_confidence` - Confidence score (0-100)
    pub fn new(
        pair: String,
        side: PositionSide,
        entry_price: Decimal,
        size: Decimal,
        stop_loss: Decimal,
        take_profit: Decimal,
        signal_confidence: u8,
    ) -> Self {
        let notional_value = entry_price * size;
        Self {
            id: Uuid::new_v4(),
            pair,
            side,
            entry_price,
            current_price: entry_price,
            size,
            notional_value,
            stop_loss,
            take_profit,
            trailing_stop_active: false,
            trailing_stop_price: None,
            unrealized_pnl: Decimal::ZERO,
            unrealized_pnl_pct: Decimal::ZERO,
            signal_confidence,
            opened_at: Utc::now(),
        }
    }

    /// Updates the position with a new market price.
    ///
    /// Recalculates unrealized PnL and percentage.
    pub fn update_price(&mut self, new_price: Decimal) {
        self.current_price = new_price;
        self.recalculate_pnl();
    }

    /// Recalculates unrealized PnL based on current price.
    fn recalculate_pnl(&mut self) {
        self.unrealized_pnl = match self.side {
            PositionSide::Long => (self.current_price - self.entry_price) * self.size,
            PositionSide::Short => (self.entry_price - self.current_price) * self.size,
        };

        // Calculate percentage PnL
        if self.notional_value != Decimal::ZERO {
            self.unrealized_pnl_pct =
                (self.unrealized_pnl / self.notional_value) * Decimal::from(100);
        }
    }

    /// Checks if stop loss has been triggered.
    pub fn is_stop_loss_hit(&self) -> bool {
        match self.side {
            PositionSide::Long => self.current_price <= self.stop_loss,
            PositionSide::Short => self.current_price >= self.stop_loss,
        }
    }

    /// Checks if take profit has been triggered.
    pub fn is_take_profit_hit(&self) -> bool {
        match self.side {
            PositionSide::Long => self.current_price >= self.take_profit,
            PositionSide::Short => self.current_price <= self.take_profit,
        }
    }

    /// Checks if trailing stop has been triggered.
    pub fn is_trailing_stop_hit(&self) -> bool {
        if let Some(trail_price) = self.trailing_stop_price {
            match self.side {
                PositionSide::Long => self.current_price <= trail_price,
                PositionSide::Short => self.current_price >= trail_price,
            }
        } else {
            false
        }
    }

    /// Updates trailing stop if applicable.
    ///
    /// # Arguments
    /// * `trail_pct` - Trailing stop offset as percentage (e.g., 1.0 for 1%)
    pub fn update_trailing_stop(&mut self, trail_pct: Decimal) {
        if !self.trailing_stop_active || self.unrealized_pnl <= Decimal::ZERO {
            return;
        }

        let offset = self.current_price * (trail_pct / Decimal::from(100));
        let new_trail = match self.side {
            PositionSide::Long => self.current_price - offset,
            PositionSide::Short => self.current_price + offset,
        };

        // Only move trailing stop in favorable direction
        match self.side {
            PositionSide::Long => {
                if let Some(current_trail) = self.trailing_stop_price {
                    if new_trail > current_trail {
                        self.trailing_stop_price = Some(new_trail);
                    }
                } else if new_trail > self.stop_loss {
                    self.trailing_stop_price = Some(new_trail);
                }
            }
            PositionSide::Short => {
                if let Some(current_trail) = self.trailing_stop_price {
                    if new_trail < current_trail {
                        self.trailing_stop_price = Some(new_trail);
                    }
                } else if new_trail < self.stop_loss {
                    self.trailing_stop_price = Some(new_trail);
                }
            }
        }
    }

    /// Gets the duration the position has been open.
    pub fn duration(&self) -> chrono::Duration {
        Utc::now() - self.opened_at
    }

    /// Returns a human-readable duration string.
    pub fn duration_display(&self) -> String {
        let dur = self.duration();
        let hours = dur.num_hours();
        let mins = dur.num_minutes() % 60;

        if hours > 0 {
            format!("{}h {}m", hours, mins)
        } else {
            format!("{}m", mins)
        }
    }
}

/// A completed paper trade with realized PnL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClosedTrade {
    /// Unique identifier (same as the position it came from).
    pub id: Uuid,

    /// Trading pair symbol.
    pub pair: String,

    /// Trade direction.
    pub side: PositionSide,

    /// Entry price.
    pub entry_price: Decimal,

    /// Exit price.
    pub exit_price: Decimal,

    /// Position size.
    pub size: Decimal,

    /// Realized PnL in quote currency.
    pub realized_pnl: Decimal,

    /// Realized PnL as percentage.
    pub realized_pnl_pct: Decimal,

    /// Why the position was closed.
    pub close_reason: CloseReason,

    /// Signal confidence when opened.
    pub signal_confidence: u8,

    /// When the position was opened.
    pub opened_at: DateTime<Utc>,

    /// When the position was closed.
    pub closed_at: DateTime<Utc>,
}

impl ClosedTrade {
    /// Creates a closed trade from a position.
    ///
    /// # Arguments
    /// * `position` - The position being closed
    /// * `exit_price` - The price at which it was closed
    /// * `reason` - Why it was closed
    pub fn from_position(position: &Position, exit_price: Decimal, reason: CloseReason) -> Self {
        let realized_pnl = match position.side {
            PositionSide::Long => (exit_price - position.entry_price) * position.size,
            PositionSide::Short => (position.entry_price - exit_price) * position.size,
        };

        let realized_pnl_pct = if position.notional_value != Decimal::ZERO {
            (realized_pnl / position.notional_value) * Decimal::from(100)
        } else {
            Decimal::ZERO
        };

        Self {
            id: position.id,
            pair: position.pair.clone(),
            side: position.side,
            entry_price: position.entry_price,
            exit_price,
            size: position.size,
            realized_pnl,
            realized_pnl_pct,
            close_reason: reason,
            signal_confidence: position.signal_confidence,
            opened_at: position.opened_at,
            closed_at: Utc::now(),
        }
    }

    /// Returns true if this was a winning trade.
    pub fn is_winner(&self) -> bool {
        self.realized_pnl > Decimal::ZERO
    }

    /// Gets the trade duration.
    pub fn duration(&self) -> chrono::Duration {
        self.closed_at - self.opened_at
    }
}

/// The virtual portfolio account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Portfolio {
    /// Available cash in quote currency.
    pub cash: Decimal,

    /// Quote currency symbol.
    pub currency: String,

    /// Total value invested in open positions.
    pub invested: Decimal,

    /// Current total unrealized PnL.
    pub unrealized_pnl: Decimal,

    /// Today's realized PnL.
    pub daily_realized_pnl: Decimal,

    /// All-time realized PnL.
    pub total_realized_pnl: Decimal,

    /// Highest portfolio value seen (for drawdown calculation).
    pub peak_value: Decimal,

    /// Current drawdown percentage from peak.
    pub current_drawdown_pct: Decimal,

    /// List of open positions.
    pub positions: Vec<Position>,

    /// History of closed trades.
    pub trade_history: Vec<ClosedTrade>,

    /// When the portfolio was initialized.
    pub created_at: DateTime<Utc>,

    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
}

impl Portfolio {
    /// Creates a new portfolio with the given starting balance.
    ///
    /// # Arguments
    /// * `initial_balance` - Starting cash balance
    /// * `currency` - Quote currency symbol (e.g., "USDT")
    pub fn new(initial_balance: Decimal, currency: String) -> Self {
        let now = Utc::now();
        Self {
            cash: initial_balance,
            currency,
            invested: Decimal::ZERO,
            unrealized_pnl: Decimal::ZERO,
            daily_realized_pnl: Decimal::ZERO,
            total_realized_pnl: Decimal::ZERO,
            peak_value: initial_balance,
            current_drawdown_pct: Decimal::ZERO,
            positions: Vec::new(),
            trade_history: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Calculates the total portfolio value (cash + positions + unrealized PnL).
    pub fn total_value(&self) -> Decimal {
        self.cash + self.invested + self.unrealized_pnl
    }

    /// Recalculates portfolio metrics from positions.
    ///
    /// Lifecycle guarantee:
    /// This method is called by portfolio mutation methods to keep aggregate
    /// fields (`invested`, `unrealized_pnl`, drawdown, timestamps) consistent
    /// after every open/close/update operation.
    pub fn recalculate(&mut self) {
        self.invested = self.positions.iter().map(|p| p.notional_value).sum();
        self.unrealized_pnl = self.positions.iter().map(|p| p.unrealized_pnl).sum();

        let total = self.total_value();
        if total >= self.peak_value {
            self.peak_value = total;
            self.current_drawdown_pct = Decimal::ZERO;
        } else if self.peak_value > Decimal::ZERO {
            self.current_drawdown_pct =
                ((self.peak_value - total) / self.peak_value) * Decimal::from(100);
        } else {
            self.current_drawdown_pct = Decimal::ZERO;
        }

        self.updated_at = Utc::now();
    }

    /// Adds a new position to the portfolio.
    ///
    /// # Arguments
    /// * `position` - The position to add
    ///
    /// # Returns
    /// * `Ok(())` if successful
    /// * `Err` if insufficient funds
    ///
    /// Lifecycle guarantee:
    /// - Deducts entry notional from `cash`
    /// - Appends the position to `positions`
    /// - Recomputes all aggregate metrics via [`Portfolio::recalculate`]
    ///
    /// Callers should never push directly to `positions`; use this method so
    /// cash, exposure, and drawdown state remain correct.
    #[must_use = "handle insufficient balance errors from open_position"]
    pub fn open_position(&mut self, position: Position) -> crate::error::Result<()> {
        if position.entry_price <= Decimal::ZERO {
            return Err(crate::error::MycryptoError::PositionOperation(
                "entry price must be greater than zero".to_string(),
            ));
        }
        if position.size <= Decimal::ZERO {
            return Err(crate::error::MycryptoError::PositionOperation(
                "position size must be greater than zero".to_string(),
            ));
        }
        if position.notional_value <= Decimal::ZERO {
            return Err(crate::error::MycryptoError::PositionOperation(
                "position notional must be greater than zero".to_string(),
            ));
        }

        let valid_risk_levels = match position.side {
            PositionSide::Long => {
                position.stop_loss < position.entry_price
                    && position.take_profit > position.entry_price
            }
            PositionSide::Short => {
                position.stop_loss > position.entry_price
                    && position.take_profit < position.entry_price
            }
        };
        if !valid_risk_levels {
            return Err(crate::error::MycryptoError::PositionOperation(
                "invalid stop-loss/take-profit for position side".to_string(),
            ));
        }

        if position.notional_value > self.cash {
            return Err(crate::error::MycryptoError::InsufficientBalance {
                required: position.notional_value,
                available: self.cash,
            });
        }

        self.cash -= position.notional_value;
        self.positions.push(position);
        self.recalculate();
        Ok(())
    }

    /// Closes a position and moves it to trade history.
    ///
    /// # Arguments
    /// * `position_id` - UUID of the position to close
    /// * `exit_price` - Price at which to close
    /// * `reason` - Why the position is being closed
    ///
    /// # Returns
    /// * `Some(ClosedTrade)` if position was found and closed
    /// * `None` if position not found
    ///
    /// Lifecycle guarantee:
    /// - Removes the position from `positions`
    /// - Creates a canonical [`ClosedTrade`] snapshot
    /// - Credits `cash` with principal + realized PnL
    /// - Updates daily/all-time realized PnL totals
    /// - Appends to `trade_history`
    /// - Recomputes aggregate metrics via [`Portfolio::recalculate`]
    ///
    /// Callers should never remove from `positions` or append to
    /// `trade_history` directly; use this method to keep accounting consistent.
    pub fn close_position(
        &mut self,
        position_id: Uuid,
        exit_price: Decimal,
        reason: CloseReason,
    ) -> Option<ClosedTrade> {
        if exit_price <= Decimal::ZERO {
            return None;
        }

        let idx = self.positions.iter().position(|p| p.id == position_id)?;
        let position = self.positions.remove(idx);

        let closed_trade = ClosedTrade::from_position(&position, exit_price, reason);

        // Return funds + PnL to cash
        self.cash += position.notional_value + closed_trade.realized_pnl;
        self.daily_realized_pnl += closed_trade.realized_pnl;
        self.total_realized_pnl += closed_trade.realized_pnl;

        self.trade_history.push(closed_trade.clone());
        self.recalculate();

        Some(closed_trade)
    }

    /// Closes an open position by pair at its current tracked market price.
    ///
    /// This is a convenience wrapper around [`Portfolio::close_position`]
    /// intended for manual/command-driven closes where execution happens at the
    /// latest in-state price.
    ///
    /// Lifecycle guarantee:
    /// delegates to [`Portfolio::close_position`] so all cash/PnL/history
    /// invariants are preserved.
    pub fn close_position_by_pair(
        &mut self,
        pair: &str,
        reason: CloseReason,
    ) -> Option<ClosedTrade> {
        let (position_id, exit_price) = self
            .positions
            .iter()
            .find(|p| p.pair.eq_ignore_ascii_case(pair))
            .map(|p| (p.id, p.current_price))?;

        self.close_position(position_id, exit_price, reason)
    }

    /// Gets a position by ID.
    pub fn get_position(&self, id: Uuid) -> Option<&Position> {
        self.positions.iter().find(|p| p.id == id)
    }

    /// Gets a mutable position by ID.
    pub fn get_position_mut(&mut self, id: Uuid) -> Option<&mut Position> {
        self.positions.iter_mut().find(|p| p.id == id)
    }

    /// Gets a position by trading pair.
    pub fn get_position_by_pair(&self, pair: &str) -> Option<&Position> {
        self.positions.iter().find(|p| p.pair == pair)
    }

    /// Checks if there's an open position for the given pair.
    pub fn has_position(&self, pair: &str) -> bool {
        self.positions.iter().any(|p| p.pair == pair)
    }

    /// Returns the number of open positions.
    pub fn open_position_count(&self) -> usize {
        self.positions.len()
    }

    /// Resets daily realized PnL (call at start of new day).
    pub fn reset_daily_pnl(&mut self) {
        self.daily_realized_pnl = Decimal::ZERO;
    }

    /// Calculates portfolio performance metrics.
    pub fn calculate_metrics(&self) -> PortfolioMetrics {
        let total_trades = self.trade_history.len();
        if total_trades == 0 {
            return PortfolioMetrics::default();
        }

        let winners: Vec<_> = self
            .trade_history
            .iter()
            .filter(|t| t.is_winner())
            .collect();
        let losers: Vec<_> = self
            .trade_history
            .iter()
            .filter(|t| !t.is_winner())
            .collect();

        let win_rate = if total_trades > 0 {
            Decimal::from(winners.len()) / Decimal::from(total_trades) * Decimal::from(100)
        } else {
            Decimal::ZERO
        };

        let gross_profit: Decimal = winners.iter().map(|t| t.realized_pnl).sum();
        let gross_loss: Decimal = losers.iter().map(|t| t.realized_pnl.abs()).sum();

        let profit_factor = if gross_loss > Decimal::ZERO {
            gross_profit / gross_loss
        } else if gross_profit > Decimal::ZERO {
            Decimal::from(999) // Infinite profit factor capped
        } else {
            Decimal::ONE
        };

        let avg_win = if !winners.is_empty() {
            gross_profit / Decimal::from(winners.len())
        } else {
            Decimal::ZERO
        };

        let avg_loss = if !losers.is_empty() {
            gross_loss / Decimal::from(losers.len())
        } else {
            Decimal::ZERO
        };

        PortfolioMetrics {
            total_trades,
            winning_trades: winners.len(),
            losing_trades: losers.len(),
            win_rate,
            profit_factor,
            gross_profit,
            gross_loss,
            net_profit: self.total_realized_pnl,
            avg_win,
            avg_loss,
            max_drawdown_pct: self.current_drawdown_pct, // Simplified - should track historical max
        }
    }

    /// Returns the total value of all open positions.
    pub fn total_position_value(&self) -> Decimal {
        self.invested
    }

    /// Returns total unrealized P/L across all positions.
    pub fn total_unrealized_pnl(&self) -> Decimal {
        self.unrealized_pnl
    }

    /// Returns total P/L as a percentage.
    pub fn total_pnl_pct(&self) -> Decimal {
        let total = self.total_value();
        let initial = total - self.total_realized_pnl - self.unrealized_pnl;
        if initial > Decimal::ZERO {
            ((total - initial) / initial) * Decimal::from(100)
        } else {
            Decimal::ZERO
        }
    }

    /// Alias for trade_history to maintain compatibility.
    pub fn closed_trades(&self) -> &[ClosedTrade] {
        &self.trade_history
    }
}

/// Portfolio performance metrics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PortfolioMetrics {
    /// Total number of closed trades.
    pub total_trades: usize,
    /// Number of winning trades.
    pub winning_trades: usize,
    /// Number of losing trades.
    pub losing_trades: usize,
    /// Win rate as percentage.
    pub win_rate: Decimal,
    /// Ratio of gross profit to gross loss.
    pub profit_factor: Decimal,
    /// Total profit from winning trades.
    pub gross_profit: Decimal,
    /// Total loss from losing trades (absolute value).
    pub gross_loss: Decimal,
    /// Net profit/loss.
    pub net_profit: Decimal,
    /// Average profit per winning trade.
    pub avg_win: Decimal,
    /// Average loss per losing trade.
    pub avg_loss: Decimal,
    /// Maximum drawdown seen.
    pub max_drawdown_pct: Decimal,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_pnl_long() {
        let mut pos = Position::new(
            "BTCUSDT".to_string(),
            PositionSide::Long,
            Decimal::from(50000),
            Decimal::new(1, 1), // 0.1 BTC
            Decimal::from(49000),
            Decimal::from(52000),
            75,
        );

        // Price goes up
        pos.update_price(Decimal::from(51000));
        assert!(pos.unrealized_pnl > Decimal::ZERO);

        // Price goes down
        pos.update_price(Decimal::from(49500));
        assert!(pos.unrealized_pnl < Decimal::ZERO);
    }

    #[test]
    fn test_position_pnl_short() {
        let mut pos = Position::new(
            "BTCUSDT".to_string(),
            PositionSide::Short,
            Decimal::from(50000),
            Decimal::new(1, 1), // 0.1 BTC
            Decimal::from(51000),
            Decimal::from(48000),
            75,
        );

        // Price goes down (profit for short)
        pos.update_price(Decimal::from(49000));
        assert!(pos.unrealized_pnl > Decimal::ZERO);

        // Price goes up (loss for short)
        pos.update_price(Decimal::from(50500));
        assert!(pos.unrealized_pnl < Decimal::ZERO);
    }

    #[test]
    fn test_stop_loss_trigger() {
        let mut pos = Position::new(
            "BTCUSDT".to_string(),
            PositionSide::Long,
            Decimal::from(50000),
            Decimal::new(1, 1),
            Decimal::from(49000), // SL at 49000
            Decimal::from(52000),
            75,
        );

        pos.update_price(Decimal::from(49500));
        assert!(!pos.is_stop_loss_hit());

        pos.update_price(Decimal::from(49000));
        assert!(pos.is_stop_loss_hit());
    }

    #[test]
    fn test_portfolio_open_close() {
        let mut portfolio = Portfolio::new(Decimal::from(10000), "USDT".to_string());

        let position = Position::new(
            "BTCUSDT".to_string(),
            PositionSide::Long,
            Decimal::from(50000),
            Decimal::new(1, 1), // 0.1 BTC = 5000 USDT
            Decimal::from(49000),
            Decimal::from(52000),
            75,
        );

        let pos_id = position.id;
        portfolio.open_position(position).unwrap();

        assert_eq!(portfolio.open_position_count(), 1);
        assert!(portfolio.cash < Decimal::from(10000));

        // Close with profit
        let closed = portfolio
            .close_position(pos_id, Decimal::from(51000), CloseReason::TakeProfit)
            .unwrap();

        assert!(closed.is_winner());
        assert_eq!(portfolio.open_position_count(), 0);
        assert!(portfolio.cash > Decimal::from(10000));
    }

    #[test]
    fn test_insufficient_balance() {
        let mut portfolio = Portfolio::new(Decimal::from(1000), "USDT".to_string());

        let position = Position::new(
            "BTCUSDT".to_string(),
            PositionSide::Long,
            Decimal::from(50000),
            Decimal::new(1, 1), // 5000 USDT needed
            Decimal::from(49000),
            Decimal::from(52000),
            75,
        );

        let result = portfolio.open_position(position);
        assert!(result.is_err());
    }

    #[test]
    fn test_open_position_rejects_negative_notional_inputs() {
        let mut portfolio = Portfolio::new(Decimal::from(10000), "USDT".to_string());
        let position = Position::new(
            "BTCUSDT".to_string(),
            PositionSide::Long,
            Decimal::from(50000),
            Decimal::new(-1, 1),
            Decimal::from(49000),
            Decimal::from(52000),
            75,
        );

        let result = portfolio.open_position(position);
        assert!(result.is_err());
    }

    #[test]
    fn test_open_position_rejects_invalid_long_risk_levels() {
        let mut portfolio = Portfolio::new(Decimal::from(10000), "USDT".to_string());
        let position = Position::new(
            "BTCUSDT".to_string(),
            PositionSide::Long,
            Decimal::from(50000),
            Decimal::new(1, 1),
            Decimal::from(51000),
            Decimal::from(52000),
            75,
        );

        let result = portfolio.open_position(position);
        assert!(result.is_err());
    }

    #[test]
    fn test_open_position_rejects_invalid_short_risk_levels() {
        let mut portfolio = Portfolio::new(Decimal::from(10000), "USDT".to_string());
        let position = Position::new(
            "BTCUSDT".to_string(),
            PositionSide::Short,
            Decimal::from(50000),
            Decimal::new(1, 1),
            Decimal::from(49000),
            Decimal::from(48000),
            75,
        );

        let result = portfolio.open_position(position);
        assert!(result.is_err());
    }

    #[test]
    fn test_close_position_rejects_non_positive_exit_price() {
        let mut portfolio = Portfolio::new(Decimal::from(10000), "USDT".to_string());
        let position = Position::new(
            "BTCUSDT".to_string(),
            PositionSide::Long,
            Decimal::from(50000),
            Decimal::new(1, 1),
            Decimal::from(49000),
            Decimal::from(52000),
            75,
        );

        let pos_id = position.id;
        portfolio.open_position(position).expect("open position");
        let closed = portfolio.close_position(pos_id, Decimal::ZERO, CloseReason::Manual);
        assert!(closed.is_none());
        assert_eq!(portfolio.open_position_count(), 1);
    }

    #[test]
    fn test_recalculate_resets_drawdown_on_new_peak() {
        let mut portfolio = Portfolio::new(Decimal::from(10000), "USDT".to_string());
        portfolio.peak_value = Decimal::from(12000);
        portfolio.current_drawdown_pct = Decimal::from(10);
        portfolio.cash = Decimal::from(13000);

        portfolio.recalculate();

        assert_eq!(portfolio.peak_value, Decimal::from(13000));
        assert_eq!(portfolio.current_drawdown_pct, Decimal::ZERO);
    }
}
