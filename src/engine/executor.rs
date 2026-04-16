//! Risk-aware order execution orchestrator for paper trading.
//!
//! Performs pre-execution risk checks before submitting orders:
//! - Signal confidence threshold
//! - Maximum concurrent trades
//! - Portfolio drawdown guard

use crate::paper::executor::{MarketOrder, OrderExecutor};
use crate::state::AppState;
use rust_decimal::Decimal;

/// Risk check result indicating pass/fail with reason.
#[derive(Debug, Clone)]
pub struct RiskCheckResult {
    pub passed: bool,
    pub reason: String,
}

/// Risk parameters for order execution.
#[derive(Debug, Clone, Copy)]
pub struct RiskParams {
    /// Minimum signal confidence to enter (0-100)
    pub min_confidence: u8,
    /// Maximum concurrent open positions
    pub max_open_trades: usize,
    /// Maximum portfolio drawdown allowed (0.0 - 1.0)
    pub max_drawdown: Decimal,
    /// Position size as percentage of balance (0.0 - 1.0)
    pub position_size_pct: Decimal,
}

impl Default for RiskParams {
    fn default() -> Self {
        RiskParams {
            min_confidence: 60,
            max_open_trades: 5,
            max_drawdown: Decimal::new(20, 2),
            position_size_pct: Decimal::new(5, 2),
        }
    }
}

/// Executes orders with pre-flight risk checks.
pub struct RiskAwareExecutor;

impl RiskAwareExecutor {
    /// Check if signal confidence meets minimum threshold.
    pub fn check_confidence(confidence: u8, min_confidence: u8) -> RiskCheckResult {
        if confidence >= min_confidence {
            RiskCheckResult {
                passed: true,
                reason: format!(
                    "Confidence {}% meets threshold {}%",
                    confidence, min_confidence
                ),
            }
        } else {
            RiskCheckResult {
                passed: false,
                reason: format!(
                    "Confidence {}% below threshold {}%",
                    confidence, min_confidence
                ),
            }
        }
    }

    /// Check if adding new trade would exceed max concurrent trades.
    pub fn check_max_trades(current_trades: usize, max_trades: usize) -> RiskCheckResult {
        if current_trades < max_trades {
            RiskCheckResult {
                passed: true,
                reason: format!(
                    "Open trades ({}/{}) within limit",
                    current_trades, max_trades
                ),
            }
        } else {
            RiskCheckResult {
                passed: false,
                reason: format!("Max concurrent trades ({}) already reached", max_trades),
            }
        }
    }

    /// Check if portfolio drawdown is within acceptable limits.
    pub fn check_drawdown(
        current_balance: Decimal,
        peak_balance: Decimal,
        max_drawdown: Decimal,
    ) -> RiskCheckResult {
        if peak_balance <= Decimal::ZERO {
            return RiskCheckResult {
                passed: false,
                reason: "Invalid peak balance".to_string(),
            };
        }

        let drawdown = (peak_balance - current_balance) / peak_balance;

        if drawdown <= max_drawdown {
            RiskCheckResult {
                passed: true,
                reason: format!(
                    "Drawdown {:.2}% within limit {:.2}%",
                    drawdown * Decimal::new(100, 0),
                    max_drawdown * Decimal::new(100, 0)
                ),
            }
        } else {
            RiskCheckResult {
                passed: false,
                reason: format!(
                    "Drawdown {:.2}% exceeds limit {:.2}%",
                    drawdown * Decimal::new(100, 0),
                    max_drawdown * Decimal::new(100, 0)
                ),
            }
        }
    }

    /// Calculate position size based on portfolio percentage.
    #[must_use = "handle invalid sizing inputs"]
    pub fn calculate_position_size(
        portfolio_balance: Decimal,
        current_price: Decimal,
        position_size_pct: Decimal,
    ) -> crate::error::Result<Decimal> {
        if current_price <= Decimal::ZERO {
            return Err(crate::error::MycryptoError::ConfigValidation(
                "Current price must be positive".to_string(),
            ));
        }

        if position_size_pct <= Decimal::ZERO || position_size_pct > Decimal::ONE {
            return Err(crate::error::MycryptoError::ConfigValidation(
                "Position size percentage must be between 0 and 1".to_string(),
            ));
        }

        let position_value = portfolio_balance * position_size_pct;
        let quantity = position_value / current_price;

        Ok(quantity)
    }

    /// Run all risk checks before order execution.
    pub fn run_all_checks(
        state: &AppState,
        confidence: u8,
        params: &RiskParams,
    ) -> Vec<RiskCheckResult> {
        let mut results = vec![];

        // Confidence check
        results.push(Self::check_confidence(confidence, params.min_confidence));

        // Max trades check
        let open_count = state.portfolio.positions.len();
        results.push(Self::check_max_trades(open_count, params.max_open_trades));

        // Drawdown check
        results.push(Self::check_drawdown(
            state.portfolio.total_value(),
            state.portfolio.peak_value,
            params.max_drawdown,
        ));

        results
    }

    /// Check if all risk checks pass.
    pub fn all_checks_pass(results: &[RiskCheckResult]) -> bool {
        results.iter().all(|r| r.passed)
    }

    /// Execute an order with risk checks. Returns position if all checks pass.
    #[must_use = "handle failed risk checks before execution"]
    pub fn execute_with_checks(
        state: &mut AppState,
        order: MarketOrder,
        confidence: u8,
        params: &RiskParams,
    ) -> crate::error::Result<crate::state::Position> {
        // Run all checks
        let checks = Self::run_all_checks(state, confidence, params);

        // If any check fails, return error
        if !Self::all_checks_pass(&checks) {
            let reasons: Vec<String> = checks
                .iter()
                .filter(|r| !r.passed)
                .map(|r| r.reason.clone())
                .collect();
            return Err(crate::error::MycryptoError::ConfigValidation(
                reasons.join("; "),
            ));
        }

        // Execute the order
        OrderExecutor::execute_order(state, order)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_confidence_pass() {
        let result = RiskAwareExecutor::check_confidence(70, 60);
        assert!(result.passed);
    }

    #[test]
    fn test_check_confidence_fail() {
        let result = RiskAwareExecutor::check_confidence(50, 60);
        assert!(!result.passed);
    }

    #[test]
    fn test_check_max_trades_pass() {
        let result = RiskAwareExecutor::check_max_trades(3, 5);
        assert!(result.passed);
    }

    #[test]
    fn test_check_max_trades_fail() {
        let result = RiskAwareExecutor::check_max_trades(5, 5);
        assert!(!result.passed);
    }

    #[test]
    fn test_check_drawdown_pass() {
        let result = RiskAwareExecutor::check_drawdown(
            Decimal::new(8000, 0),
            Decimal::new(10000, 0),
            Decimal::new(20, 2),
        );
        assert!(result.passed);
    }

    #[test]
    fn test_check_drawdown_fail() {
        let result = RiskAwareExecutor::check_drawdown(
            Decimal::new(7500, 0),
            Decimal::new(10000, 0),
            Decimal::new(20, 2),
        );
        assert!(!result.passed);
    }

    #[test]
    fn test_calculate_position_size() {
        let size = RiskAwareExecutor::calculate_position_size(
            Decimal::new(10000, 0),
            Decimal::new(50000, 0),
            Decimal::new(5, 2),
        );

        assert!(size.is_ok());
        assert_eq!(size.unwrap(), Decimal::new(1, 2));
    }

    #[test]
    fn test_calculate_position_size_invalid_price() {
        let size = RiskAwareExecutor::calculate_position_size(
            Decimal::new(10000, 0),
            Decimal::ZERO,
            Decimal::new(5, 2),
        );

        assert!(size.is_err());
    }

    #[test]
    fn test_all_checks_pass() {
        let results = vec![
            RiskCheckResult {
                passed: true,
                reason: "A".to_string(),
            },
            RiskCheckResult {
                passed: true,
                reason: "B".to_string(),
            },
        ];

        assert!(RiskAwareExecutor::all_checks_pass(&results));
    }

    #[test]
    fn test_all_checks_fail() {
        let results = vec![
            RiskCheckResult {
                passed: true,
                reason: "A".to_string(),
            },
            RiskCheckResult {
                passed: false,
                reason: "B".to_string(),
            },
        ];

        assert!(!RiskAwareExecutor::all_checks_pass(&results));
    }

    #[test]
    fn test_run_all_checks_includes_drawdown_guard() {
        let mut state = AppState::new(crate::config::Config::default());
        state.portfolio.cash = Decimal::new(70, 0);
        state.portfolio.peak_value = Decimal::new(100, 0);

        let params = RiskParams {
            min_confidence: 10,
            max_open_trades: 10,
            max_drawdown: Decimal::new(20, 2),
            position_size_pct: Decimal::new(5, 2),
        };

        let results = RiskAwareExecutor::run_all_checks(&state, 100, &params);
        assert_eq!(results.len(), 3);
        assert!(results
            .iter()
            .any(|r| !r.passed && r.reason.contains("Drawdown")));
    }
}
