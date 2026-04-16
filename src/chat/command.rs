//! Command execution from detected intents.
//!
//! Executes parsed commands against the application state.

use rust_decimal::Decimal;
use tracing::{info, warn};

use crate::config::AgentStatus;
use crate::state::{AppState, CloseReason};

use super::intent::DetectedIntent;

/// Result of executing a command.
#[derive(Debug, Clone)]
pub struct CommandResult {
    /// Whether the command succeeded.
    pub success: bool,
    /// Human-readable message about the result.
    pub message: String,
}

impl CommandResult {
    /// Create a success result.
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
        }
    }

    /// Create a failure result.
    pub fn failure(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
        }
    }
}

/// Execute a detected intent against the application state.
pub fn execute_intent(state: &mut AppState, intent: &DetectedIntent) -> CommandResult {
    info!("Executing intent: {} {:?}", intent.command, intent.argument);

    match intent.command.as_str() {
        "pause" => execute_pause(state),
        "resume" => execute_resume(state),
        "status" => execute_status(state),
        "portfolio" => CommandResult::success("Switching to portfolio view"),
        "signals" => CommandResult::success("Switching to signals view"),
        "close" => execute_close(state, intent.argument.as_deref()),
        "risk" => execute_risk(state, intent.argument.as_deref()),
        "confidence" => execute_confidence(state, intent.argument.as_deref()),
        _ => {
            warn!("Unknown command: {}", intent.command);
            CommandResult::failure(format!("Unknown command: {}", intent.command))
        }
    }
}

/// Pause the trading agent.
fn execute_pause(state: &mut AppState) -> CommandResult {
    if state.agent_status == AgentStatus::Paused {
        return CommandResult::success("Agent is already paused");
    }

    state.agent_status = AgentStatus::Paused;
    info!("Agent paused via chat command");
    CommandResult::success("Agent paused. No new signals will be generated.")
}

/// Resume the trading agent.
fn execute_resume(state: &mut AppState) -> CommandResult {
    if state.agent_status == AgentStatus::Running {
        return CommandResult::success("Agent is already running");
    }

    state.agent_status = AgentStatus::Running;
    info!("Agent resumed via chat command");
    CommandResult::success("Agent resumed. Signal generation is now active.")
}

/// Get current status.
fn execute_status(state: &AppState) -> CommandResult {
    let status = format!(
        "Agent: {}, Portfolio: {} {}, Open positions: {}, Total P/L: {:+.2}%",
        state.agent_status,
        state.portfolio.cash + state.portfolio.total_position_value(),
        state.config.portfolio.currency,
        state.portfolio.positions.len(),
        state.portfolio.total_pnl_pct()
    );
    CommandResult::success(status)
}

/// Close a position.
fn execute_close(state: &mut AppState, pair: Option<&str>) -> CommandResult {
    let pair = match pair {
        Some(p) => p.to_uppercase(),
        None => return CommandResult::failure("No pair specified. Use: close BTCUSDT"),
    };

    match state
        .portfolio
        .close_position_by_pair(&pair, CloseReason::Manual)
    {
        Some(closed) => {
            info!(
                "Position {} closed via chat command, P/L: {:+.2}%",
                pair, closed.realized_pnl_pct
            );
            CommandResult::success(format!(
                "Closed {} position. P/L: {:+.2}% ({:+.2} {})",
                pair, closed.realized_pnl_pct, closed.realized_pnl, state.config.portfolio.currency
            ))
        }
        None => CommandResult::failure(format!("No open position for {}", pair)),
    }
}

/// Update risk per trade.
fn execute_risk(state: &mut AppState, value: Option<&str>) -> CommandResult {
    let value = match value {
        Some(v) => v,
        None => return CommandResult::failure("No value specified. Use: risk 2.5"),
    };

    match value.parse::<f64>() {
        Ok(pct) if pct > 0.0 && pct <= 100.0 => {
            let decimal = Decimal::try_from(pct).unwrap_or(Decimal::from(2));
            state.config.risk.risk_per_trade_pct = decimal;
            info!("Risk per trade updated to {}%", pct);
            CommandResult::success(format!("Risk per trade set to {}%", pct))
        }
        Ok(_) => CommandResult::failure("Risk must be between 0 and 100%"),
        Err(_) => CommandResult::failure(format!("Invalid number: {}", value)),
    }
}

/// Update minimum confidence.
fn execute_confidence(state: &mut AppState, value: Option<&str>) -> CommandResult {
    let value = match value {
        Some(v) => v,
        None => return CommandResult::failure("No value specified. Use: confidence 75"),
    };

    match value.parse::<u8>() {
        Ok(conf) if conf <= 100 => {
            state.config.agent.min_confidence = conf;
            info!("Minimum confidence updated to {}%", conf);
            CommandResult::success(format!("Minimum confidence set to {}%", conf))
        }
        Ok(_) => CommandResult::failure("Confidence must be between 0 and 100"),
        Err(_) => CommandResult::failure(format!("Invalid number: {}", value)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn test_execute_pause() {
        let mut state = AppState::new(Config::default());
        state.agent_status = AgentStatus::Running;

        let intent = DetectedIntent::new("pause", None, "[COMMAND:pause]");
        let result = execute_intent(&mut state, &intent);

        assert!(result.success);
        assert_eq!(state.agent_status, AgentStatus::Paused);
    }

    #[test]
    fn test_execute_resume() {
        let mut state = AppState::new(Config::default());
        state.agent_status = AgentStatus::Paused;

        let intent = DetectedIntent::new("resume", None, "[COMMAND:resume]");
        let result = execute_intent(&mut state, &intent);

        assert!(result.success);
        assert_eq!(state.agent_status, AgentStatus::Running);
    }

    #[test]
    fn test_execute_status() {
        let mut state = AppState::new(Config::default());
        let intent = DetectedIntent::new("status", None, "[COMMAND:status]");
        let result = execute_intent(&mut state, &intent);

        assert!(result.success);
        assert!(result.message.contains("Agent:"));
    }

    #[test]
    fn test_execute_confidence() {
        let mut state = AppState::new(Config::default());

        let intent = DetectedIntent::new(
            "confidence",
            Some("80".to_string()),
            "[COMMAND:confidence 80]",
        );
        let result = execute_intent(&mut state, &intent);

        assert!(result.success);
        assert_eq!(state.config.agent.min_confidence, 80);
    }

    #[test]
    fn test_execute_risk() {
        let mut state = AppState::new(Config::default());

        let intent = DetectedIntent::new("risk", Some("2.5".to_string()), "[COMMAND:risk 2.5]");
        let result = execute_intent(&mut state, &intent);

        assert!(result.success);
        assert_eq!(state.config.risk.risk_per_trade_pct, Decimal::new(25, 1));
    }

    #[test]
    fn test_execute_close_no_position() {
        let mut state = AppState::new(Config::default());

        let intent = DetectedIntent::new(
            "close",
            Some("BTCUSDT".to_string()),
            "[COMMAND:close BTCUSDT]",
        );
        let result = execute_intent(&mut state, &intent);

        assert!(!result.success);
        assert!(result.message.contains("No open position"));
    }

    #[test]
    fn test_execute_unknown_command() {
        let mut state = AppState::new(Config::default());

        let intent = DetectedIntent::new("unknown", None, "[COMMAND:unknown]");
        let result = execute_intent(&mut state, &intent);

        assert!(!result.success);
        assert!(result.message.contains("Unknown command"));
    }
}
